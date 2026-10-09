//! 会话额度：逻辑与物理分开记账、排空、节点额度租约。
//!
//! 这一半的账本实现了全部约束：
//!
//! - **逻辑与物理是两套上限**。`New` 状态没有 socket，但计入逻辑上限（单用户 100 / 单组织 1000）；
//!   物理连接另有一套上限（桌面 16 / team 20 是总量口径，物理侧由配置给出）。
//! - **回落到空闲池不释放物理额度**。`transition` 只换状态、不动计数；只有真正
//!   `close_session` 才回收。否则同一份预算会被一份还活着的 socket 重复出售。
//! - **排空不抢占**。`drain` 只停止接受新的 permit，并报出手上还有多少；已建立的固定会话、
//!   活跃事务、游标一个都不动。

use datazen_platform_api::id::{ConnectionId, DbSessionId, OrganizationId, PrincipalId, WorkerId};
use datazen_platform_api::ports::budget::{
    BudgetSnapshot, DrainScope, DrainStatus, NodeLease, PhysicalOccupancy,
};

use crate::budget::ledger::{BudgetLedger, DenialReason, DrainReport, SessionScope};
use crate::budget::records::{counter, mono_timestamp, SessionRecord};

impl BudgetLedger {
    // ------------------------------------------------------------ 逻辑会话

    /// 开一个**逻辑** session（`New` 状态）：没有 socket，但计入逻辑上限。
    ///
    /// 单用户与单组织两个上限**都**要查，先查用户后查组织，拒哪个由先触发的那个决定。
    pub fn open_session(
        &mut self,
        organization_id: &OrganizationId,
        principal: &PrincipalId,
        connection_id: &ConnectionId,
    ) -> Result<DbSessionId, DenialReason> {
        let user_cap = self.config.per_user_logical_sessions;
        let user_used = self.user_logical.get(principal).copied().unwrap_or(0);
        if user_used >= user_cap {
            return Err(DenialReason::SessionQuotaExceeded {
                scope: SessionScope::PerUser,
                cap: user_cap,
                used: user_used,
            });
        }
        let org_cap = self.config.per_org_logical_sessions;
        let org_used = self.org_logical.get(organization_id).copied().unwrap_or(0);
        if org_used >= org_cap {
            return Err(DenialReason::SessionQuotaExceeded {
                scope: SessionScope::PerOrganization,
                cap: org_cap,
                used: org_used,
            });
        }
        self.ensure_service(connection_id);
        self.next_session = self.next_session.saturating_add(1);
        let ticket = DbSessionId::new(format!("dbs-{:010}", self.next_session));
        self.sessions.insert(
            ticket.clone(),
            SessionRecord {
                ticket: ticket.clone(),
                organization_id: organization_id.clone(),
                principal: principal.clone(),
                connection_id: connection_id.clone(),
                occupancy: None,
            },
        );
        self.user_logical.insert(principal.clone(), user_used + 1);
        self.org_logical
            .insert(organization_id.clone(), org_used + 1);
        Ok(ticket)
    }

    /// 某个 session 的记录。
    pub fn session(&self, ticket: &DbSessionId) -> Option<&SessionRecord> {
        self.sessions.get(ticket)
    }

    /// 单用户逻辑 session 数。
    pub fn user_logical(&self, principal: &PrincipalId) -> u32 {
        self.user_logical.get(principal).copied().unwrap_or(0)
    }

    /// 单组织逻辑 session 数。
    pub fn org_logical(&self, organization_id: &OrganizationId) -> u32 {
        self.org_logical.get(organization_id).copied().unwrap_or(0)
    }

    /// 物理连接占用。
    pub fn physical(&self, connection_id: &ConnectionId) -> u32 {
        crate::budget::records::physical_of(self, connection_id)
    }

    /// 单用户在该服务上的已连接数（「已连接编辑器」口径）。
    pub fn user_connected(&self, connection_id: &ConnectionId, principal: &PrincipalId) -> u32 {
        crate::budget::records::connected_of(self, connection_id, principal)
    }

    // ------------------------------------------------------------ 物理会话

    /// 给逻辑 session 挂上物理连接。物理额度与逻辑额度**各有一套上限**，这里管物理。
    ///
    /// 已挂过物理的 session 重复调用是幂等的，不会重复计数。
    pub fn attach_physical(
        &mut self,
        ticket: &DbSessionId,
        occupancy: PhysicalOccupancy,
    ) -> Result<(), DenialReason> {
        let Some(record) = self.sessions.get(ticket).cloned() else {
            return Err(DenialReason::UnknownConnection {
                connection_id: ConnectionId::new(ticket.as_str()),
            });
        };
        if record.occupancy.is_some() {
            return Ok(());
        }
        let cap = self.config.service_physical_connections;
        let physical = crate::budget::records::physical_of(self, &record.connection_id);
        if physical >= cap {
            return Err(DenialReason::PhysicalExhausted {
                connection_id: record.connection_id.clone(),
                cap,
            });
        }
        let editor_cap = self.config.per_user_connected_editors;
        let connected =
            crate::budget::records::connected_of(self, &record.connection_id, &record.principal);
        if connected >= editor_cap {
            return Err(DenialReason::ConnectedEditorsExhausted {
                principal: record.principal.clone(),
                cap: editor_cap,
            });
        }
        if let Some(service) = self.services.get_mut(&record.connection_id) {
            service.physical = service.physical.saturating_add(1);
            *service
                .user_connected
                .entry(record.principal.clone())
                .or_insert(0) += 1;
        }
        if let Some(entry) = self.sessions.get_mut(ticket) {
            entry.occupancy = Some(occupancy);
        }
        Ok(())
    }

    /// 切换物理占用状态，**不改变任何计数**。
    ///
    /// 回落到空闲池（`IdlePooled`）不释放物理额度，只有真正 close 才释放。
    /// 如果这里顺手减了计数，同一份预算就会被一份还活着的 socket 重复出售。
    pub fn transition(
        &mut self,
        ticket: &DbSessionId,
        occupancy: PhysicalOccupancy,
    ) -> Result<(), DenialReason> {
        let Some(record) = self.sessions.get(ticket).cloned() else {
            return Err(DenialReason::UnknownConnection {
                connection_id: ConnectionId::new(ticket.as_str()),
            });
        };
        // 只有已经挂上物理的 session 才有占用状态；`New` 不接受状态跃迁。
        if record.occupancy.is_some() {
            if let Some(entry) = self.sessions.get_mut(ticket) {
                entry.occupancy = Some(occupancy);
            }
        }
        Ok(())
    }

    /// 真正 close：同时释放物理额度与逻辑额度。
    pub fn close_session(&mut self, ticket: &DbSessionId) -> Result<(), DenialReason> {
        let Some(record) = self.sessions.remove(ticket) else {
            return Ok(());
        };
        if record.occupancy.is_some() {
            if let Some(service) = self.services.get_mut(&record.connection_id) {
                service.physical = service.physical.saturating_sub(1);
                if let Some(connected) = service.user_connected.get_mut(&record.principal) {
                    *connected = connected.saturating_sub(1);
                }
            }
        }
        if let Some(cell) = self.user_logical.get_mut(&record.principal) {
            *cell = cell.saturating_sub(1);
        }
        if let Some(cell) = self.org_logical.get_mut(&record.organization_id) {
            *cell = cell.saturating_sub(1);
        }
        Ok(())
    }

    // ------------------------------------------------------------ 排空

    /// 按作用域排空（节点 / 组织 / 连接）。
    ///
    /// 排空 = 停止接受新的 permit，并报出还在手上多少。**不抢占**：已建立的固定会话、
    /// 活跃事务、游标一个都不动。
    pub fn drain(&mut self, scope: DrainScope) -> DrainReport {
        match &scope {
            DrainScope::Connection(connection_id) => {
                if let Some(service) = self.services.get_mut(connection_id) {
                    service.draining = true;
                }
            }
            DrainScope::Organization(organization_id) => {
                self.draining_orgs.insert(organization_id.clone());
                for service in self.services.values_mut() {
                    service.draining = true;
                }
            }
            DrainScope::Node(worker_id) => {
                self.draining_nodes.insert(worker_id.clone());
            }
        }
        let (mut outstanding, mut pinned) = (0usize, 0usize);
        for record in self.permits.values() {
            let in_scope = match &scope {
                DrainScope::Connection(connection_id) => &record.connection_id == connection_id,
                DrainScope::Organization(organization_id) => {
                    &record.organization_id == organization_id
                }
                DrainScope::Node(worker_id) => record.worker.as_ref() == Some(worker_id),
            };
            if !in_scope {
                continue;
            }
            if record.pinned {
                pinned += 1;
            } else {
                outstanding += 1;
            }
        }
        DrainReport {
            scope,
            outstanding,
            pinned,
            draining: true,
        }
    }

    /// 排空结果投影成端口的 `DrainStatus`。
    pub fn drain_status(report: DrainReport) -> DrainStatus {
        DrainStatus {
            scope: report.scope,
            outstanding: report.outstanding,
            draining: report.draining,
        }
    }

    /// 某节点是否已被标记排空。
    pub fn node_is_draining(&self, worker_id: &WorkerId) -> bool {
        self.draining_nodes.contains(worker_id)
    }

    /// 某组织是否已被标记排空。
    pub fn org_is_draining(&self, organization_id: &OrganizationId) -> bool {
        self.draining_orgs.contains(organization_id)
    }

    // ------------------------------------------------------------ 节点额度

    /// 续期节点额度租约。
    ///
    /// 排空中的节点续期**失败**并按失联处理：调用方据此停止发放新资源。
    /// 旧额度在 worker 隔离或连接关闭确认之前**不回收**，所以成功路径不缩容。
    pub fn renew_node_lease(
        &mut self,
        lease: &NodeLease,
        now_ms: u64,
    ) -> Result<NodeLease, DenialReason> {
        let worker_id = &lease.worker_id;
        if self.draining_nodes.contains(worker_id) {
            return Err(DenialReason::NodeLost {
                worker_id: worker_id.clone(),
            });
        }
        let expires_at_ms = now_ms.saturating_add(self.config.node_lease_ttl_ms);
        self.node_leases.insert(worker_id.clone(), expires_at_ms);
        Ok(NodeLease {
            lease_id: lease.lease_id.clone(),
            worker_id: worker_id.clone(),
            expires_at: mono_timestamp(expires_at_ms),
        })
    }

    /// 节点租约当前到期时刻（单调毫秒）。
    pub fn node_lease_expiry(&self, worker_id: &WorkerId) -> Option<u64> {
        self.node_leases.get(worker_id).copied()
    }

    // ------------------------------------------------------------ 快照

    /// 快照。`in_use` 是**该组织**的在手 permit 数，`granted_total` 是全局累计。
    pub fn snapshot(&self, organization_id: &OrganizationId) -> BudgetSnapshot {
        let in_use = self
            .permits
            .values()
            .filter(|record| &record.organization_id == organization_id)
            .count();
        BudgetSnapshot {
            organization_id: organization_id.clone(),
            in_use: counter(in_use),
            granted_total: counter(self.granted_total as usize),
        }
    }
}
