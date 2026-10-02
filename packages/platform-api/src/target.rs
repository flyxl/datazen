//! 目标（target）契约：命名空间、对象、连接与规范化。
//!
//! 权威来源：[连接管理 §4](../../../docs/architecture/platform/connection-management.md) 与
//! [§4.3 命名空间规范化契约](../../../docs/architecture/platform/connection-management.md)。
//! 处理顺序（§4.3）在 `application` 的 `target` 模块实现为纯函数：
//! 字段齐全 → 拒绝不存在层级的非 null 值 → 合并别名并拒绝冲突 → driver 规范化
//! → 校验本操作 `targetRequirements` → 输出 `CanonicalTarget`。
//!
//! ## 为什么 `CanonicalTarget` 与 `ExecutionTarget` 是两个类型
//!
//! `ExecutionTarget` 承载**用户/前端提交的名字**，`CanonicalTarget` 承载**驱动 namespace ID**
//! （大小写折叠、别名归一后的身份）。§4.3 要求「前端展示名称不参与身份比较」，
//! ResourceManager、权限检查、缓存与执行必须使用同一份 `CanonicalTarget`。
//! 用不同类型承载两者，就是让「用展示名做身份比较」在编译期就不可能发生。
//!
//! ## P2 待补
//!
//! `NamespaceShape` 与 `TargetRequirements` 的具体字段、canonical ID 折叠规则、别名表、
//! 大小写策略与 `pathSegments` 顺序语义属 P2（见 shared-boundaries §「本文不覆盖什么」）。
//! 本模块只落地 §4.3 明确枚举的字段，驱动注册数据待 P2 补齐。

use serde::{Deserialize, Serialize};

use crate::id::ConnectionId;

/// 命名空间层级。`path` 表示 database/catalog/schema 之外的扩展层级，
/// 承载的是驱动 namespace ID，**不是文件系统路径**（§4.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceLayer {
    Database,
    Catalog,
    Schema,
    Path,
}

impl NamespaceLayer {
    /// 全部层级，供遍历与校验使用。
    pub const ALL: [NamespaceLayer; 4] = [Self::Database, Self::Catalog, Self::Schema, Self::Path];

    /// 协议字面量，与 `#[serde(rename_all = "camelCase")]` 派生出的线格式逐字一致。
    ///
    /// 用途与 `packages/runtime/src/connection/error.rs` 的 `ApiErrorCode::as_str()` 相同：
    /// 让错误信息里的层级名与实际序列化结果一致，**不是**重新声明一份协议字面量。
    pub const fn as_str(self) -> &'static str {
        match self {
            NamespaceLayer::Database => "database",
            NamespaceLayer::Catalog => "catalog",
            NamespaceLayer::Schema => "schema",
            NamespaceLayer::Path => "path",
        }
    }
}

/// 驱动 `ResourceDescriptor` 注册的命名空间形状：声明该驱动**存在哪些层级**、哪些必填。
///
/// 本类型是 `NamespaceShape` 的**唯一**定义：`packages/runtime` 通过
/// `pub use datazen_platform_api::target::NamespaceShape` 直接复用它，不存在第二套语义。
///
/// `runtime` 侧的副本原先在本文件基础上多了一个 `#[serde(default)]`。删掉它会**收窄**线上
/// 的报文接受域：`{"required":["database"]}` 今天在 runtime 能反序列化，删掉就会开始拒绝
/// 那样的报文 —— 一个没有对应收益的破坏性变更。因此这里刻意保持宽松（ruling #2）。
/// `required` 不加 `default`：省略必填层必须被判 `TargetRequired`，放过它才是真缺陷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceShape {
    pub required: Vec<NamespaceLayer>,
    #[serde(default)]
    pub optional: Vec<NamespaceLayer>,
}

impl NamespaceShape {
    pub fn new(
        required: impl IntoIterator<Item = NamespaceLayer>,
        optional: impl IntoIterator<Item = NamespaceLayer>,
    ) -> Self {
        Self {
            required: required.into_iter().collect(),
            optional: optional.into_iter().collect(),
        }
    }

    /// PostgreSQL 式：database 必填，schema 可省。
    pub fn database_and_schema() -> Self {
        Self::new([NamespaceLayer::Database], [NamespaceLayer::Schema])
    }

    /// 该层级是否存在（required 或 optional 任一命中即存在）。
    ///
    /// 不存在的层级**只能是 null**（§4.3 步骤 2）。
    pub fn declares(&self, layer: NamespaceLayer) -> bool {
        self.required.contains(&layer) || self.optional.contains(&layer)
    }

    pub fn is_required(&self, layer: NamespaceLayer) -> bool {
        self.required.contains(&layer)
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
/// 与 [`NamespaceShape`] **必须同时校验**，不能用一个全驱动 required 列表代替操作需求（§4.3）。
///
/// `Default` 是「全部层级可省、不允许对象」的占位值：它对命名空间形状不作任何额外要求，
/// 因此不会掩盖错误。真实取值由 Command definition 在注册时给出（P2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRequirements {
    pub database: LayerRequirement,
    pub catalog: LayerRequirement,
    pub schema: LayerRequirement,
    pub path: LayerRequirement,
    /// 本操作是否接受对象身份。对象操作必须传完整身份，不得把未指定 schema 猜成 `public`/`dbo`。
    pub allows_object: bool,
    /// 是否允许使用该 session **已确认**的默认值。`executeInSession` 只有显式允许时才能用。
    pub allows_session_defaults: bool,
}

impl TargetRequirements {
    /// 单一目标操作：指定的两个层级分别必填/可省，允许对象。
    pub fn single_target(
        required: NamespaceLayer,
        optional: NamespaceLayer,
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

    pub fn set(&mut self, layer: NamespaceLayer, requirement: LayerRequirement) -> &mut Self {
        let slot = match layer {
            NamespaceLayer::Database => &mut self.database,
            NamespaceLayer::Catalog => &mut self.catalog,
            NamespaceLayer::Schema => &mut self.schema,
            NamespaceLayer::Path => &mut self.path,
        };
        *slot = requirement;
        self
    }

    pub fn requirement_for(&self, layer: NamespaceLayer) -> LayerRequirement {
        match layer {
            NamespaceLayer::Database => self.database,
            NamespaceLayer::Catalog => self.catalog,
            NamespaceLayer::Schema => self.schema,
            NamespaceLayer::Path => self.path,
        }
    }

    /// 是否允许用该 session 已确认的值补齐目标（§4.3：`executeInSession` 的唯一例外来源）。
    pub fn session_defaults_allowed(&self) -> bool {
        self.allows_session_defaults
    }
}

/// 命名空间目标。**四个字段都必须出现在请求里**（§4.4 字段校验），空字符串非法。
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

    pub fn get(&self, layer: NamespaceLayer) -> Option<&str> {
        match layer {
            NamespaceLayer::Database => self.database.as_deref(),
            NamespaceLayer::Catalog => self.catalog.as_deref(),
            NamespaceLayer::Schema => self.schema.as_deref(),
            NamespaceLayer::Path => None,
        }
    }

    /// 该层级是否带了非 null 值。`path` 只要非空即视为带了值。
    pub fn has_value(&self, layer: NamespaceLayer) -> bool {
        match layer {
            NamespaceLayer::Path => !self.path.is_empty(),
            other => self.get(other).is_some(),
        }
    }

    /// 写入单个层级。`Path` 在这里是单值形态（写入 `path` 的唯一一项），
    /// 与「`NamespaceTarget.path` 是列表」的形状不冲突：清空即传 `None`。
    pub fn set(&mut self, layer: NamespaceLayer, value: Option<String>) -> &mut Self {
        match layer {
            NamespaceLayer::Database => self.database = value,
            NamespaceLayer::Catalog => self.catalog = value,
            NamespaceLayer::Schema => self.schema = value,
            NamespaceLayer::Path => self.path = value.into_iter().collect(),
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
    /// [`CanonicalTarget::namespace_fingerprint`]，两者不可混用。
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
    pub fn get(&self, layer: NamespaceLayer) -> Option<&CanonicalNamespaceId> {
        match layer {
            NamespaceLayer::Database => self.database.as_ref(),
            NamespaceLayer::Catalog => self.catalog.as_ref(),
            NamespaceLayer::Schema => self.schema.as_ref(),
            NamespaceLayer::Path => None,
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
    fn execution_target_round_trips_with_and_without_object() {
        let bare = ExecutionTarget::new(
            ConnectionId::new("conn-1"),
            NamespaceTarget::database("app"),
        );
        let value = serde_json::to_value(&bare).expect("serialize");
        assert_eq!(value["object"], json!(null));
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
    fn namespace_shape_declares_layers() {
        let shape = NamespaceShape::database_and_schema();
        assert!(shape.is_required(NamespaceLayer::Database));
        assert!(!shape.is_required(NamespaceLayer::Schema));
        assert!(shape.declares(NamespaceLayer::Schema));
        assert!(
            !shape.declares(NamespaceLayer::Catalog),
            "未声明的层级不存在，只能是 null"
        );
        assert!(!shape.declares(NamespaceLayer::Path));
    }

    #[test]
    fn namespace_shape_matches_the_runtime_field_shape() {
        // 与 packages/runtime 的 `NamespaceShape { required, optional }` 逐字段一致，
        // 这样 runtime 迁移成 `pub use` 时不产生第二套语义。
        let shape = NamespaceShape::new([NamespaceLayer::Database], [NamespaceLayer::Path]);
        let value = serde_json::to_value(&shape).expect("serialize");
        assert!(value.get("required").is_some());
        assert!(value.get("optional").is_some());
        assert_eq!(value.as_object().map(|o| o.len()), Some(2));
    }

    #[test]
    fn namespace_shape_pins_its_wire_shape_as_literals() {
        // 全部断言都是**绝对字面量**，不与 runtime 的同名类型互相印证 ——
        // runtime 现在是 `pub use` 本类型，两边对比已无意义，且两套定义曾经就靠对比互相放过。
        let shape = NamespaceShape::new([NamespaceLayer::Database], [NamespaceLayer::Schema]);
        assert_eq!(
            serde_json::to_value(&shape).expect("serialize"),
            json!({"required": ["database"], "optional": ["schema"]})
        );

        // 层级字面量同样逐字钉住（`as_str()` 不得偏离 `rename_all = "camelCase"`）。
        assert_eq!(
            NamespaceLayer::ALL.map(NamespaceLayer::as_str).to_vec(),
            ["database", "catalog", "schema", "path"]
        );

        let pg = NamespaceShape::database_and_schema();
        assert_eq!(pg.required, vec![NamespaceLayer::Database]);
        assert_eq!(pg.optional, vec![NamespaceLayer::Schema]);
        assert!(pg.is_required(NamespaceLayer::Database));
        assert!(!pg.is_required(NamespaceLayer::Schema));
    }

    #[test]
    fn namespace_shape_accepts_a_missing_optional_key() {
        // 钉住 `#[serde(default)]` 决定的线上接受域：`optional` 省略必须仍能反序列化。
        // 这条正是删掉 `#[serde(default)]` 时唯一会变红的断言 —— 而那正是本文件注释
        // 记录过的、**无收益的破坏性收窄**（today runtime 能收，删掉就不能收）。
        let parsed: NamespaceShape =
            serde_json::from_str(r#"{"required":["database"]}"#).expect("optional 省略必须被接受");
        assert_eq!(parsed.required, vec![NamespaceLayer::Database]);
        assert_eq!(parsed.optional, Vec::<NamespaceLayer>::new());

        // 反面：`required` 没有 `default`，省略它必须仍然被拒。
        assert!(
            serde_json::from_str::<NamespaceShape>(r#"{"optional":["schema"]}"#).is_err(),
            "省略必填层必须被拒，放过它才是真缺陷"
        );
    }

    #[test]
    fn target_requirements_address_each_layer() {
        let mut requirements = TargetRequirements::single_target(
            NamespaceLayer::Database,
            NamespaceLayer::Schema,
            true,
        );
        requirements.set(NamespaceLayer::Catalog, LayerRequirement::Forbidden);

        assert_eq!(
            requirements.requirement_for(NamespaceLayer::Database),
            LayerRequirement::Required
        );
        assert_eq!(
            requirements.requirement_for(NamespaceLayer::Schema),
            LayerRequirement::Optional
        );
        assert_eq!(
            requirements.requirement_for(NamespaceLayer::Catalog),
            LayerRequirement::Forbidden
        );
        assert_eq!(
            requirements.requirement_for(NamespaceLayer::Path),
            LayerRequirement::Optional
        );
        assert!(!LayerRequirement::Forbidden.accepts_value());
        assert!(LayerRequirement::Required.accepts_value());
    }

    #[test]
    fn canonical_target_ids_do_not_borrow_the_submitted_type() {
        let submitted = NamespaceTarget::database("App");
        let canonical = CanonicalNamespace {
            database: Some(CanonicalNamespaceId::new("app")),
            ..CanonicalNamespace::default()
        };
        let target = CanonicalTarget::new(
            ConnectionId::new("conn-1"),
            canonical,
            Some(ObjectTarget::new("table", "users")),
        );
        let value = serde_json::to_value(&target).expect("serialize");
        assert_eq!(value["namespace"]["database"], json!("app"));
        assert_eq!(
            value["namespace"]["database"].as_str(),
            Some(
                target
                    .namespace
                    .database
                    .as_ref()
                    .map(|id| id.as_str())
                    .unwrap_or_default()
            )
        );
        // 展示名与规范化 ID 数值不同，但只有规范化 ID 进入身份。
        assert_ne!(submitted.database.as_deref(), Some("app"));
    }

    #[test]
    fn has_value_distinguishes_absent_from_empty_path() {
        let target = NamespaceTarget::default();
        assert!(!target.has_value(NamespaceLayer::Database));
        assert!(!target.has_value(NamespaceLayer::Path));

        let with_path = NamespaceTarget::new(None, None, None, vec!["a".into()]);
        assert!(with_path.has_value(NamespaceLayer::Path));
    }
}
