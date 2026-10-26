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
use std::collections::BTreeMap;

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

/// 本次请求所使用的命名空间形状：声明**哪些层必填**、哪些层存在但可省。
///
/// The driver API has its own `NamespaceShape` because it also owns driver validation behavior.
/// The host maps every declared level, path segment, case rule, canonical-ID rule, and alias into
/// this transport type before the application resolver runs.
///
/// `packages/runtime` 不持有副本：它通过 `pub use datazen_platform_api::target::TargetNamespaceShape`
/// 把这一份直接暴露出去，因此平台侧与 runtime 侧永远编译成同一个类型。
///
/// The runtime re-exports this type directly. `required` stays mandatory in serialized requests;
/// optional metadata fields default so older payloads remain readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetNamespaceShape {
    pub required: Vec<TargetNamespaceLayer>,
    #[serde(default)]
    pub optional: Vec<TargetNamespaceLayer>,
    #[serde(default)]
    pub path_segments: Vec<TargetNamespacePathSegment>,
    #[serde(default)]
    pub case_rules: TargetCaseRules,
    #[serde(default)]
    pub canonical_id_rules: TargetCanonicalIdRules,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
}

/// Driver 声明的规范 namespace identity segment 位置和语义。`position` 从零开始。
/// 这是 identity 组成元数据，不会把数据库层级自动转换成额外的请求 `path` 层。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetNamespacePathSegment {
    pub position: usize,
    pub kind: TargetNamespaceLayer,
    pub meaning: String,
}

/// 标识符大小写折叠规则。`Unknown` 保留调用方写法，不自行猜测数据库规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetCaseFolding {
    Lowercases,
    Preserved,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetCaseRules {
    #[serde(default)]
    pub unquoted: TargetCaseFolding,
    #[serde(default)]
    pub quoted: TargetCaseFolding,
}

/// Driver 生成规范身份时声明的比较规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetCanonicalIdRules {
    pub case_sensitive: bool,
    pub delimiter: String,
}

impl Default for TargetCanonicalIdRules {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            delimiter: ".".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TargetCanonicalizationError {
    #[error("namespace alias chain is cyclic")]
    AliasConflict,
}

impl TargetNamespaceShape {
    pub fn new(
        required: impl IntoIterator<Item = TargetNamespaceLayer>,
        optional: impl IntoIterator<Item = TargetNamespaceLayer>,
    ) -> Self {
        Self {
            required: required.into_iter().collect(),
            optional: optional.into_iter().collect(),
            path_segments: Vec::new(),
            case_rules: TargetCaseRules::default(),
            canonical_id_rules: TargetCanonicalIdRules::default(),
            aliases: BTreeMap::new(),
        }
    }

    /// PostgreSQL 式：database 必填，schema 可省。
    pub fn database_and_schema() -> Self {
        Self::new(
            [TargetNamespaceLayer::Database],
            [TargetNamespaceLayer::Schema],
        )
    }

    /// 该层级是否存在（required 或 optional 任一命中即存在）。
    ///
    /// 不存在的层级**只能是 null**（§4.3 步骤 2）。
    pub fn declares(&self, layer: TargetNamespaceLayer) -> bool {
        self.required.contains(&layer) || self.optional.contains(&layer)
    }

    pub fn is_required(&self, layer: TargetNamespaceLayer) -> bool {
        self.required.contains(&layer)
    }

    /// Resolve an identifier using the driver's aliases and case rules. Quoted identifiers keep
    /// the driver's quoted-case semantics, including doubled quotes.
    pub fn canonicalize_identifier(
        &self,
        raw: &str,
    ) -> Result<String, TargetCanonicalizationError> {
        let quoted = raw
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'));
        let rule = if quoted.is_some() {
            self.case_rules.quoted
        } else {
            self.case_rules.unquoted
        };
        let fold = |value: &str| {
            let bare = value
                .strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))
                .unwrap_or(value)
                .replace("\"\"", "\"");
            match rule {
                TargetCaseFolding::Lowercases => bare.to_lowercase(),
                TargetCaseFolding::Preserved | TargetCaseFolding::Unknown => bare,
            }
        };

        let mut current = fold(raw);
        for _ in 0..=self.aliases.len() {
            match self.aliases.get(&current) {
                Some(next) if *next != current => current = next.clone(),
                _ => return Ok(fold(&current)),
            }
        }
        Err(TargetCanonicalizationError::AliasConflict)
    }
}

/// 单个层级在本操作中的要求（§4.3 `targetRequirements`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerRequirement {
    /// 必须提供非 null 值。
    Required,
    /// 可以缺省；一旦提供必须非空。**`Default` 取这一档**：`TargetRequirements::default()`
    /// 因此不对命名空间形状作任何要求，也就不会掩盖错误。
    #[default]
    Optional,
    /// 必须为 null，出现即拒绝。
    Forbidden,
}

impl LayerRequirement {
    pub fn accepts_value(self) -> bool {
        !matches!(self, Self::Forbidden)
    }
}

/// Command definition 注册的操作级目标要求。
///
/// 与 [`TargetNamespaceShape`] **必须同时校验**，不能用一个全驱动 required 列表代替操作需求（§4.3）。
///
/// `Default` is a permissive namespace-only rule. Registered drivers provide concrete values per
/// command; an absent command requirement is treated as unsupported by the application resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRequirements {
    pub database: LayerRequirement,
    pub catalog: LayerRequirement,
    pub schema: LayerRequirement,
    pub path: LayerRequirement,
    /// 本操作是否接受对象身份。对象操作必须传完整身份，不得把未指定 schema 猜成 `public`/`dbo`。
    pub allows_object: bool,
    /// 本操作是否必须提供完整对象身份。
    #[serde(default)]
    pub requires_object: bool,
    /// 是否允许使用该 session **已确认**的默认值。`executeInSession` 只有显式允许时才能用。
    pub allows_session_defaults: bool,
}

impl TargetRequirements {
    /// 单一目标操作：指定的两个层级分别必填/可省，允许对象。
    pub fn single_target(
        required: TargetNamespaceLayer,
        optional: TargetNamespaceLayer,
        allows_object: bool,
    ) -> Self {
        let mut requirements = Self {
            allows_object,
            ..Self::default()
        };
        requirements.set(required, LayerRequirement::Required);
        requirements.set(optional, LayerRequirement::Optional);
        requirements
    }

    pub fn set(&mut self, layer: TargetNamespaceLayer, requirement: LayerRequirement) -> &mut Self {
        let slot = match layer {
            TargetNamespaceLayer::Database => &mut self.database,
            TargetNamespaceLayer::Catalog => &mut self.catalog,
            TargetNamespaceLayer::Schema => &mut self.schema,
            TargetNamespaceLayer::Path => &mut self.path,
        };
        *slot = requirement;
        self
    }

    pub fn requirement_for(&self, layer: TargetNamespaceLayer) -> LayerRequirement {
        match layer {
            TargetNamespaceLayer::Database => self.database,
            TargetNamespaceLayer::Catalog => self.catalog,
            TargetNamespaceLayer::Schema => self.schema,
            TargetNamespaceLayer::Path => self.path,
        }
    }

    /// 是否允许用该 session 已确认的值补齐目标（§4.3：`executeInSession` 的唯一例外来源）。
    pub fn session_defaults_allowed(&self) -> bool {
        self.allows_session_defaults
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
    #[test]
    fn namespace_shape_declares_layers() {
        let shape = TargetNamespaceShape::database_and_schema();
        assert!(shape.is_required(TargetNamespaceLayer::Database));
        assert!(!shape.is_required(TargetNamespaceLayer::Schema));
        assert!(shape.declares(TargetNamespaceLayer::Schema));
        assert!(
            !shape.declares(TargetNamespaceLayer::Catalog),
            "未声明的层级不存在，只能是 null"
        );
        assert!(!shape.declares(TargetNamespaceLayer::Path));
    }

    #[test]
    fn namespace_shape_pins_its_wire_shape_as_literals() {
        // 全部断言都是**绝对字面量**，不与 runtime 的同名类型互相印证 ——
        // runtime 现在是 `pub use` 本类型，两边对比已无意义，且两套定义曾经就靠对比互相放过。
        let shape = TargetNamespaceShape::new(
            [TargetNamespaceLayer::Database],
            [TargetNamespaceLayer::Schema],
        );
        assert_eq!(
            serde_json::to_value(&shape).expect("serialize"),
            json!({
                "required": ["database"],
                "optional": ["schema"],
                "pathSegments": [],
                "caseRules": {"unquoted": "unknown", "quoted": "unknown"},
                "canonicalIdRules": {"caseSensitive": false, "delimiter": "."},
                "aliases": {}
            })
        );

        // 往返：把上面那份字面量**再读回来**并要求完全相等。
        // 旧测试只读 serialize 一侧（`get(key).is_some()` + `len()==2`），
        // 对「同一份字面量能不能被解析回来」零覆盖 —— 而那是缺陷真正藏身的另一半。
        let back: TargetNamespaceShape =
            serde_json::from_value(json!({"required": ["database"], "optional": ["schema"]}))
                .expect("线上字面量必须能被解析回来");
        assert_eq!(back, shape, "序列化与反序列化必须是同一个值");

        // 层级字面量同样逐字钉住（`as_str()` 不得偏离 `rename_all = "camelCase"`）。
        assert_eq!(
            TargetNamespaceLayer::ALL
                .map(TargetNamespaceLayer::as_str)
                .to_vec(),
            ["database", "catalog", "schema", "path"]
        );

        let pg = TargetNamespaceShape::database_and_schema();
        assert_eq!(pg.required, vec![TargetNamespaceLayer::Database]);
        assert_eq!(pg.optional, vec![TargetNamespaceLayer::Schema]);
        assert!(pg.is_required(TargetNamespaceLayer::Database));
        assert!(!pg.is_required(TargetNamespaceLayer::Schema));
    }

    #[test]
    fn namespace_shape_accepts_a_missing_optional_key() {
        // 钉住 `#[serde(default)]` 决定的线上接受域：`optional` 省略必须仍能反序列化。
        // 这条正是删掉 `#[serde(default)]` 时唯一会变红的断言 —— 而那正是本文件注释
        // 记录过的、**无收益的破坏性收窄**（today runtime 能收，删掉就不能收）。
        let parsed: TargetNamespaceShape =
            serde_json::from_str(r#"{"required":["database"]}"#).expect("optional 省略必须被接受");
        assert_eq!(parsed.required, vec![TargetNamespaceLayer::Database]);
        assert_eq!(parsed.optional, Vec::<TargetNamespaceLayer>::new());

        // 反面：`required` 没有 `default`，省略它必须仍然被拒。
        assert!(
            serde_json::from_str::<TargetNamespaceShape>(r#"{"optional":["schema"]}"#).is_err(),
            "省略必填层必须被拒，放过它才是真缺陷"
        );
    }

    #[test]
    fn target_requirements_address_each_layer() {
        let mut requirements = TargetRequirements::single_target(
            TargetNamespaceLayer::Database,
            TargetNamespaceLayer::Schema,
            true,
        );
        requirements.set(TargetNamespaceLayer::Catalog, LayerRequirement::Forbidden);

        assert_eq!(
            requirements.requirement_for(TargetNamespaceLayer::Database),
            LayerRequirement::Required
        );
        assert_eq!(
            requirements.requirement_for(TargetNamespaceLayer::Schema),
            LayerRequirement::Optional
        );
        assert_eq!(
            requirements.requirement_for(TargetNamespaceLayer::Catalog),
            LayerRequirement::Forbidden
        );
        assert_eq!(
            requirements.requirement_for(TargetNamespaceLayer::Path),
            LayerRequirement::Optional
        );
        assert!(!LayerRequirement::Forbidden.accepts_value());
        assert!(LayerRequirement::Required.accepts_value());
    }

}
