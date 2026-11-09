//! `AuditSink` 的服务端适配器（WR-D3 第一刀）：跨形态审计 envelope →
//! `audit_events` 行（v6 迁移）。
//!
//! 三条纪律（web-server-host §10.1 实现矩阵的 `Audit` 行）：
//!
//! 1. **append-only**：本模块对 `audit_events` 只有 INSERT 与 SELECT——
//!    没有 update/delete 代码路径（PostgreSQL 生产化时由应用角色授权再
//!    兜一层：只授 INSERT/SELECT）。源文本钉子测试把这条纪律钉在 CI 上，
//!    改坏 = 测试红。
//! 2. **envelope 是唯一入口**：写入前不做第二次投影，`payload` 已在
//!    `shared::audit::AuditPayload` 的类型层被白名单约束；本层只把
//!    结构化字段序列化成 JSON 投影（`JSONB → TEXT`，v6 迁移的登记口径）。
//! 3. **失败如实上报**：落库失败返回 `PortError::BackendUnavailable`，
//!    由调用方按 web-server-host §3.2 处置（不吞业务响应、触发告警）。
//!
//! 异步边界：端口是同步的（`shared::audit` 模块头的裁决），SQLite 写入
//! 在 [`AdminStore`] 的单连接互斥锁后是亚毫秒操作；宿主若需线程池隔离，
//! 由上层包装（差异只出现在适配器层之外）。因此本实现**同步**实现
//! `AuditSink`——不引 `async_trait` 包装（包装只会把同步 INSERT 假装成
//! async，徒增一次堆分配，不改变任何阻塞事实）。

use std::sync::Arc;

use rusqlite::OptionalExtension;

use datazen::shared::audit::{AuditEvent, AuditSink};
use datazen::shared::error::PortError;

use super::AdminStore;

/// 服务端审计追加写。构造方负责先跑 `AdminStore::migrate`（v6 起有
/// `audit_events`）。
#[derive(Clone)]
pub struct ServerAudit {
    store: Arc<AdminStore>,
}

impl ServerAudit {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    fn backend(source: impl std::fmt::Display) -> PortError {
        PortError::BackendUnavailable(format!("audit append failed: {source}"))
    }

    /// 同步核心：单条 INSERT + 回读自增的 `event_id`（调用方日志可对账）。
    fn sync_record(&self, event: &AuditEvent) -> Result<i64, PortError> {
        let payload_json =
            serde_json::to_string(&event.payload).map_err(|source| Self::backend(source))?;
        let target_json = match &event.target_projection {
            Some(projection) => serde_json::to_string(projection)
                .map(Some)
                .map_err(|source| Self::backend(source))?,
            None => None,
        };
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|source| Self::backend(source))?;
        conn.execute(
            "INSERT INTO audit_events (
                 organization_id, occurred_at, principal_id, principal_kind,
                 delegation_id, client_instance_id, action, connection_id,
                 config_revision, target_projection_json, outcome, error_code,
                 request_id, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                event.organization_id.as_str(),
                event.occurred_at.as_str(),
                id_param(event.principal_id.as_ref()),
                event.principal_kind.as_str(),
                id_param(event.delegation_id.as_ref()),
                id_param(event.client_instance_id.as_ref()),
                event.action,
                id_param(event.connection_id.as_ref()),
                event.config_revision,
                target_json,
                event.outcome.as_str(),
                event.error_code,
                id_param(event.request_id.as_ref()),
                payload_json,
            ],
        )
        .map_err(Self::backend)?;
        // `last_insert_rowid` 在同一连接上读回刚才那行（单写者互斥内，无竞态）。
        Ok(conn.last_insert_rowid())
    }
}

impl AuditSink for ServerAudit {
    /// 同步实现（端口形状见 `shared::audit` 模块头的裁决）：单条 INSERT
    /// 在单写者互斥锁后完成，无 await 点，也不引入 async 包装。
    fn record(&self, event: AuditEvent) -> Result<(), PortError> {
        self.sync_record(&event).map(|_| ())
    }
}

impl ServerAudit {
    /// 只读探查：按组织回读最近事件（`migrate`/运维诊断与钉子测试用）。
    /// append-only 的读半边同样只有 SELECT。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn latest_event_id(&self, organization_id: &str) -> Result<Option<i64>, PortError> {
        let conn = self
            .store
            .lock_for_adapter()
            .map_err(|source| Self::backend(source))?;
        conn.query_row(
            "SELECT MAX(event_id) FROM audit_events WHERE organization_id = ?1",
            [organization_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map(|value| value.flatten())
        .map_err(Self::backend)
    }
}

/// `Option<IdType>` → SQL 参数（ID newtype 无 `Deref`，走 `AsRef<str>`）。
fn id_param(value: Option<&impl AsRef<str>>) -> Option<&str> {
    value.map(AsRef::as_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::shared::audit::{
        AuditCapability, AuditOutcome, AuditPayload, AuditPrincipalKind, AuditStreamRef,
    };
    use datazen::shared::id::{ConnectionId, OrganizationId, PrincipalId, RequestId, Timestamp};

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("in-memory store");
        store.migrate().expect("migrations apply");
        Arc::new(store)
    }

    fn event(action: &str) -> AuditEvent {
        AuditEvent {
            principal_id: Some(PrincipalId::new("user-1")),
            connection_id: Some(ConnectionId::new("conn-1")),
            config_revision: Some(7),
            request_id: Some(RequestId::new("req-1")),
            payload: AuditPayload {
                stream_ref: Some(AuditStreamRef {
                    stream_id: "s-1".into(),
                    sequence: 42,
                }),
                context_revision: Some(3),
                capability: Some(AuditCapability {
                    driver_id: "postgres".into(),
                    driver_version: "16".into(),
                    protocol_version: "1".into(),
                    capability_revision: "2026-11-01".into(),
                }),
                request: None,
            },
            ..AuditEvent::new(
                OrganizationId::new("org-a"),
                AuditPrincipalKind::User,
                action,
                AuditOutcome::Denied,
                Timestamp::new("2026-01-01T00:00:00.000Z"),
            )
        }
    }

    #[test]
    fn record_appends_one_selectable_row_with_the_envelope_projection() {
        let audit = ServerAudit::new(store());
        audit.record(event("csrf_rejected")).expect("append");

        let conn = audit.store.lock_for_adapter().expect("lock");
        let row = conn
            .query_row(
                "SELECT organization_id, occurred_at, principal_id, principal_kind,
                        action, outcome, request_id, payload_json
                 FROM audit_events WHERE organization_id = 'org-a'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .expect("row is selectable");
        drop(conn);

        assert_eq!(row.0, "org-a");
        assert_eq!(row.1, "2026-01-01T00:00:00.000Z");
        assert_eq!(row.2.as_deref(), Some("user-1"));
        assert_eq!(row.3, "user");
        assert_eq!(row.4, "csrf_rejected");
        assert_eq!(row.5, "denied");
        assert_eq!(row.6.as_deref(), Some("req-1"));
        // payload 是白名单键的 JSON 投影（键集合断言在 modules 钉住，
        // 这里钉「落库即该投影」）。
        let payload: serde_json::Value = serde_json::from_str(&row.7).expect("payload is JSON");
        assert_eq!(payload["streamRef"]["sequence"], 42);
        assert_eq!(payload["capability"]["driverId"], "postgres");
    }

    #[test]
    fn event_ids_are_globally_monotonic_across_organizations() {
        let audit = ServerAudit::new(store());
        audit.record(event("a-one")).expect("append 1");
        audit.record(event("a-two")).expect("append 2");

        // 跨组织仍是同一全局序列（§3.7：event_id 是全局单调序列）。
        let mut cross_org = event("b-one");
        cross_org.organization_id = OrganizationId::new("org-b");
        audit.record(cross_org).expect("append 3");

        assert_eq!(audit.latest_event_id("org-a").expect("read"), Some(2));
        assert_eq!(audit.latest_event_id("org-b").expect("read"), Some(3));
    }

    #[test]
    fn schema_rejects_unknown_outcomes_and_kinds() {
        let audit = ServerAudit::new(store());
        // CHECK 是防线不是摆设：越词汇的行必须落不进去（这里从端口侧
        // 构造不出非法枚举，直接对列验证 CHECK 存在性——插入前显式破坏
        // 只在测试里发生）。
        let conn = audit.store.lock_for_adapter().expect("lock");
        let broken = conn.execute(
            "INSERT INTO audit_events (organization_id, occurred_at, principal_kind, action, outcome)
             VALUES ('org-a', '2026-01-01T00:00:00.000Z', 'ghost', 'x', 'denied')",
            [],
        );
        assert!(broken.is_err(), "unknown principal_kind must violate CHECK");
        let broken = conn.execute(
            "INSERT INTO audit_events (organization_id, occurred_at, principal_kind, action, outcome)
             VALUES ('org-a', '2026-01-01T00:00:00.000Z', 'user', 'x', 'rejected')",
            [],
        );
        assert!(broken.is_err(), "unknown outcome must violate CHECK");
    }

    /// append-only 的**源文本钉子**：本模块对 `audit_events` 不存在
    /// UPDATE / DELETE 代码路径（PostgreSQL 生产化由应用角色授权再兜底，
    /// SQLite 首版由本钉子守纪律——改坏即 CI 红）。
    #[test]
    fn source_has_no_mutation_statements_against_audit_events() {
        const SOURCE: &str = include_str!("audit.rs");
        // 禁词由两段拼装：include_str! 扫的是本文件源码，字面量连写会把
        // 本测试自己变成命中项（自指）。
        for verb in ["UPDATE", "DELETE FROM"] {
            let forbidden = format!("{verb} audit_events");
            assert!(
                !SOURCE.contains(&forbidden),
                "audit_events is append-only: `{forbidden}` must not appear"
            );
        }
    }
}
