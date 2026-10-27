//! 命名空间目标词汇：连接侧 namespace 的层级与值形状。
//!
//! 原属 platform-api `target.rs`，D2（2026-10-26）起唯一定义在
//! `datazen-modules::connection::domain`（§5.1：初始 `NamespaceTarget` 归
//! connection::domain）；platform-api 只再导出同一类型，执行侧的
//! `ExecutionTarget` 在 D3 组合连接侧 namespace 时引用本模块。
//!
//! 权威来源：[连接管理 §4](../../../../../docs/architecture/platform/connection-management.md)
//! 与 §4.3 命名空间规范化契约。驱动能力元数据（`TargetNamespaceShape`、
//! 大小写规则、`TargetRequirements`）仍在 platform-api `target.rs`，随
//! schema/execution 迁移（D3/D4）另行归位。

use serde::{Deserialize, Serialize};

/// 命名空间层级。`path` 表示 database/catalog/schema 之外的扩展层级，
/// 承载的是驱动 namespace ID，**不是文件系统路径**（§4.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetNamespaceLayer {
    Database,
    Catalog,
    Schema,
    Path,
}

impl TargetNamespaceLayer {
    /// 全部层级，供遍历与校验使用。
    pub const ALL: [TargetNamespaceLayer; 4] =
        [Self::Database, Self::Catalog, Self::Schema, Self::Path];

    /// 协议字面量，与 `#[serde(rename_all = "camelCase")]` 派生出的线格式逐字一致。
    ///
    /// 用途与 `modules/src/execution/runtime/connection/error.rs` 的 `ApiErrorCode::as_str()` 相同：
    /// 让错误信息里的层级名与实际序列化结果一致，**不是**重新声明一份协议字面量。
    pub const fn as_str(self) -> &'static str {
        match self {
            TargetNamespaceLayer::Database => "database",
            TargetNamespaceLayer::Catalog => "catalog",
            TargetNamespaceLayer::Schema => "schema",
            TargetNamespaceLayer::Path => "path",
        }
    }
}

/// 提交的命名空间目标：各层级的显式值。
///
/// 存在的层级未指定时为 `null`；只有 `optional` 允许留空。
/// `path` 是驱动命名空间 ID，不是文件路径；重复表达同一层级必须一致，否则 `TargetConflict`。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceTarget {
    pub database: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub path: Vec<String>,
}

impl NamespaceTarget {
    pub fn new(
        database: Option<String>,
        catalog: Option<String>,
        schema: Option<String>,
        path: Vec<String>,
    ) -> Self {
        Self {
            database,
            catalog,
            schema,
            path,
        }
    }

    pub fn database(name: impl Into<String>) -> Self {
        Self {
            database: Some(name.into()),
            ..Self::default()
        }
    }

    pub fn get(&self, layer: TargetNamespaceLayer) -> Option<&str> {
        match layer {
            TargetNamespaceLayer::Database => self.database.as_deref(),
            TargetNamespaceLayer::Catalog => self.catalog.as_deref(),
            TargetNamespaceLayer::Schema => self.schema.as_deref(),
            TargetNamespaceLayer::Path => None,
        }
    }

    /// 该层级是否带了非 null 值。`path` 只要非空即视为带了值。
    pub fn has_value(&self, layer: TargetNamespaceLayer) -> bool {
        match layer {
            TargetNamespaceLayer::Path => !self.path.is_empty(),
            other => self.get(other).is_some(),
        }
    }

    /// 写入单个层级。`Path` 在这里是单值形态（写入 `path` 的唯一一项），
    /// 与「`NamespaceTarget.path` 是列表」的形状不冲突：清空即传 `None`。
    pub fn set(&mut self, layer: TargetNamespaceLayer, value: Option<String>) -> &mut Self {
        match layer {
            TargetNamespaceLayer::Database => self.database = value,
            TargetNamespaceLayer::Catalog => self.catalog = value,
            TargetNamespaceLayer::Schema => self.schema = value,
            TargetNamespaceLayer::Path => self.path = value.into_iter().collect(),
        }
        self
    }

    /// 稳定指纹：层次用 `/` 拼接，不存在的层级留空段。
    /// 用途是「同一目标」的比较与 checkpoint 记录，**不是**权限凭据。
    pub fn namespace_fingerprint(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(3 + self.path.len());
        parts.push(self.database.as_deref().unwrap_or_default());
        parts.push(self.catalog.as_deref().unwrap_or_default());
        parts.push(self.schema.as_deref().unwrap_or_default());
        parts.extend(self.path.iter().map(String::as_str));
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn namespace_target_round_trips_camel_case_with_null_layers() {
        let original = NamespaceTarget::new(
            Some("app".into()),
            None,
            Some("public".into()),
            vec!["tenant_a".into(), "tenant_b".into()],
        );
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(
            value,
            json!({
                "database": "app",
                "catalog": null,
                "schema": "public",
                "path": ["tenant_a", "tenant_b"],
            })
        );
        let back: NamespaceTarget = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, original);
    }

    #[test]
    fn namespace_target_path_is_a_list_not_a_string() {
        // runtime 的 `NamespaceTarget.path` 当前是 `String`，与契约的 `readonly string[]` 冲突。
        let value = serde_json::to_value(NamespaceTarget::default()).expect("serialize");
        assert!(value["path"].is_array(), "path 必须是数组，JSON={}", value);
        assert!(value["database"].is_null(), "不存在的层级只能是 null");
    }

    #[test]
    fn has_value_distinguishes_absent_from_empty_path() {
        let target = NamespaceTarget::default();
        assert!(!target.has_value(TargetNamespaceLayer::Database));
        assert!(!target.has_value(TargetNamespaceLayer::Path));

        let with_path = NamespaceTarget::new(None, None, None, vec!["a".into()]);
        assert!(with_path.has_value(TargetNamespaceLayer::Path));
    }

    #[test]
    fn layer_literals_stay_pinned_to_the_wire_format() {
        assert_eq!(
            TargetNamespaceLayer::ALL
                .map(TargetNamespaceLayer::as_str)
                .to_vec(),
            ["database", "catalog", "schema", "path"]
        );
    }

    #[test]
    fn fingerprint_joins_layers_with_fixed_width_semantics() {
        let target = NamespaceTarget::new(
            Some("app".into()),
            None,
            Some("public".into()),
            vec!["a".into()],
        );
        assert_eq!(target.namespace_fingerprint(), "app//public/a");
        assert_eq!(NamespaceTarget::default().namespace_fingerprint(), "//");
    }
}
