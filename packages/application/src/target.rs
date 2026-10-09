//! §4.3 目标计算：请求目标 → [`CanonicalTarget`]（连接 §4.3）。
//!
//! ## 固定处理顺序（不得调换）
//!
//! 1. **字段完整性**：四个字段必须都出现在请求里，值不得为空串。
//! 2. **形状合法性**：按驱动的 `namespaceShape` 拒绝「给不存在的层级发值」。
//! 3. **别名合并 / 冲突**：同一值被别名指到别的层级 ⇒ `TargetConflict`。
//! 4. **驱动规范化**：`displayName → 驱动身份 id`；驱动不认 ⇒ `TargetUnsupported`。
//! 5. **操作级要求**：按 Command definition 的 `targetRequirements` 校验 ⇒ `TargetRequired`。
//! 6. **产出** [`CanonicalTarget`]，ResourceManager、权限、缓存、执行**共用**这一份。
//!
//! 顺序本身就是契约：先规范化再校验操作要求，权限比较才落在身份上而不是展示名上。
//!
//! ## 为什么驱动器是注入的闭包
//!
//! 依赖图里 `APP → PA, DAPI` 明确**没有** `APP → RT`：application 拿不到运行时，
//! 也拿不到具体驱动（别名表、`ResourceDescriptor` 注册表都在驱动/运行时侧）。
//! 因此驱动相关能力一律以 `Arc<dyn Fn …>` 注入，P1 只实现「顺序与判定」这段纯逻辑。
//!
//! ## P2 待办
//!
//! `namespaceShape` 与操作级 `targetRequirements` 的**逐驱动、逐 Command 具体取值**
//! 属于 P2（详见 `shared-boundaries-and-ports.md`）。P1 只编码连接 §4.3 明文枚举的判定，
//! 并把这两张表建模为注入源；注入源返回 `None` 时**失败关闭**（`TargetUnsupported`），
//! 绝不静默跳过第 2/5 步。

use std::sync::Arc;

use datazen_platform_api::id::ConnectionId;
use datazen_platform_api::target::{
    CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, NamespaceTarget, ObjectTarget,
    TargetNamespaceLayer, TargetNamespaceShape, TargetRequirements,
};

use crate::dto::requests::validate_namespace_target;
use crate::error::{ApiError, ApiErrorCode};

/// 别名解析：把提交态的一个值映射回它所属的层级。返回 `None` 表示不是别名。
pub type AliasResolver = Arc<dyn Fn(&str) -> Option<TargetNamespaceLayer> + Send + Sync>;

/// 驱动规范化：把提交态标识规范化为驱动身份标识。返回 `None` 表示驱动不认这个标识。
pub type Canonicalizer = Arc<dyn Fn(TargetNamespaceLayer, &str) -> Option<String> + Send + Sync>;

/// 命名空间形状来源：`connectionId → 驱动注册表里声明的形状`。返回 `None` 表示连接不可解析。
pub type ShapeProvider = Arc<dyn Fn(&ConnectionId) -> Option<TargetNamespaceShape> + Send + Sync>;

/// 操作级要求来源：`command → Command definition 声明的 targetRequirements`。`None` 表示操作未注册。
pub type RequirementsProvider = Arc<dyn Fn(&str) -> Option<TargetRequirements> + Send + Sync>;

/// 计算模式。
#[derive(Debug, Clone, Default)]
pub enum TargetSource {
    /// `executeAtTarget`：**永不回填**。请求没给就是没给，缺必需层级直接 `TargetRequired`。
    #[default]
    Request,
    /// `executeInSession`：只有当 Command 允许（`allows_session_defaults`）时，才用
    /// **已确认**的会话值补齐缺失层级。无法确认的层级依旧 `TargetRequired`。
    Session {
        confirmed: NamespaceTarget,
        /// 会话对未知层级的自我认知（连接 §4.3：`observedContext` 可能为 `unknown`）。
        confirmed_layers: Vec<TargetNamespaceLayer>,
    },
}

/// §4.3 的目标计算器。持有四个注入源，没有具体驱动、没有运行时、没有传输。
#[derive(Clone)]
pub struct TargetResolver {
    alias: AliasResolver,
    canonicalize: Canonicalizer,
    shape: ShapeProvider,
    requirements: RequirementsProvider,
}

impl TargetResolver {
    /// 组装计算器。
    pub fn new(
        alias: AliasResolver,
        canonicalize: Canonicalizer,
        shape: ShapeProvider,
        requirements: RequirementsProvider,
    ) -> Self {
        Self {
            alias,
            canonicalize,
            shape,
            requirements,
        }
    }

    /// §4.3 全流程：提交目标 → 规范化目标。
    pub fn compute(
        &self,
        command: &str,
        connection_id: &ConnectionId,
        requested: &NamespaceTarget,
        object: Option<&ObjectTarget>,
        source: &TargetSource,
    ) -> Result<CanonicalTarget, ApiError> {
        // 第 1 步：字段完整性。
        validate_namespace_target(requested)?;

        // 第 5 步必须先取到操作级要求才能判定「能否用会话默认值」，但**判定顺序**仍是
        // 2 → 3 → 4 → 5：这里只是提前读表，读表本身不产生副作用。
        let requirements = (self.requirements)(command).ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("操作 {command} 未注册目标要求，拒绝放行"),
            )
        })?;

        // 第 2 步：形状合法性。形状未知 ⇒ 失败关闭。
        let shape = (self.shape)(connection_id).ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("连接 {connection_id} 的命名空间形状不可解析"),
            )
        })?;
        self.reject_unknown_layers(requested, &shape)?;

        // 会话默认值：仅在 Command 允许、且会话自认已确认时补齐。
        let effective = self.apply_session_defaults(requested, &requirements, source)?;

        // 第 3 步：别名合并 / 冲突。
        self.merge_aliases(&effective)?;

        // 第 4 步：驱动规范化。
        let namespace = self.canonicalize_namespace(&effective)?;

        // 第 5 步：操作级要求。
        self.require_layers(&effective, &requirements)?;
        if object.is_some() && !requirements.allows_object {
            return Err(ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("操作 {command} 不接受对象身份"),
            ));
        }
        if object.is_none() && requires_object(command, &requirements) {
            return Err(ApiError::target_required("该操作要求完整对象身份"));
        }

        Ok(CanonicalTarget::new(
            connection_id.clone(),
            namespace,
            object.cloned(),
        ))
    }

    /// 第 2 步：给不存在的层级发值 ⇒ `TargetUnsupported`（形态问题，不是参数问题）。
    fn reject_unknown_layers(
        &self,
        target: &NamespaceTarget,
        shape: &TargetNamespaceShape,
    ) -> Result<(), ApiError> {
        for layer in TargetNamespaceLayer::ALL {
            if target.has_value(layer) && !shape.declares(layer) {
                return Err(ApiError::new(
                    ApiErrorCode::TargetUnsupported,
                    format!("该驱动不存在 {layer:?} 层级"),
                ));
            }
        }
        Ok(())
    }

    /// 会话已确认值的补齐。**只有** Command 允许时才补，且只补会话自认确认过的层级。
    fn apply_session_defaults(
        &self,
        requested: &NamespaceTarget,
        requirements: &TargetRequirements,
        source: &TargetSource,
    ) -> Result<NamespaceTarget, ApiError> {
        let TargetSource::Session {
            confirmed,
            confirmed_layers,
        } = source
        else {
            // `executeAtTarget` 永不回填。
            return Ok(requested.clone());
        };
        if !requirements.session_defaults_allowed() {
            return Ok(requested.clone());
        }
        let mut merged = requested.clone();
        for layer in TargetNamespaceLayer::ALL {
            if merged.has_value(layer) || !confirmed_layers.contains(&layer) {
                continue;
            }
            let fallback = match confirmed.get(layer) {
                Some(value) => value.to_string(),
                None => continue,
            };
            merged.set(layer, Some(fallback));
        }
        Ok(merged)
    }

    /// 第 3 步：别名冲突检测。
    ///
    /// 别名指到与提交槽位不同的层级时，`TargetConflict`。同名同层不算冲突——
    /// 那是合法的重复表达，必须保持一致。
    fn merge_aliases(&self, target: &NamespaceTarget) -> Result<(), ApiError> {
        for layer in TargetNamespaceLayer::ALL {
            let Some(value) = target.get(layer) else {
                continue;
            };
            if let Some(alias_layer) = (self.alias)(value) {
                if alias_layer != layer {
                    return Err(ApiError::target_conflict(format!(
                        "别名 {value} 指向 {alias_layer:?}，但提交在 {layer:?} 槽位"
                    )));
                }
            }
        }
        for segment in &target.path {
            if let Some(alias_layer) = (self.alias)(segment) {
                if alias_layer != TargetNamespaceLayer::Path {
                    return Err(ApiError::target_conflict(format!(
                        "别名 {segment} 指向 {alias_layer:?}，但提交在 path 段"
                    )));
                }
            }
        }
        Ok(())
    }

    /// 第 4 步：驱动规范化。
    fn canonicalize_namespace(
        &self,
        target: &NamespaceTarget,
    ) -> Result<CanonicalNamespace, ApiError> {
        let database =
            self.canonicalize_layer(TargetNamespaceLayer::Database, target.database.as_deref())?;
        let catalog =
            self.canonicalize_layer(TargetNamespaceLayer::Catalog, target.catalog.as_deref())?;
        let schema =
            self.canonicalize_layer(TargetNamespaceLayer::Schema, target.schema.as_deref())?;
        let mut path = Vec::with_capacity(target.path.len());
        for segment in &target.path {
            let canonical = self.canonicalize_layer(TargetNamespaceLayer::Path, Some(segment))?;
            // `segment` 已是 `&String`，`canonicalize_layer` 只会返回 `Some`。
            // 空结果意味着驱动把一个已知非空的片段规范化成了空身份，那在上一步已被拒绝。
            if let Some(canonical) = canonical {
                path.push(canonical);
            }
        }
        Ok(CanonicalNamespace {
            database,
            catalog,
            schema,
            path,
        })
    }

    fn canonicalize_layer(
        &self,
        layer: TargetNamespaceLayer,
        value: Option<&str>,
    ) -> Result<Option<CanonicalNamespaceId>, ApiError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let canonical = (self.canonicalize)(layer, value).ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("驱动无法把 {layer:?} 的 {value} 规范化为身份"),
            )
        })?;
        if canonical.trim().is_empty() {
            return Err(ApiError::invalid_argument(format!(
                "驱动把 {layer:?} 的 {value} 规范化成了空标识"
            )));
        }
        Ok(Some(CanonicalNamespaceId::new(canonical)))
    }

    /// 第 5 步：操作级层级要求。
    fn require_layers(
        &self,
        target: &NamespaceTarget,
        requirements: &TargetRequirements,
    ) -> Result<(), ApiError> {
        for layer in TargetNamespaceLayer::ALL {
            match LayerCheck::from(requirements.requirement_for(layer)) {
                LayerCheck::Required => {
                    if !target.has_value(layer) {
                        return Err(ApiError::target_required(format!(
                            "该操作要求提供 {layer:?} 层级"
                        )));
                    }
                }
                LayerCheck::Forbidden => {
                    if target.has_value(layer) {
                        return Err(ApiError::invalid_argument(format!(
                            "该操作不接受 {layer:?} 层级"
                        )));
                    }
                }
                LayerCheck::Optional => {}
            }
        }
        Ok(())
    }
}

/// `require_layers` 用的三态，与 [`datazen_platform_api::target::LayerRequirement`] 同名不同型。
///
/// 单独起名是为了让上面那个 `match` 读起来是「要求 → 判定」而不是「枚举 → 判定」，
/// 同时避免在 use 里与 `LayerRequirement` 混用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerCheck {
    Required,
    Forbidden,
    Optional,
}

impl From<datazen_platform_api::target::LayerRequirement> for LayerCheck {
    fn from(value: datazen_platform_api::target::LayerRequirement) -> Self {
        match value {
            datazen_platform_api::target::LayerRequirement::Required => Self::Required,
            datazen_platform_api::target::LayerRequirement::Forbidden => Self::Forbidden,
            datazen_platform_api::target::LayerRequirement::Optional => Self::Optional,
        }
    }
}

/// 操作是否要求完整对象身份：`allows_object == false` 且请求没带对象时不算要求，
/// 因此这里显式读取「要求方」，默认所有操作都**不**额外要求对象。
///
/// P2 由 Command definition 补齐 `requires_object` 字段；P1 保留此函数而不是
/// 直接写 `false`，是为了让 P2 的改动点唯一。
fn requires_object(_command: &str, _requirements: &TargetRequirements) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PostgreSQL 风格形状：database 必填，schema 可省，无 catalog/path。
    fn postgres_shape(_connection: &ConnectionId) -> Option<TargetNamespaceShape> {
        Some(TargetNamespaceShape::new(
            vec![TargetNamespaceLayer::Database],
            vec![TargetNamespaceLayer::Schema],
        ))
    }

    /// 单目标操作：database 必填、schema 可省、允许对象。
    fn single_target(command: &str) -> Option<TargetRequirements> {
        match command {
            "query" => Some(TargetRequirements::single_target(
                TargetNamespaceLayer::Database,
                TargetNamespaceLayer::Schema,
                true,
            )),
            "execute_in_session" => {
                let mut requirements = TargetRequirements::single_target(
                    TargetNamespaceLayer::Database,
                    TargetNamespaceLayer::Schema,
                    false,
                );
                requirements.allows_session_defaults = true;
                Some(requirements)
            }
            // §7.3：executeAtTarget 不接受对象，也绝不从会话回填。
            "execute_at_target" => Some(TargetRequirements::single_target(
                TargetNamespaceLayer::Database,
                TargetNamespaceLayer::Schema,
                false,
            )),
            "list_objects" => {
                let mut requirements = TargetRequirements::default();
                requirements.allows_session_defaults = true;
                Some(requirements)
            }
            _ => None,
        }
    }

    fn resolver() -> TargetResolver {
        TargetResolver::new(
            Arc::new(|value: &str| match value {
                "main" => Some(TargetNamespaceLayer::Database),
                "sales" => Some(TargetNamespaceLayer::Schema),
                _ => None,
            }),
            Arc::new(|_layer: TargetNamespaceLayer, value: &str| {
                let canonical = match value {
                    "app" | "main" => "app",
                    "public" => "public",
                    "sales" => "sales",
                    other => other,
                };
                Some(canonical.to_string())
            }),
            Arc::new(postgres_shape),
            Arc::new(single_target),
        )
    }

    fn target(database: Option<&str>, schema: Option<&str>) -> NamespaceTarget {
        NamespaceTarget::new(
            database.map(str::to_string),
            None,
            schema.map(str::to_string),
            Vec::new(),
        )
    }

    fn compute(
        resolver: &TargetResolver,
        command: &str,
        requested: &NamespaceTarget,
    ) -> Result<CanonicalTarget, ApiError> {
        resolver.compute(
            command,
            &ConnectionId::new("conn-1"),
            requested,
            None,
            &TargetSource::Request,
        )
    }

    #[test]
    fn the_six_steps_run_in_the_documented_order() {
        // 一次请求里同时踩到「形状非法」（catalog）与「别名冲突」（schema=main），
        // 结果必须是第 2 步的形状错误，证明顺序没有被调换。
        let bad = NamespaceTarget::new(
            Some("app".into()),
            Some("public".into()),
            Some("main".into()),
            Vec::new(),
        );
        let error = compute(&resolver(), "query", &bad).expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetUnsupported);
        assert!(
            error.message.to_ascii_lowercase().contains("catalog"),
            "实际信息: {}",
            error.message
        );
    }

    #[test]
    fn empty_field_values_are_rejected_before_anything_else() {
        let empty = NamespaceTarget::new(Some(String::new()), None, None, Vec::new());
        let error = compute(&resolver(), "query", &empty).expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    }

    #[test]
    fn alias_in_the_wrong_slot_is_a_target_conflict() {
        let conflict = target(Some("app"), Some("main"));
        let error = compute(&resolver(), "query", &conflict).expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetConflict);
        assert!(error.message.contains("main"));
    }

    #[test]
    fn a_successful_computation_yields_canonical_identity_only() {
        let canonical = compute(&resolver(), "query", &target(Some("app"), Some("sales")))
            .expect("canonical target");
        assert_eq!(canonical.namespace_fingerprint(), "app//sales");
        // 展示名不出现在规范化结果里：别名 main 被折叠成驱动身份 app。
        let again = compute(&resolver(), "query", &target(Some("main"), Some("sales")))
            .expect("canonical target");
        assert_eq!(canonical, again, "别名与真名必须落到同一身份");
    }

    #[test]
    fn a_missing_required_layer_is_target_required_not_a_backfill() {
        let error =
            compute(&resolver(), "query", &target(None, Some("sales"))).expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetRequired);
    }

    #[test]
    fn execute_at_target_never_backfills_from_a_session() {
        let resolver = resolver();
        let requested = target(None, Some("sales"));
        let source = TargetSource::Session {
            confirmed: target(Some("app"), Some("sales")),
            confirmed_layers: vec![TargetNamespaceLayer::Database],
        };
        let error = resolver
            .compute(
                "execute_at_target",
                &ConnectionId::new("conn-1"),
                &requested,
                None,
                &source,
            )
            .expect_err("must fail");
        // 会话自认 database=app，但 executeAtTarget 不回填 ⇒ 仍是 TargetRequired，
        // 而不是 TargetConflict（层可选）或成功（层必填）。补齐只发生在 execute_in_session。
        assert_eq!(error.code, ApiErrorCode::TargetRequired);
    }

    #[test]
    fn session_defaults_apply_only_when_the_command_allows_them() {
        let resolver = resolver();
        let requested = target(None, Some("sales"));
        let source = TargetSource::Session {
            confirmed: target(Some("app"), Some("sales")),
            confirmed_layers: vec![TargetNamespaceLayer::Database],
        };
        // execute_in_session 允许回填 → 成功。
        let canonical = resolver
            .compute(
                "execute_in_session",
                &ConnectionId::new("conn-1"),
                &requested,
                None,
                &source,
            )
            .expect("backfilled");
        assert_eq!(canonical.namespace_fingerprint(), "app//sales");
        // query 不允许回填 → 同样输入必须失败。
        let error = resolver
            .compute(
                "query",
                &ConnectionId::new("conn-1"),
                &requested,
                None,
                &source,
            )
            .expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetRequired);
    }

    #[test]
    fn unconfirmed_session_layers_are_not_used() {
        let resolver = resolver();
        let requested = target(None, None);
        let source = TargetSource::Session {
            confirmed: target(Some("app"), Some("sales")),
            // 会话自认 database 未确认。
            confirmed_layers: Vec::new(),
        };
        let error = resolver
            .compute(
                "execute_in_session",
                &ConnectionId::new("conn-1"),
                &requested,
                None,
                &source,
            )
            .expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetRequired);
    }

    #[test]
    fn unknown_operation_or_connection_fails_closed() {
        let resolver = resolver();
        let unknown_command = resolver
            .compute(
                "not_registered",
                &ConnectionId::new("conn-1"),
                &target(Some("app"), None),
                None,
                &TargetSource::Request,
            )
            .expect_err("must fail");
        assert_eq!(unknown_command.code, ApiErrorCode::TargetUnsupported);

        let no_shape = TargetResolver::new(
            resolver.alias.clone(),
            resolver.canonicalize.clone(),
            Arc::new(|_connection: &ConnectionId| None),
            Arc::new(single_target),
        );
        let error = no_shape
            .compute(
                "query",
                &ConnectionId::new("conn-1"),
                &target(Some("app"), None),
                None,
                &TargetSource::Request,
            )
            .expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetUnsupported);
    }

    #[test]
    fn object_identity_is_gated_by_the_command_requirements() {
        let resolver = resolver();
        let object = ObjectTarget::new("table", "orders");
        // query 允许对象。
        let canonical = resolver
            .compute(
                "query",
                &ConnectionId::new("conn-1"),
                &target(Some("app"), None),
                Some(&object),
                &TargetSource::Request,
            )
            .expect("object allowed");
        assert_eq!(canonical.object_signature(), "table:orders/");
        // execute_in_session 不接受对象。
        let error = resolver
            .compute(
                "execute_in_session",
                &ConnectionId::new("conn-1"),
                &target(Some("app"), None),
                Some(&object),
                &TargetSource::Request,
            )
            .expect_err("must fail");
        assert_eq!(error.code, ApiErrorCode::TargetUnsupported);
    }

    #[test]
    fn the_canonical_target_round_trips_and_carries_no_display_name() {
        let canonical = compute(&resolver(), "query", &target(Some("main"), Some("sales")))
            .expect("canonical target");
        let encoded = serde_json::to_string(&canonical).expect("serialize");
        assert!(
            !encoded.contains("main"),
            "别名不得出现在规范化目标里: {encoded}"
        );
        let decoded: CanonicalTarget = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, canonical);
        assert_eq!(
            decoded.namespace.database.as_ref().map(|id| id.as_str()),
            Some("app")
        );
    }

    #[test]
    fn counter_free_computation_does_not_invent_revisions() {
        // 目标计算**不**引入任何版本号：configRevision 的比对在用例层，
        // 避免「算目标」顺带把乐观锁语义搅进来。
        let source = include_str!("target.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(!source.contains("ConfigRevision"));
    }
}
