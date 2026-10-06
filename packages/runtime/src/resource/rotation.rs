//! 轮换、禁用/删除、排队三类**生命周期编排**。
//!
//! - 轮换：凭据 / 路由 / ACL 轮换后旧代空闲不再签发并立刻关闭；
//!   缓存回填按代次比对，旧代慢结果**不得**回填新一代。
//! - 禁用/删除：新申请与排队申请不再执行，已在跑的执行按**真实**取消处置记账。
//! - 排队：获取超时（10s）到期后交还 `ResourceBusy` 的上游由网关决定。
//!
//! 这些都是 `ResourceManager` 的方法，但关注的是「代次与归属」而不是「单条租约」，
//! 因此独立成文件以保持单个职责清晰。

use crate::connection::{ConfigRevision, ConnectionId};
use crate::resource::cleanup::{CancellationOutcome, ExecutionCancellationRecord};
use crate::resource::generation::{RotationKind, RotationOutcome};
use crate::resource::lease::{LeaseRequest, LeaseState};
use crate::resource::table::{DisableOutcome, QueueDrain, QueuedLease, RotationReport};
use crate::resource::{ResourceError, NANOS_PER_SECOND};

impl super::manager::ResourceManager {
    // ---- 轮换 ----

    /// 凭据 / 路由 / ACL 轮换：旧代空闲**不再签发**并立刻关闭，新材料才签发新资源。
    pub fn rotate(
        &mut self,
        kind: RotationKind,
        request: &LeaseRequest,
    ) -> Result<RotationReport, ResourceError> {
        let connection_id = request.pool_key_inputs.connection_id.clone();
        let base_key = self
            .generations
            .current(&connection_id)
            .map(|current| current.pool_key.clone());
        let mut new_pool_key = request.pool_key();
        // 轮换只前进不后退：申请没声明代号（`None`）就沿用当前代，声明了就必须不小于当前代。
        // 没有这一步，一次「没带代号」的轮换会把代次**退回** 0，等于把新材料悄悄改回旧材料。
        if let Some(base) = &base_key {
            new_pool_key.credential_revision = request
                .credential_revision
                .unwrap_or(base.credential_revision)
                .max(base.credential_revision);
            new_pool_key.network_route_revision = request
                .network_route_revision
                .unwrap_or(base.network_route_revision)
                .max(base.network_route_revision);
        }
        let base = self.generations.current_config_revision(&connection_id);
        let config_revision = match kind {
            RotationKind::AccessControl => ConfigRevision::new(base.map_or(1, |r| r.get() + 1)),
            _ => base.unwrap_or(ConfigRevision::new(1)),
        };
        let revision =
            self.generations
                .rotate(&connection_id, new_pool_key.clone(), config_revision);
        let outcome = RotationOutcome {
            kind,
            from: revision.pool_key.clone(),
            to: new_pool_key.clone(),
            cache_generation: revision.generation,
        };
        // 旧代的空闲一律摘出并关闭（失败则进隔离）。
        let mut retired_idles = Vec::new();
        let mut quarantined = Vec::new();
        let stale = self
            .table
            .idle_leases_for_connection(&connection_id, &new_pool_key);
        for lease_id in stale {
            retired_idles.push(lease_id.clone());
            if self.force_close(&lease_id).is_err() {
                quarantined.push(lease_id);
            }
        }
        Ok(RotationReport {
            outcome,
            retired_idles,
            quarantined,
        })
    }

    // ---- 禁用/删除 ----

    /// 禁用或删除一份连接配置：新申请与排队申请**不再执行**，
    /// 已经在跑的执行按**真实**取消处置记账。
    pub fn disable(
        &mut self,
        connection_id: &ConnectionId,
    ) -> Result<DisableOutcome, ResourceError> {
        self.disabled.insert(connection_id.clone());

        // 1) 排队中的申请直接丢弃：它们从未被执行。
        let mut dropped_requests = 0usize;
        self.queue.retain(|queued| {
            let keep = queued.request.pool_key_inputs.connection_id != *connection_id;
            if !keep {
                dropped_requests += 1;
            }
            keep
        });

        // 2) 空闲租约立刻关闭。
        let mut closed_idle = Vec::new();
        let now_nanos = self.now_nanos();
        let idle = self
            .table
            .all_leases()
            .into_iter()
            .filter(|record| &record.connection_id == connection_id && record.idle_for_issue);
        for record in idle {
            match self.force_close(&record.lease_id) {
                Ok(_) => closed_idle.push(record.lease_id),
                Err(_) => tracing::warn!(
                    lease_id = record.lease_id.as_str(),
                    "idle close failed during disable; the lease stays quarantined"
                ),
            }
        }

        // 3) 已经在跑的执行：如实记录取消处置。
        let mut cancellations = Vec::new();
        let mut quarantined = Vec::new();
        let busy = self.table.all_leases().into_iter().filter(|record| {
            &record.connection_id == connection_id && record.active_execution.is_some()
        });
        for record in busy {
            let Some(execution_id) = record.active_execution.clone() else {
                continue;
            };
            let disposition = self
                .transport
                .cancel(&record.resource_id, &execution_id)
                .map_err(|error| error.logged("cancel an in-flight execution during disable"));
            let outcome = CancellationOutcome::observed(disposition, now_nanos);
            let failed = matches!(outcome, CancellationOutcome::Failed { .. });
            cancellations.push(ExecutionCancellationRecord {
                execution_id,
                resource_id: record.resource_id.clone(),
                outcome,
            });
            if failed {
                quarantined.push(record.lease_id.clone());
                if let Some(entry) = self.table.lease_mut(&record.lease_id) {
                    let _ = entry.move_to(LeaseState::Quarantined);
                }
            }
        }

        Ok(DisableOutcome {
            connection_id: connection_id.clone(),
            dropped_requests,
            closed_idle,
            cancellations,
            quarantined,
        })
    }

    /// 重新启用（撤销禁用）。已隔离的租约**不会**被自动复活。
    pub fn enable(&mut self, connection_id: &ConnectionId) {
        self.disabled.shift_remove(connection_id);
    }

    // ---- 排队（获取超时） ----

    pub fn enqueue(&mut self, request: LeaseRequest) -> Result<QueuedLease, ResourceError> {
        let connection_id = request.pool_key_inputs.connection_id.clone();
        if self.disabled.contains(&connection_id) {
            return Err(
                ResourceError::OwnerDisabled(connection_id.as_str().to_owned())
                    .logged("enqueue on a disabled connection"),
            );
        }
        let now_nanos = self.now_nanos();
        let deadline_nanos = now_nanos.saturating_add(
            request
                .acquire_timeout_ms
                .saturating_mul(1_000_000)
                .min(u64::MAX / NANOS_PER_SECOND)
                .saturating_mul(NANOS_PER_SECOND)
                / 1_000_000,
        );
        let queued = QueuedLease {
            request,
            enqueued_at_nanos: now_nanos,
            deadline_nanos,
        };
        self.queue.push(queued.clone());
        Ok(queued)
    }

    /// 排空排队区：区分「可继续」「已超时」「归属已禁用」。
    pub fn drain_queue(&mut self) -> QueueDrain {
        let now_nanos = self.now_nanos();
        let drained = std::mem::take(&mut self.queue);
        let mut drain = QueueDrain {
            ready: Vec::new(),
            expired: Vec::new(),
            suppressed: Vec::new(),
        };
        for queued in drained {
            if self
                .disabled
                .contains(&queued.request.pool_key_inputs.connection_id)
            {
                drain.suppressed.push(queued.request);
            } else if now_nanos >= queued.deadline_nanos {
                drain.expired.push(queued.request);
            } else {
                drain.ready.push(queued.request);
            }
        }
        drain
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }
}
