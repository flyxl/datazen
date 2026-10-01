//! 产物契约：`ProfileView` 与连接模块的回执集合。
//!
//! §4.1 的字段校验：列表**不返回** password、token、TLS 私钥或任何可直接解密材料；
//! `publicOptions` 只含可见且非敏感字段。凭据内容只存 `SecretProvider`（§4.2）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dto::session::{CancelReceipt, CloseReceipt, ContextChangeReceipt, OpenSessionReceipt};
use crate::id::{ConnectionId, Counter, DbSessionId, ExecutionId, StageId, StreamId, Timestamp};
use crate::target::{ExecutionTarget, NamespaceTarget};

/// 连接配置视图。**不含** `secretRef`、`networkRouteRef`、明文密码等敏感字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub connection_id: ConnectionId,
    pub name: String,
    pub driver_id: String,
    /// 配置版本。变更时该连接上所有会话被判失效（租约仍保持固定，INV-02）。
    pub config_revision: Counter,
    pub credential_revision: Counter,
    pub initial_namespace: NamespaceTarget,
    /// 可见且非敏感的配置项。
    pub public_options: BTreeMap<String, serde_json::Value>,
    /// 只表示「是否已配置凭据」，**不泄露**是否存在、长度或内容。
    pub credential_configured: bool,
    pub enabled: bool,
}

impl ProfileView {
    pub fn new(
        connection_id: ConnectionId,
        name: impl Into<String>,
        driver_id: impl Into<String>,
    ) -> Self {
        Self {
            connection_id,
            name: name.into(),
            driver_id: driver_id.into(),
            config_revision: Counter::ZERO,
            credential_revision: Counter::ZERO,
            initial_namespace: NamespaceTarget::default(),
            public_options: BTreeMap::new(),
            credential_configured: false,
            enabled: true,
        }
    }

    pub fn with_config_revision(mut self, revision: Counter) -> Self {
        self.config_revision = revision;
        self
    }

    pub fn with_credential_revision(mut self, revision: Counter) -> Self {
        self.credential_revision = revision;
        self
    }

    pub fn with_initial_namespace(mut self, namespace: NamespaceTarget) -> Self {
        self.initial_namespace = namespace;
        self
    }

    pub fn with_public_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.public_options.insert(key.into(), value);
        self
    }

    pub fn with_credential_configured(mut self) -> Self {
        self.credential_configured = true;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// 新建连接的配置草稿。`credentials` 是**只写**字段：
/// 落进 `SecretProvider` 后永不回显，也不进 `ProfileView`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraft {
    pub name: String,
    pub driver_id: String,
    pub initial_namespace: NamespaceTarget,
    pub public_options: BTreeMap<String, serde_json::Value>,
    /// 只写。序列化出去的草稿即视为已提交，不得再回写视图。
    pub credentials: Option<BTreeMap<String, String>>,
}

impl ProfileDraft {
    pub fn new(name: impl Into<String>, driver_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            driver_id: driver_id.into(),
            initial_namespace: NamespaceTarget::default(),
            public_options: BTreeMap::new(),
            credentials: None,
        }
    }

    pub fn with_initial_namespace(mut self, namespace: NamespaceTarget) -> Self {
        self.initial_namespace = namespace;
        self
    }

    pub fn with_public_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.public_options.insert(key.into(), value);
        self
    }

    /// 草稿的凭据是否已填写。
    pub fn has_credentials(&self) -> bool {
        self.credentials.as_ref().is_some_and(|c| !c.is_empty())
    }

    /// 清除凭据，用于把草稿转成可回显的部分。
    ///
    /// 用例在返回任何响应前必须先调用它，避免凭据经 `Debug`/序列化泄漏到日志或事件。
    pub fn without_credentials(mut self) -> Self {
        self.credentials = None;
        self
    }
}

/// 配置补丁。`driverId` 不可改：driver 决定 namespaceShape 与能力，
/// 换 driver 等价于换一条配置，不做原地修改。
pub type ProfilePatch = ProfileDraftPatch;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraftPatch {
    pub name: Option<String>,
    pub initial_namespace: Option<NamespaceTarget>,
    pub public_options: Option<BTreeMap<String, serde_json::Value>>,
    pub credentials: Option<BTreeMap<String, String>>,
}

impl ProfileDraftPatch {
    /// 补丁里是否带了敏感字段。`SecretProvider` 需要凭据时才轮换。
    pub fn touches_credentials(&self) -> bool {
        self.credentials.as_ref().is_some_and(|c| !c.is_empty())
    }
}

/// 结果块到达事件携带的来源标签。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultChunkEvent {
    pub artifact_id: crate::id::ArtifactId,
    pub chunk_index: Counter,
    pub source: crate::dto::execution::StatementResultSource,
}

/// 事件载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionEvent {
    /// 会话变化（状态、上下文、attachment）。
    SessionChanged {
        session: crate::dto::session::SessionView,
    },
    /// 执行状态变化。
    ExecutionChanged {
        execution: crate::dto::execution::ExecutionView,
    },
    /// 结果块到达。
    ResultChunk(ResultChunkEvent),
    /// 流已失效，客户端必须重新订阅；**不得**据此推断执行未发生。
    StreamResetRequired { reason: String },
}

impl ConnectionEvent {
    /// 该事件是否需要客户端重新订阅。
    pub fn requires_resubscribe(&self) -> bool {
        matches!(self, Self::StreamResetRequired { .. })
    }
}

/// 创建会话的回执（附件令牌仅发给原 owner）。
pub type ProfileOpenSessionReceipt = OpenSessionReceipt;
/// 上下文变更回执。
pub type ProfileContextChangeReceipt = ContextChangeReceipt;

/// 数据库会话 ID 的等价物：在回执里它以裸 ID 出现而不是 `SessionHandle`，
/// 因为回执要跨进程/跨 owner 传递，不能携带运行时纪元。
pub type ReceiptDbSessionId = DbSessionId;
/// 回执里的执行 ID。
pub type ReceiptExecutionId = ExecutionId;
/// 回执里的阶段 ID。
pub type ReceiptStageId = StageId;
/// 回执里的流 ID。
pub type ReceiptStreamId = StreamId;
/// 回执里的时刻标记。
pub type ReceiptTimestamp = Timestamp;
/// 初始目标在配置视图里的别名。
pub type ProfileInitialTarget = ExecutionTarget;
/// 取消回执在连接模块接口上的别名。
pub type ProfileCancelReceipt = CancelReceipt;
/// 关闭回执在连接模块接口上的别名。
pub type ProfileCloseReceipt = CloseReceipt;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profile_view_round_trips_without_any_secret_field() {
        let view = ProfileView::new(ConnectionId::new("conn-1"), "prod", "postgres")
            .with_config_revision(Counter::new(2))
            .with_credential_revision(Counter::new(5));
        let value = serde_json::to_value(&view).expect("serialize");
        assert_eq!(value["configRevision"], json!("2"));
        assert_eq!(value["credentialRevision"], json!("5"));
        assert_eq!(value["credentialConfigured"], json!(false));
        assert_eq!(value["enabled"], json!(true));

        for forbidden in [
            "secretRef",
            "password",
            "token",
            "privateKey",
            "networkRouteRef",
        ] {
            assert!(value.get(forbidden).is_none(), "视图不得出现 {forbidden}");
        }
        assert_eq!(
            serde_json::from_value::<ProfileView>(value).expect("deserialize"),
            view
        );
    }

    #[test]
    fn profile_view_helpers_are_reachable_without_mutating_callers() {
        let view = ProfileView::new(ConnectionId::new("conn-1"), "prod", "postgres")
            .with_config_revision(Counter::new(1))
            .with_initial_namespace(NamespaceTarget::database("app"))
            .with_public_option("sslMode", json!("require"))
            .with_credential_configured()
            .with_enabled(false);

        assert_eq!(view.initial_namespace.database.as_deref(), Some("app"));
        assert_eq!(view.public_options.get("sslMode"), Some(&json!("require")));
        assert!(view.credential_configured);
        assert!(!view.enabled);
        let value = serde_json::to_value(&view).expect("serialize");
        assert_eq!(
            serde_json::from_value::<ProfileView>(value).expect("deserialize"),
            view
        );
    }

    #[test]
    fn profile_draft_credentials_are_write_only_and_clearable() {
        let draft = ProfileDraft {
            credentials: Some(BTreeMap::from([(
                "password".to_owned(),
                "s3cret".to_owned(),
            )])),
            ..ProfileDraft::new("prod", "postgres")
        };
        assert!(draft.has_credentials());

        let echoed = draft.clone().without_credentials();
        assert!(!echoed.has_credentials());
        let value = serde_json::to_value(&echoed).expect("serialize");
        assert_eq!(value["credentials"], json!(null));
        assert!(serde_json::to_string(&echoed)
            .expect("serialize")
            .find("s3cret")
            .is_none());
    }

    #[test]
    fn profile_draft_round_trips_with_initial_namespace() {
        let draft = ProfileDraft::new("prod", "postgres").with_initial_namespace(
            NamespaceTarget::new(Some("app".into()), None, Some("public".into()), vec![]),
        );
        let value = serde_json::to_value(&draft).expect("serialize");
        assert_eq!(value["initialNamespace"]["schema"], json!("public"));
        assert_eq!(
            serde_json::from_value::<ProfileDraft>(value).expect("deserialize"),
            draft
        );
    }

    #[test]
    fn profile_patch_cannot_change_the_driver() {
        let patch = ProfilePatch {
            name: Some("renamed".into()),
            ..ProfilePatch::default()
        };
        let value = serde_json::to_value(&patch).expect("serialize");
        assert!(value.get("driverId").is_none(), "driverId 不可改");
        assert!(!patch.touches_credentials());
        assert_eq!(
            serde_json::from_value::<ProfilePatch>(value).expect("deserialize"),
            patch
        );
    }

    #[test]
    fn connection_event_uses_the_spec_tag_names() {
        let events = [
            ConnectionEvent::StreamResetRequired {
                reason: "runtime restarted".into(),
            },
            ConnectionEvent::ExecutionChanged {
                execution: crate::dto::execution::ExecutionView {
                    execution_id: ExecutionId::new("exec-1"),
                    state: crate::dto::execution::ExecutionState::Running,
                    effect_outcome: crate::dto::execution::EffectOutcome::NotStarted,
                    provenance: None,
                    artifact_ids: vec![],
                    result_completeness: crate::dto::execution::ResultCompleteness::Pending,
                    truncation_reason: None,
                    error_code: None,
                    runtime_binding: None,
                },
            },
        ];
        let kinds: Vec<String> = events
            .iter()
            .map(|e| {
                serde_json::to_value(e).expect("serialize")["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds, ["streamResetRequired", "executionChanged"]);

        for event in &events {
            let value = serde_json::to_value(event).expect("serialize");
            assert_eq!(
                serde_json::from_value::<ConnectionEvent>(value).expect("deserialize"),
                *event
            );
        }
        assert!(events[0].requires_resubscribe());
        assert!(!events[1].requires_resubscribe());
    }

    #[test]
    fn result_chunk_event_round_trips() {
        let event = ConnectionEvent::ResultChunk(ResultChunkEvent {
            artifact_id: crate::id::ArtifactId::new("art-1"),
            chunk_index: Counter::new(2),
            source: crate::dto::execution::StatementResultSource {
                execution_id: ExecutionId::new("exec-1"),
                statement_index: 0,
                context: crate::dto::session::SessionContext {
                    namespace: NamespaceTarget::database("app"),
                    search_path: None,
                    effective_identity: None,
                    transaction_state: crate::dto::session::TransactionState::None,
                    autocommit: Some(true),
                    confidence: crate::dto::session::ContextConfidence::Confirmed,
                },
                relation: None,
                writable_mapping: crate::dto::execution::WritableMapping::ReadOnly,
            },
        });
        let value = serde_json::to_value(&event).expect("serialize");
        assert_eq!(value["kind"], json!("resultChunk"));
        assert_eq!(value["chunkIndex"], json!("2"));
        assert_eq!(value["source"]["writableMapping"], json!("readOnly"));
        assert_eq!(
            serde_json::from_value::<ConnectionEvent>(value).expect("deserialize"),
            event
        );
    }
}
