//! §4.3 目标计算：请求目标 → [`CanonicalTarget`]（连接 §4.3）。
//!
//! ## 固定处理顺序（不得调换）
//!
//! 1. **字段完整性**：四个字段必须都出现在请求里，值不得为空串。
//! 2. **形状合法性**：按驱动的 `namespaceShape` 拒绝「给不存在的层级发值」。
//! 3. **别名解析**：驱动别名在所属层级内归一；循环别名 ⇒ `TargetConflict`。
//! 4. **驱动规范化**：`displayName → 驱动身份 id`；驱动不认 ⇒ `TargetUnsupported`。
//! 5. **操作级要求**：按 Command definition 的 `targetRequirements` 校验 ⇒ `TargetRequired`。
//! 6. **产出** [`CanonicalTarget`]，ResourceManager、权限、缓存、执行**共用**这一份。
//!
//! 顺序本身就是契约：先规范化再校验操作要求，权限比较才落在身份上而不是展示名上。
//!
//! The host supplies one atomic, connection-and-command-specific metadata snapshot. The
//! application remains independent of the runtime and concrete driver crates.

use std::sync::Arc;

use crate::execution::target::{
    CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, NamespaceTarget, ObjectTarget,
    TargetCanonicalizationError, TargetNamespaceLayer, TargetNamespaceShape, TargetRequirements,
};
use crate::shared::id::ConnectionId;

use crate::execution::application::requests::validate_namespace_target;
use crate::shared::error::{ApiError, ApiErrorCode};

/// One atomic snapshot of the selected connection's driver shape and command requirements.
/// `None` requirements mean the command is not registered for that driver.
pub type TargetMetadataProvider = Arc<
    dyn Fn(&ConnectionId, &str) -> Option<(TargetNamespaceShape, Option<TargetRequirements>)>
        + Send
        + Sync,
>;

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

/// §4.3 的目标计算器。每次取同一连接与命令的驱动元数据快照，不依赖具体驱动或运行时。
#[derive(Clone)]
pub struct TargetResolver {
    metadata: TargetMetadataProvider,
}

impl TargetResolver {
    /// 组装计算器。
    pub fn new(metadata: TargetMetadataProvider) -> Self {
        Self { metadata }
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
        let (shape, requirements) = (self.metadata)(connection_id, command).ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("连接 {connection_id} 的命名空间形状不可解析"),
            )
        })?;
        let requirements = requirements.ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("操作 {command} 未注册目标要求，拒绝放行"),
            )
        })?;
        if requirements.requires_object && !requirements.allows_object {
            return Err(ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("操作 {command} 的目标要求无效：要求对象但不接受对象"),
            ));
        }

        // 第 2 步：形状合法性。形状未知 ⇒ 失败关闭。
        self.reject_unknown_layers(requested, &shape)?;

        // 会话默认值：仅在 Command 允许、且会话自认已确认时补齐。
        let effective = self.apply_session_defaults(requested, &requirements, source)?;
        self.reject_unknown_layers(&effective, &shape)?;
        self.require_shape_layers(&effective, &shape)?;

        // 第 3-4 步：驱动形状负责同层别名与大小写规范化；别名的目标值仍在原层级，
        // 不把跨层字符串映射误解为 namespace alias。
        let namespace = self.canonicalize_namespace(&effective, &shape)?;

        // 第 5 步：操作级要求。
        self.require_layers(&effective, &requirements)?;
        if let Some(object) = object {
            if object.kind.is_empty()
                || object.name.is_empty()
                || object.signature.as_deref().is_some_and(str::is_empty)
            {
                return Err(ApiError::invalid_argument(
                    "对象身份的 kind 与 name 必须有效；提供的 signature 不能是空串",
                ));
            }
        }
        if object.is_some() && !requirements.allows_object {
            return Err(ApiError::new(
                ApiErrorCode::TargetUnsupported,
                format!("操作 {command} 不接受对象身份"),
            ));
        }
        if object.is_none() && requirements.requires_object {
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

    /// Driver-required namespace levels apply to every command for this connection.
    fn require_shape_layers(
        &self,
        target: &NamespaceTarget,
        shape: &TargetNamespaceShape,
    ) -> Result<(), ApiError> {
        for layer in TargetNamespaceLayer::ALL {
            if shape.is_required(layer) && !target.has_value(layer) {
                return Err(ApiError::target_required(format!(
                    "该连接要求提供 {layer:?} 层级"
                )));
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

    /// 第 4 步：驱动规范化。
    fn canonicalize_namespace(
        &self,
        target: &NamespaceTarget,
        shape: &TargetNamespaceShape,
    ) -> Result<CanonicalNamespace, ApiError> {
        let database = self.canonicalize_layer(
            shape,
            TargetNamespaceLayer::Database,
            target.database.as_deref(),
        )?;
        let catalog = self.canonicalize_layer(
            shape,
            TargetNamespaceLayer::Catalog,
            target.catalog.as_deref(),
        )?;
        let schema = self.canonicalize_layer(
            shape,
            TargetNamespaceLayer::Schema,
            target.schema.as_deref(),
        )?;
        let mut path = Vec::with_capacity(target.path.len());
        for segment in &target.path {
            let canonical =
                self.canonicalize_layer(shape, TargetNamespaceLayer::Path, Some(segment))?;
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
        shape: &TargetNamespaceShape,
        layer: TargetNamespaceLayer,
        value: Option<&str>,
    ) -> Result<Option<CanonicalNamespaceId>, ApiError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let canonical = shape
            .canonicalize_identifier(value)
            .map_err(|error| match error {
                TargetCanonicalizationError::AliasConflict => {
                    ApiError::target_conflict(format!("{layer:?} 标识符别名无法解析"))
                }
            })?;
        if canonical.is_empty() {
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

/// `require_layers` 用的三态，与 [`crate::execution::target::LayerRequirement`] 同名不同型。
///
/// 单独起名是为了让上面那个 `match` 读起来是「要求 → 判定」而不是「枚举 → 判定」，
/// 同时避免在 use 里与 `LayerRequirement` 混用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerCheck {
    Required,
    Forbidden,
    Optional,
}

impl From<crate::execution::target::LayerRequirement> for LayerCheck {
    fn from(value: crate::execution::target::LayerRequirement) -> Self {
        match value {
            crate::execution::target::LayerRequirement::Required => Self::Required,
            crate::execution::target::LayerRequirement::Forbidden => Self::Forbidden,
            crate::execution::target::LayerRequirement::Optional => Self::Optional,
        }
    }
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
    fn single_target(_connection: &ConnectionId, command: &str) -> Option<TargetRequirements> {
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
        TargetResolver::new(Arc::new(|connection: &ConnectionId, command: &str| {
            let mut shape = postgres_shape(connection)?;
            shape.aliases.insert("main".into(), "app".into());
            Some((shape, single_target(connection, command)))
        }))
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
        // 一次请求同时包含「形状非法」（catalog）与驱动别名（schema=main），
        // 结果必须先报告第 2 步的形状错误，不能越过形状校验做别名规范化。
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
    fn a_driver_alias_is_canonicalized_in_its_submitted_layer() {
        let canonical = compute(&resolver(), "query", &target(Some("app"), Some("main")))
            .expect("same-layer alias canonicalizes");
        assert_eq!(canonical.namespace_fingerprint(), "app//app");
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
        // 会话自认 database=app，但 executeAtTarget 不回填 ⇒ 仍是 TargetRequired。
        // 补齐只发生在明确允许的 execute_in_session。
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

        let no_shape = TargetResolver::new(Arc::new(|_, _| None));
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
