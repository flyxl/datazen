//! 执行侧目标词汇：`ExecutionTarget` 与 `ObjectTarget`。
//!
//! 原属 platform-api `target.rs`，D3（2026-10-26）起唯一定义在
//! `datazen-modules::execution::domain`（§5.1：`ExecutionTarget` 组合连接侧
//! namespace，而不是让 connection 反向引用 execution）。
//!
//! `ObjectTarget`（表对象身份）按 §5.1 最终归 schema::domain（「表对象目标由
//! schema 定义并在应用边界转换」）；D4 落位 schema 时迁移并在此处保留再导出。
//! 规范化目标（`CanonicalTarget` / `CanonicalNamespace`）与驱动能力元数据
//! （shape/case/requirements）仍在 platform-api，随执行机制迁移（D3 后续切片）。

use serde::{Deserialize, Serialize};

use crate::connection::domain::NamespaceTarget;
use crate::shared::id::ConnectionId;

/// 对象身份。必须传完整身份，**不能把未指定 schema 猜成 `public`/`dbo`**（§4.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectTarget {
    pub kind: String,
    pub name: String,
    /// 签名（如 table|view），无法确定时为 `null`。
    pub signature: Option<String>,
}

impl ObjectTarget {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
            signature: None,
        }
    }

    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }
}

/// 请求提交的目标：`connectionId` + 命名空间 + 可选对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTarget {
    pub connection_id: ConnectionId,
    pub namespace: NamespaceTarget,
    pub object: Option<ObjectTarget>,
}

impl ExecutionTarget {
    pub fn new(connection_id: ConnectionId, namespace: NamespaceTarget) -> Self {
        Self {
            connection_id,
            namespace,
            object: None,
        }
    }

    pub fn with_object(mut self, object: ObjectTarget) -> Self {
        self.object = Some(object);
        self
    }

    /// 提交态命名空间的稳定指纹（未规范化）。规范化后的身份一律用
    /// 规范化目标的 `namespace_fingerprint`（platform-api 过渡期再导出），
    /// 两者不可混用。
    pub fn namespace_fingerprint(&self) -> String {
        self.namespace.namespace_fingerprint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn execution_target_round_trips_with_and_without_object() {
        let bare = ExecutionTarget::new(
            ConnectionId::new("conn-1"),
            NamespaceTarget::database("app"),
        );
        let value = serde_json::to_value(&bare).expect("serialize");
        assert_eq!(value["object"], json!(null));
        assert_eq!(value["namespace"]["database"], json!("app"));
        let back: ExecutionTarget = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, bare);

        let with_object = bare
            .clone()
            .with_object(ObjectTarget::new("table", "users").with_signature("table"));
        let value = serde_json::to_value(&with_object).expect("serialize");
        assert_eq!(value["object"]["signature"], json!("table"));
        assert_eq!(
            serde_json::from_value::<ExecutionTarget>(value).expect("deserialize"),
            with_object
        );
    }

    #[test]
    fn namespace_fingerprint_is_the_submitted_not_canonical_one() {
        let target = ExecutionTarget::new(
            ConnectionId::new("conn-1"),
            NamespaceTarget::new(Some("app".into()), None, Some("public".into()), vec![]),
        );
        assert_eq!(target.namespace_fingerprint(), "app//public");
    }
}
