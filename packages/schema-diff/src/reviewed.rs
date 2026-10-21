//! Short-lived, one-shot backend plans. Client SQL and risk labels never authorize writes.
//!
//! # 规则留包，存储走端口
//!
//! 本模块保留**评审计划的规则**：身份指纹、目标快照复核、依赖目录比对、计划有效期。
//! **存储**由 [`ReviewedPlanStore`] 端口抽象，实例由宿主注入。
//!
//! 包内**不再持有进程级全局可变状态**：计划不再寄居在某个 `static` 里，
//! 而是寄居在宿主注入的存储实例中，并按组织作用域隔离——多租户宿主里
//! A 组织的计划不会被 B 组织读到、顶掉或提前消费掉。
//!
//! 端口方法一律带 `scope: &OrganizationScope`；**越组织访问一律按「不存在」处理**
//! （`None` / `false` / 空），**不得**返回「无权限」错误——那会泄漏「存在但无权」
//! （CM-05）。作用域类型由 `datazen-platform-api` 提供，本包不自造。
use super::{
    object_identity::{SchemaObjectDependencySnapshot, SchemaObjectIdentity},
    objects::SchemaObjectSnapshot,
    types::SchemaDiffPlan,
};
use async_trait::async_trait;
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, TableSchema};
use datazen_platform_api::ports::scope::OrganizationScope;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "reviewed_catalog.rs"]
mod catalog;
pub use catalog::{validate_object_dependency_catalog, validate_table_identity_catalog};

/// 评审计划的有效期。超过即失效，必须重新比对后再部署。
pub const REVIEWED_PLAN_TTL: Duration = Duration::from_secs(1800);

/// 单个存储实例内保留的评审计划上限。超出时裁剪最旧的一条。
pub const REVIEWED_PLAN_MAX_ENTRIES: usize = 128;

/// 评审计划存储端口的错误。
///
/// 只表达「存储不可用」；**不**表达「越权」——越组织访问一律按「不存在」处理。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewedPlanStoreError {
    #[error("reviewed plan store unavailable: {0}")]
    Unavailable(String),
}

/// 已评审计划的存储端口。
///
/// 行类型是本包的词汇 [`ReviewedPlan`]——端口不另造一份计划结构。
///
/// * `put` 冻结一份计划；`get` 读取但不消费；`remove` 消费（取回并删除）。
/// * 缺失**与**越组织一律返回 `None` / `false`——**不**返回「无权限」错误，
///   否则会泄漏「存在但无权」（CM-05）。
/// * `remove` 返回 `false` 表示该计划已被消费，据此保证「一次部署只消费一次」。
#[async_trait]
pub trait ReviewedPlanStore: Send + Sync + 'static {
    /// 冻结（写入）一份已评审计划。
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanStoreError>;

    /// 读取一份已评审计划（不消费）。缺失或越组织一律返回 `None`。
    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanStoreError>;

    /// 消费一份已评审计划（取回并删除）。已被消费返回 `false`。
    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanStoreError>;
}

#[derive(Debug, Clone)]
pub struct ReviewedPlan {
    pub plan: SchemaDiffPlan,
    pub target_session: String,
    pub target_pool: String,
    pub target_identity: serde_json::Value,
    pub target_database_scope: Option<String>,
    pub target_schema_scope: Option<String>,
    pub snapshots: Vec<(String, TableSchema)>,
    pub has_complete_target_dependency_catalog: bool,
    pub target_dependency_schema_scope: Option<String>,
    pub object_snapshots: Vec<SchemaObjectSnapshot>,
    pub object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    pub target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    pub has_complete_target_object_catalog: bool,
    pub target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
    pub source_snapshot: Option<ReviewedSourceSnapshot>,
    /// 冻结时刻（Unix 毫秒）。
    ///
    /// 用可持久化的墙上时钟而不是单调钟：存储实例可能被换成落盘实现，
    /// 而有效期规则（[`REVIEWED_PLAN_TTL`]）必须由规则层与存储层读同一份时刻。
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ReviewedSourceSnapshot {
    pub session: String,
    pub pool: String,
    pub identity: serde_json::Value,
    pub database_scope: Option<String>,
    pub schema_scope: Option<String>,
    pub table_snapshots: Vec<(String, TableSchema)>,
    pub object_snapshots: Vec<SchemaObjectSnapshot>,
    pub table_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
}

impl ReviewedPlan {
    /// 距冻结时刻过去了多久。
    pub fn age(&self) -> Duration {
        Duration::from_millis(now_unix_millis().saturating_sub(self.created_at_ms))
    }
}

/// 当前时刻（Unix 毫秒）。系统时钟早于纪元时按 0 处理，不 panic。
fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

pub fn identity(config: &ConnectionConfig) -> serde_json::Value {
    serde_json::json!({"driver":config.database_type,"host":config.host,"port":config.port,
        "database":config.database,"schema":config.schema,"user":config.username,
        "options":config.options,"tunnel":config.ssh_tunnel})
}

pub fn validate_source_connection_snapshot(
    snapshot: &ReviewedSourceSnapshot,
    session: &str,
    pool: &str,
    config: &ConnectionConfig,
) -> Result<(), String> {
    if snapshot.session != session
        || snapshot.pool != pool
        || snapshot.identity != identity(config)
        || snapshot.database_scope != config.database
    {
        return Err("Source connection changed after review; compare again".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalDatabaseScope {
    Same,
    Different,
    Unknown,
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        false,
        None,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_dependency_catalog(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    dependency_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        true,
        dependency_schema_scope,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_objects(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
) -> Result<(), String> {
    freeze_with_objects_and_mysql_view_context(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        target_database_scope,
        target_schema_scope,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_objects_and_mysql_view_context(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        Vec::new(),
        Vec::new(),
        false,
        target_database_scope,
        target_schema_scope,
        false,
        None,
        None,
        target_mysql_view_scope_context,
    )
    .await
}

/// Freeze one unified plan together with its full target object dependency catalog.
/// Deploy re-reads this catalog before executing any statement.
#[allow(clippy::too_many_arguments)]
pub async fn freeze_with_unified_catalog(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    has_complete_target_dependency_catalog: bool,
    has_complete_target_object_catalog: bool,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    dependency_schema_scope: Option<String>,
    source_snapshot: ReviewedSourceSnapshot,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    freeze_internal(
        store,
        scope,
        plan,
        session,
        handle,
        config,
        snapshots,
        object_snapshots,
        object_dependency_catalog,
        target_table_identity_catalog,
        has_complete_target_object_catalog,
        target_database_scope,
        target_schema_scope,
        has_complete_target_dependency_catalog,
        dependency_schema_scope,
        Some(source_snapshot),
        target_mysql_view_scope_context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn freeze_internal(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    plan: &mut SchemaDiffPlan,
    session: String,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    snapshots: Vec<(String, TableSchema)>,
    object_snapshots: Vec<SchemaObjectSnapshot>,
    object_dependency_catalog: Vec<SchemaObjectDependencySnapshot>,
    target_table_identity_catalog: Vec<SchemaObjectIdentity>,
    has_complete_target_object_catalog: bool,
    target_database_scope: Option<String>,
    target_schema_scope: Option<String>,
    has_complete_target_dependency_catalog: bool,
    target_dependency_schema_scope: Option<String>,
    source_snapshot: Option<ReviewedSourceSnapshot>,
    target_mysql_view_scope_context: Option<super::unified_scope::MySqlViewScopeContext>,
) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().to_string();
    plan.plan_id = Some(id.clone());
    let frozen = ReviewedPlan {
        plan: plan.clone(),
        target_session: session,
        target_pool: handle.pool_id.clone(),
        target_identity: identity(config),
        target_database_scope,
        target_schema_scope,
        snapshots,
        has_complete_target_dependency_catalog,
        target_dependency_schema_scope,
        object_snapshots,
        object_dependency_catalog,
        target_table_identity_catalog,
        has_complete_target_object_catalog,
        target_mysql_view_scope_context,
        source_snapshot,
        created_at_ms: now_unix_millis(),
    };
    store
        .put(scope, &id, frozen)
        .await
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub async fn consume(
    store: &dyn ReviewedPlanStore,
    scope: &OrganizationScope,
    submitted: &SchemaDiffPlan,
    session: &str,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    target_database_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<ReviewedPlan, String> {
    let id = submitted
        .plan_id
        .as_ref()
        .ok_or("Prepare and review a new plan before deploying")?;
    let frozen = store
        .get(scope, id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Plan expired or already executed; compare again")?;
    validate(
        &frozen,
        submitted,
        session,
        handle,
        config,
        target_database_scope,
        target_schema_scope,
    )?;
    // 校验通过才消费：复核失败**不**烧掉计划，用户改回正确目标后仍能部署。
    // 并发部署时只有一个 `remove` 会成功，另一个据此判定「已被执行」。
    if !store
        .remove(scope, id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err("Plan already executed".into());
    }
    Ok(frozen)
}

fn validate(
    frozen: &ReviewedPlan,
    submitted: &SchemaDiffPlan,
    session: &str,
    handle: &ConnectionHandle,
    config: &ConnectionConfig,
    target_database_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<(), String> {
    if frozen.age() >= REVIEWED_PLAN_TTL {
        return Err("Plan expired; compare again".into());
    }
    if frozen.plan != *submitted {
        return Err("Reviewed plan was modified; prepare again".into());
    }
    if frozen.target_session != session
        || frozen.target_pool != handle.pool_id
        || frozen.target_identity != identity(config)
    {
        return Err("Target identity changed after review; compare again".into());
    }
    if config.read_only {
        return Err("Target connection is read-only".into());
    }
    if normalized_scope(target_database_scope)
        != normalized_scope(frozen.target_database_scope.as_deref())
        || normalized_scope(target_schema_scope)
            != normalized_scope(frozen.target_schema_scope.as_deref())
    {
        return Err("Target database or schema scope changed after review; compare again".into());
    }
    Ok(())
}

fn normalized_scope(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Configuration identity is a conservative local check; driver-provided physical
/// identity is required to recognize aliases and distinct tunnels to one server.
pub fn same_endpoint(source: &ConnectionConfig, target: &ConnectionConfig) -> bool {
    let source_dialect = crate::types::normalize_dialect(&source.database_type);
    let target_dialect = crate::types::normalize_dialect(&target.database_type);
    if source_dialect != target_dialect {
        return false;
    }
    let mut a = identity(source);
    let mut b = identity(target);
    if let Some(v) = a.as_object_mut() {
        v.remove("user");
        v.insert("driver".into(), serde_json::Value::String(source_dialect));
    }
    if let Some(v) = b.as_object_mut() {
        v.remove("user");
        v.insert("driver".into(), serde_json::Value::String(target_dialect));
    }
    a == b
}

/// Detect aliases that reach the same database through different connection
/// settings. Database and schema names are only used as proof of distinct
/// scopes when the dialect's namespace rules make that conclusion reliable;
/// MySQL database names may be case-folded and SQLite paths may be hard links.
pub fn physical_database_scope(
    source: &ConnectionConfig,
    target: &ConnectionConfig,
    source_identity: Option<&str>,
    target_identity: Option<&str>,
    source_schema_scope: Option<&str>,
    target_schema_scope: Option<&str>,
    source_schema_identity: Option<&str>,
    target_schema_identity: Option<&str>,
) -> PhysicalDatabaseScope {
    let source_dialect = crate::types::normalize_dialect(&source.database_type);
    let target_dialect = crate::types::normalize_dialect(&target.database_type);
    if source_dialect != target_dialect {
        return PhysicalDatabaseScope::Different;
    }

    let source_schema = source_schema_scope
        .and_then(|value| normalized_scope(Some(value)))
        .or_else(|| normalized_scope(source.schema.as_deref()));
    let target_schema = target_schema_scope
        .and_then(|value| normalized_scope(Some(value)))
        .or_else(|| normalized_scope(target.schema.as_deref()));

    if let (Some(left), Some(right)) = (source_identity, target_identity) {
        if left != right {
            return PhysicalDatabaseScope::Different;
        }
        if source_dialect == "postgresql"
            && matches!((source_schema, target_schema), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
        // SQL Server identifier equality follows the database collation, so
        // only driver-proven schema catalog identities can establish distinct
        // scopes. Object kinds without a proven definition rewrite remain
        // fail-closed in the unified planner's scope-mapping checks.
        if source_dialect == "sqlserver"
            && matches!((source_schema_identity, target_schema_identity), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
        return PhysicalDatabaseScope::Same;
    }

    // PostgreSQL database names are case-sensitive catalog identities, and
    // schemas are separate namespaces inside a database. These names can
    // prove distinct scopes even when the server does not expose an identity.
    if source_dialect == "postgresql" {
        if matches!(
            (normalized_scope(source.database.as_deref()), normalized_scope(target.database.as_deref())),
            (Some(left), Some(right)) if left != right
        ) || matches!((source_schema, target_schema), (Some(left), Some(right)) if left != right)
        {
            return PhysicalDatabaseScope::Different;
        }
    }

    // MySQL can fold database names according to lower_case_table_names,
    // while SQLite paths can be aliases (including hard links). If the
    // driver could not provide a physical identity, spelling alone is not a
    // safe reason to allow a migration.
    PhysicalDatabaseScope::Unknown
}

pub fn validate_snapshot(
    table: &str,
    reviewed: &TableSchema,
    current: &TableSchema,
) -> Result<(), String> {
    fn relation_presentation_matches(identity: &str, reported_name: &str) -> bool {
        let (identity_scope, identity_relation) = identity
            .rsplit_once('.')
            .map_or((None, identity), |(scope, relation)| {
                (Some(scope), relation)
            });
        let (reported_scope, reported_relation) = reported_name
            .rsplit_once('.')
            .map_or((None, reported_name), |(scope, relation)| {
                (Some(scope), relation)
            });

        identity_relation == reported_relation
            && match (identity_scope, reported_scope) {
                (Some(identity_scope), Some(reported_scope)) => identity_scope == reported_scope,
                // Drivers may report an unqualified name after a schema has
                // already been supplied as a separate argument. The frozen
                // snapshot key remains the authoritative scoped identity.
                (Some(_), None) | (None, None) => true,
                // An unqualified frozen identity cannot prove that a
                // newly-qualified driver name belongs to the same schema.
                (None, Some(_)) => false,
            }
    }

    if !relation_presentation_matches(table, &reviewed.table_name)
        || !relation_presentation_matches(table, &current.table_name)
    {
        return Err(format!("Target schema changed for {table}; compare again"));
    }

    let mut reviewed_value = serde_json::to_value(reviewed).map_err(|e| e.to_string())?;
    let mut current_value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    if let Some(object) = reviewed_value.as_object_mut() {
        object.remove("tableName");
    }
    if let Some(object) = current_value.as_object_mut() {
        object.remove("tableName");
    }
    if reviewed_value != current_value {
        return Err(format!("Target schema changed for {table}; compare again"));
    }
    Ok(())
}

pub fn validate_object_snapshot(
    reviewed: &SchemaObjectSnapshot,
    current: &SchemaObjectSnapshot,
) -> Result<(), String> {
    if reviewed.kind != current.kind
        || reviewed.schema != current.schema
        || reviewed.name != current.name
        || reviewed.signature != current.signature
        || reviewed.target_schema != current.target_schema
        || reviewed.target_name != current.target_name
        || reviewed.definition.trim() != current.definition.trim()
        || reviewed.mysql_view_metadata != current.mysql_view_metadata
    {
        return Err(format!(
            "Target object {} changed after review; compare again",
            reviewed.name
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "reviewed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reviewed_store_tests.rs"]
mod store_tests;
