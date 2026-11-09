//! 跨形态审计端口：事件 envelope + [`AuditSink`] 端口。
//!
//! 审计是**安全资产**（web-server-host §10.1：桌面只写本地日志，服务端审计落
//! 管理库且应用角色无 UPDATE/DELETE 授权），因此它的**词汇**必须跨形态
//! 唯一：桌面 IPC 宿主与团队 Web 宿主写同一种 envelope，落同一种表投影
//! （[persistence-model §3.7](../../../../../docs/architecture/platform/persistence-model.md)
//! 的 `audit_events`，SQLite/PostgreSQL 适配器各自的落库半边）。
//!
//! # envelope 与表字段的逐列对齐
//!
//! [`AuditEvent`] 的字段即 §3.7 的列（去掉存储侧生成的 `event_id`）；
//! `principal_kind` / `outcome` 的字面值逐字钉在 §3.7 的 CHECK 取值上
//! （见模块内钉子测试）。`payload` 是 §2.4 白名单的**结构化**投影：
//! [`AuditPayload`] 只有四个字段——新增键必须改类型 + 改键集合断言测试，
//! 「对象序列化顺带落库」的通道在类型层面不存在（§2.4 的要求）。第四个
//! 键 `request`（控制面请求上下文：路由 + 来源 Origin）来自 web-server-host
//! §5.3 对 `csrf_rejected` 审计的显式要求（"审计记录 csrf_rejected 并带
//! 上 requestId、来源 Origin 与路由"）；§2.4 的映射表只覆盖事件归档
//! （EventEnvelope → 审计），控制面事件的这个键在本模块登记为白名单。
//!
//! # `AuditSink` 为什么是同步端口
//!
//! 与 [`crate::shared::clock::Clock`] 的裁决同款（§4.9 端口形状惯例），
//! 另有一条仓库内的先例：`tunnel::domain::KnownHostsRepository` 也是同步
//! 端口（宿主 SQLite 单写者、亚毫秒写入）。理由：
//!
//! 1. **依赖图**：`shared` 无 feature 门，`async-trait` 在 modules 里是
//!    可选依赖——把端口做成 `#[async_trait]` 等于把一个 dep 边加进所有
//!    feature 矩阵腿（CI 的单 feature 构建都要付出代价），而审计追加写
//!    是单条 INSERT，不值得。
//! 2. **调用点形状**：审计在错误映射/中间件里触发（如 CSRF 拒绝记
//!    `csrf_rejected`，web-server-host §5.3），这些调用点本身是同步上下文
//!    友好的；宿主若在异步线程消费，由**宿主的适配器**决定是否
//!    `spawn_blocking`（差异只允许出现在适配器层——与 `storage/mod.rs`
//!    的异步边界纪律同一口径）。
//! 3. **失败语义**：返回 `Result<(), PortError>`（`BackendUnavailable`），
//!    调用方按 §3.2 处置——审计写失败不吞掉业务响应，但必须告警。
//!
//! # 禁持久化清单的落实
//!
//! F4（`sessionHandle` 不进审计）、F10（秘密不进审计）由**结构**保证：
//! envelope 没有承载它们的字段（钉子测试断言序列化输出不含这些键）。
//! F14（IdP 令牌）同理。

use serde::{Deserialize, Serialize};

use crate::shared::error::PortError;
use crate::shared::id::{
    ClientInstanceId, ConnectionId, DelegationId, OrganizationId, PrincipalId, RequestId, Timestamp,
};

/// 主体种类。字面值与 §3.7 `ck_audit_principal` 的 CHECK 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditPrincipalKind {
    /// 应用用户（登录会话映射出的 principal）。
    User,
    /// 服务身份（调度以 memberships 登记的 service principal，web-server-host §6.4）。
    Service,
    /// 委托身份（`delegationId` 链路上的实际执行者）。
    Delegation,
}

impl AuditPrincipalKind {
    /// 协议字面量（§3.7 CHECK 取值），存储适配器按它落列。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Service => "service",
            Self::Delegation => "delegation",
        }
    }
}

/// 审计处置。字面值与 §3.7 `ck_audit_outcome` 的 CHECK 逐字一致。
///
/// 与执行终态（`ExecutionState` / `ExecutionErrorCode`）是两个命名空间：
/// 这里说的是「这次**控制请求**被怎么处置」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditOutcome {
    /// 请求被完整执行。
    Succeeded,
    /// 请求失败（副作用未知与否由 `error_code` / payload 佐证）。
    Failed,
    /// 请求在派发前被拒绝（认证、授权、CSRF）。
    Denied,
    /// 结果不可判定（如已派发但回执丢失）。
    Unknown,
}

impl AuditOutcome {
    /// 协议字面量（§3.7 CHECK 取值）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Denied => "denied",
            Self::Unknown => "unknown",
        }
    }
}

/// 非敏感能力版本快照（§2.4 `payload.capability`；CM-72 要求审计含能力
/// 版本、不含凭据）。只有版本号——与 `registry::audit::CapabilityVersions`
/// 同一纪律：凭据、驱动侧句柄一个都不许进来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditCapability {
    pub driver_id: String,
    pub driver_version: String,
    pub protocol_version: String,
    pub capability_revision: String,
}

/// SSE 流引用（§2.4 `payload.streamRef`）。
///
/// `sequence` 是 `EventEnvelope.sequence`（runtime 维护、SSE 重放用），
/// 与 `audit_events.event_id`（存储侧全局单调序列）**互不替代**（§3.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditStreamRef {
    pub stream_id: String,
    /// 传输层不得转 JS `Number`（连接 §13）；i64 原样序列化。
    pub sequence: i64,
}

/// 目标投影（§3.7 `target_projection` 列）：`connectionId` + 归一化
/// namespace 段，**不含路径**（§2.4；与连接 §4.3 的归一化契约同口径——
/// 归一化在调用方完成，本类型只承载结果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditTargetProjection {
    pub connection_id: String,
    /// 归一化 namespace 段序列（库/schema/table 的规范形式）。
    pub namespace: Vec<String>,
}

/// 控制面请求上下文（§5.3：`csrf_rejected` 一类控制事件必须携带路由与
/// 来源 Origin；`requestId` 走 envelope 的同名列，不重复进本结构）。
///
/// 只有路由（path，**不含查询串**——查询串可能携带敏感参数）与 Origin
/// 两个键：审计不能变成请求日志，§2.4 的「自由内容不归档」纪律不变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRequestContext {
    /// 请求路由（path 形式，如 `/api/v1/connections`）。
    pub route: String,
    /// 请求的 `Origin` 头；缺失时缺席（同源 GET 不保证带此头，web-server-host §8.1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// `audit_events.payload` 的**白名单投影**（§2.4：payload 使用 JSON 承载
/// 「已枚举」的白名单字段，键名固定在代码常量中）。
///
/// 四个键、再无其他：`streamRef` / `contextRevision` / `capability` /
/// `request`。`None` 字段序列化时**缺席**（不是 null）——落库的 JSON 键集合等于
/// `Some` 字段的集合，钉子测试对键集合做相等断言。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditPayload {
    /// SSE 流引用（含 sequence）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_ref: Option<AuditStreamRef>,
    /// 上下文版本（审计字段，不参与绑定判断，§2.4）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_revision: Option<i64>,
    /// 非敏感能力版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<AuditCapability>,
    /// 控制面请求上下文（§5.3；见类型文档）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<AuditRequestContext>,
}

impl AuditPayload {
    /// 空白名单（落库为 `{}`）。
    pub fn empty() -> Self {
        Self::default()
    }
}

/// 一条跨形态审计事件（§3.7 `audit_events` 行的 envelope 投影，不含
/// 存储侧生成的 `event_id`）。
///
/// `action` 是开放词表（字符串）：本层只固定两个既有字面量先例——
/// `csrf_rejected`（web-server-host §5.3）与 `identity_field_ignored`（§6.5）；
/// 其余动作随切片登记，envelope 形状不变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    /// 组织（§3.1：所有表以 organization_id 打头；NOT NULL，无组织外事件）。
    pub organization_id: OrganizationId,
    /// 主体。服务级事件可为 `None`（列可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<PrincipalId>,
    pub principal_kind: AuditPrincipalKind,
    /// 委托链路时非空（§6.4：审计必须能定位到人——delegation 与
    /// principal 并存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<DelegationId>,
    /// 客户端实例（定位事件流归属；不是授权依据，概要 §6.1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_instance_id: Option<ClientInstanceId>,
    /// 动作词（开放词表，见类型文档）。
    pub action: String,
    /// 目标连接（无连接语义的事件为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<ConnectionId>,
    /// 事件发生时的配置版本快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_revision: Option<i64>,
    /// 目标投影（归一化，不含路径）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_projection: Option<AuditTargetProjection>,
    pub outcome: AuditOutcome,
    /// 对外错误码字面量。`ApiError.code` 与 `ExecutionErrorCode` 两个命名
    /// 空间的字面量都可能出现在这里（§9.4：审计按真实原因记录），故为
    /// `String` 而非某一侧枚举。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// 请求关联 id（下游凭此对齐日志）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    /// 白名单 payload（§2.4）。
    pub payload: AuditPayload,
    /// 发生时刻（定宽 RFC3339 UTC，`clock::Clock` 的单一形状）。
    pub occurred_at: Timestamp,
}

impl AuditEvent {
    /// 骨架构造：必填字段 + `Denied` 之外的通用缺省。调用方按需覆盖
    /// `Option` 字段（结构体字面量更新语法）。
    pub fn new(
        organization_id: OrganizationId,
        principal_kind: AuditPrincipalKind,
        action: impl Into<String>,
        outcome: AuditOutcome,
        occurred_at: Timestamp,
    ) -> Self {
        Self {
            organization_id,
            principal_id: None,
            principal_kind,
            delegation_id: None,
            client_instance_id: None,
            action: action.into(),
            connection_id: None,
            config_revision: None,
            target_projection: None,
            outcome,
            error_code: None,
            request_id: None,
            payload: AuditPayload::empty(),
            occurred_at,
        }
    }
}

/// 审计追加写端口（§10.1 实现矩阵的 `Audit` 行）。
///
/// **只有追加**：端口没有 update / delete / 查询形状——§3.7 的
/// append-only 纪律在端口层面就不给违规通道（保留期到期由运维按时间
/// 分区整段卸载，不做逐行删除）。实现方（服务端 `ServerAudit`、桌面
/// 日志适配器）各自落库/落日志；审计写失败返回
/// `PortError::BackendUnavailable`，调用方按 §3.2「不吞业务响应、触发
/// 告警」处置。
pub trait AuditSink: Send + Sync + 'static {
    /// 追加一条事件。幂等性不做承诺（每次调用即一条新行）。
    fn record(&self, event: AuditEvent) -> Result<(), PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn occurred_at() -> Timestamp {
        Timestamp::new("2026-01-01T00:00:00.000Z")
    }

    fn envelope() -> AuditEvent {
        AuditEvent {
            principal_id: Some(PrincipalId::new("user-1")),
            delegation_id: Some(DelegationId::new("del-1")),
            client_instance_id: Some(ClientInstanceId::new("client-1")),
            connection_id: Some(ConnectionId::new("conn-1")),
            config_revision: Some(7),
            target_projection: Some(AuditTargetProjection {
                connection_id: "conn-1".into(),
                namespace: vec!["db".into(), "public".into(), "t".into()],
            }),
            error_code: Some("permissionDenied".into()),
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
                OrganizationId::new("org-1"),
                AuditPrincipalKind::User,
                "csrf_rejected",
                AuditOutcome::Denied,
                occurred_at(),
            )
        }
    }

    /// §3.7 CHECK 取值逐字钉住：存储适配器按这两个 `as_str` 落列，
    /// 字面量漂移 = 全部历史审计行不可读。
    #[test]
    fn principal_kind_and_outcome_literals_match_the_schema_check_values() {
        assert_eq!(AuditPrincipalKind::User.as_str(), "user");
        assert_eq!(AuditPrincipalKind::Service.as_str(), "service");
        assert_eq!(AuditPrincipalKind::Delegation.as_str(), "delegation");
        assert_eq!(AuditOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(AuditOutcome::Failed.as_str(), "failed");
        assert_eq!(AuditOutcome::Denied.as_str(), "denied");
        assert_eq!(AuditOutcome::Unknown.as_str(), "unknown");
    }

    /// §2.4：「构造函数的单测对键集合做相等断言——新增键必须显式改测试」。
    /// 全字段 payload 序列化出的键集合必须逐字等于白名单。
    #[test]
    fn payload_serializes_exactly_to_the_whitelisted_key_set() {
        let value = serde_json::to_value(&envelope().payload).expect("payload serializes");
        assert_eq!(
            value,
            json!({
                "streamRef": {"streamId": "s-1", "sequence": 42},
                "contextRevision": 3,
                "capability": {
                    "driverId": "postgres",
                    "driverVersion": "16",
                    "protocolVersion": "1",
                    "capabilityRevision": "2026-11-01",
                },
            })
        );
    }

    /// `None` 字段缺席（不是 null）：落库 JSON 的键集合 = `Some` 字段集合。
    #[test]
    fn absent_payload_fields_do_not_appear_as_null_keys() {
        let value = serde_json::to_value(&AuditPayload::empty()).expect("payload serializes");
        assert_eq!(value, json!({}));
        let thin = AuditPayload {
            context_revision: Some(1),
            ..AuditPayload::empty()
        };
        let value = serde_json::to_value(&thin).expect("payload serializes");
        assert_eq!(value, json!({"contextRevision": 1}));
    }

    /// §5.3 的 `request` 键：只携带 route 与 origin，origin 缺席时不落 null。
    #[test]
    fn the_request_key_carries_only_route_and_origin() {
        let full = AuditPayload {
            request: Some(AuditRequestContext {
                route: "/api/v1/connections".into(),
                origin: Some("https://team.example.com".into()),
            }),
            ..AuditPayload::empty()
        };
        assert_eq!(
            serde_json::to_value(&full).expect("payload serializes"),
            json!({"request": {"route": "/api/v1/connections", "origin": "https://team.example.com"}})
        );
        let thin = AuditPayload {
            request: Some(AuditRequestContext {
                route: "/api/v1/connections".into(),
                origin: None,
            }),
            ..AuditPayload::empty()
        };
        assert_eq!(
            serde_json::to_value(&thin).expect("payload serializes"),
            json!({"request": {"route": "/api/v1/connections"}}),
            "origin 缺席即缺席，不落 null"
        );
    }

    /// F4/F10/F14 的结构化落实：wire 上不存在 sessionHandle / 秘密类键。
    #[test]
    fn envelope_wire_never_carries_forbidden_keys() {
        let value = serde_json::to_value(&envelope()).expect("envelope serializes");
        let object = value.as_object().expect("envelope is a JSON object");
        for forbidden in [
            "sessionHandle",
            "dbSessionId",
            "cancelHandle",
            "secret",
            "token",
            "idToken",
            "accessToken",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "envelope must not carry {forbidden}"
            );
        }
        // 键集合整体也要钉住：新增字段必须显式改本断言（与 payload 同款纪律）。
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected = vec![
            "action",
            "clientInstanceId",
            "configRevision",
            "connectionId",
            "delegationId",
            "errorCode",
            "occurredAt",
            "organizationId",
            "outcome",
            "payload",
            "principalId",
            "principalKind",
            "requestId",
            "targetProjection",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    #[test]
    fn envelope_round_trips_with_camel_case_keys() {
        let original = envelope();
        let back: AuditEvent =
            serde_json::from_value(serde_json::to_value(&original).expect("serialize"))
                .expect("deserialize");
        assert_eq!(back, original);
    }

    /// 端口对象安全：宿主以 `Arc<dyn AuditSink>` 注入，测试替身同一形状。
    #[test]
    fn the_port_is_injectable_as_a_trait_object_and_reports_failures() {
        struct Recording(Mutex<Vec<AuditEvent>>);

        impl AuditSink for Recording {
            fn record(&self, event: AuditEvent) -> Result<(), PortError> {
                self.0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(event);
                Ok(())
            }
        }
        struct Broken;

        impl AuditSink for Broken {
            fn record(&self, _event: AuditEvent) -> Result<(), PortError> {
                Err(PortError::BackendUnavailable("audit store down".into()))
            }
        }

        let recording = std::sync::Arc::new(Recording(Mutex::new(Vec::new())));
        let sink: std::sync::Arc<dyn AuditSink> = recording.clone();
        let event = envelope();
        sink.record(event.clone()).expect("recording sink accepts");

        let recorded = recording
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(recorded.len(), 1, "the port object delivered the event");
        assert_eq!(recorded[0], event);

        let broken: std::sync::Arc<dyn AuditSink> = std::sync::Arc::new(Broken);
        let error = broken
            .record(event)
            .expect_err("broken sink reports the fact");
        assert!(matches!(error, PortError::BackendUnavailable(_)));
    }
}
