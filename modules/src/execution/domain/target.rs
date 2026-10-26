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

use crate::connection::domain::{NamespaceTarget, TargetNamespaceLayer};
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

/// 驱动规范化后的命名空间标识。
///
/// 与 [`NamespaceTarget`] 是**不同类型**：后者是用户/前端提交的展示名，前者是身份。
/// 禁止在缓存键、权限比较或 PoolKey 中使用展示名（§4.3 末段）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanonicalNamespaceId(String);

impl CanonicalNamespaceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for CanonicalNamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 规范化后的命名空间。不存在的层级恒为 `None`。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalNamespace {
    pub database: Option<CanonicalNamespaceId>,
    pub catalog: Option<CanonicalNamespaceId>,
    pub schema: Option<CanonicalNamespaceId>,
    pub path: Vec<CanonicalNamespaceId>,
}

impl CanonicalNamespace {
    pub fn get(&self, layer: TargetNamespaceLayer) -> Option<&CanonicalNamespaceId> {
        match layer {
            TargetNamespaceLayer::Database => self.database.as_ref(),
            TargetNamespaceLayer::Catalog => self.catalog.as_ref(),
            TargetNamespaceLayer::Schema => self.schema.as_ref(),
            TargetNamespaceLayer::Path => None,
        }
    }

    /// 便捷：`schema = public` 且无 database/catalog/path 的最小命名空间。
    pub fn public() -> Self {
        Self {
            database: None,
            catalog: None,
            schema: Some(CanonicalNamespaceId::new("public")),
            path: Vec::new(),
        }
    }

    /// 是否就是上面那个最小 `public` 命名空间（区分大小写，不做别名折叠）。
    pub fn is_public(&self) -> bool {
        self.database.is_none()
            && self.catalog.is_none()
            && self.path.is_empty()
            && self.schema_is("public")
    }

    fn schema_is(&self, expected: &str) -> bool {
        matches!(&self.schema, Some(id) if id.as_str() == expected)
    }
}

/// §4.3 处理顺序的输出：ResourceManager、权限检查、缓存和执行**共用**这一份身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTarget {
    pub connection_id: ConnectionId,
    pub namespace: CanonicalNamespace,
    pub object: Option<ObjectTarget>,
}

impl CanonicalTarget {
    pub fn new(
        connection_id: ConnectionId,
        namespace: CanonicalNamespace,
        object: Option<ObjectTarget>,
    ) -> Self {
        Self {
            connection_id,
            namespace,
            object,
        }
    }

    /// 规范化目标的对象签名，用于结果归属指纹。无对象时为空串。
    pub fn object_signature(&self) -> String {
        match &self.object {
            Some(object) => format!(
                "{}:{}/{}",
                object.kind,
                object.name,
                object.signature.as_deref().unwrap_or_default()
            ),
            None => String::new(),
        }
    }

    /// 规范化命名空间的稳定指纹：`database/catalog/schema/path` 逐段拼接，
    /// 缺失层为空串。用于缓存键、权限比较与 PoolKey（§4.3 末段：禁用展示名）。
    pub fn namespace_fingerprint(&self) -> String {
        let segment = |value: &Option<CanonicalNamespaceId>| {
            value
                .as_ref()
                .map(CanonicalNamespaceId::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let mut parts = vec![
            segment(&self.namespace.database),
            segment(&self.namespace.catalog),
            segment(&self.namespace.schema),
        ];
        parts.extend(self.namespace.path.iter().map(|id| id.as_str().to_string()));
        parts.join("/")
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
