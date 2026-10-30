//! Schema Diff 用例编排（host-boundary F4c-3）。
//!
//! 按《宿主边界后续方案》§3 的三层约定，把桌面宿主里剩下的 Schema Diff
//! 编排收回本 feature：源/目标目录与快照读取、表级与统一计划准备、复核
//! 消费与部署前验证、单表对比。`api` 只收纯数据与已构造好的端口，不依赖
//! Tauri / Axum / `AppState`；错误走 [`SchemaDiffApiError`]，宿主经
//! `From<SchemaDiffApiError> for CommandError` 映射，IPC 载荷逐字节不变。
//!
//! ## 端口
//!
//! * [`SchemaDiffAdapterHost`] —— 跨方言类型映射与对比预览要用的 sync
//!   适配器。实现在 data_transfer 的注册表上；schema_diff 不依赖
//!   data_transfer feature，四个迁移引擎保持独立门控，适配器经端口进入。
//! * [`ComparisonPreviewHost`] —— 对比预览的 DDL 渲染与全类型读取
//!   （`data_transfer::transfer::{ddl, full_types}` 的宿主侧薄转发）。
//!
//! 会话端点（driver / handle / config）由宿主解析后以
//! [`SchemaDiffEndpoint`] 传入；`apps/desktop` 的实现保持既有错误顺序
//! （先入参校验、后会话解析），本模块对同一批入参再校验一次，供未来的
//! Web 宿主独立调用（同 F4b `read_table_page` 的双检纪律）。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use datazen_driver_api::sync::{IRTable, SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_api::{
    CatalogReader, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseObject, DriverError,
    LegacyCatalogReader, ObjectKind, SchemaMigration as _, TableSchema,
};
use serde::Serialize;

use super::compare::{diff_table_schemas, diff_table_schemas_ir};
use super::object_identity::SchemaObjectDependencySnapshot;
use super::object_selection::{
    catalog_entry_snapshot, object_identity, resolve_object_selectors,
    table_identities_from_catalog, table_identity_for_target, unique_object_selectors,
};
use super::objects::SchemaObjectSnapshot;
use super::reviewed::{ReviewedPlan, ReviewedPlanRepository, ReviewedSourceSnapshot};
use super::types::{
    normalize_dialect, resolve_table_for_dialect, uses_schema_scope, ColumnTypeOverride,
    PlanRequirement, SchemaDiffPlan, TableColumnDiff,
};
use super::unified::build_unified_schema_diff_plan_with_source_scope;
use super::unified_scope::MySqlViewScopeContext;
use crate::shared::scope::OrganizationScope;

// ---------------------------------------------------------------- 错误

/// `api` 层错误：与桌面宿主此前内联构造的 `CommandError` 变体一一对应。
///
/// `Driver` 的转换沿用宿主 `From<DriverError> for CommandError` 的既有
/// 分叉（`NotSupported` 折叠为 `Validation`），保证 IPC 字节不变。
#[derive(Debug, thiserror::Error)]
pub enum SchemaDiffApiError {
    #[error("{0}")]
    Driver(DriverError),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Internal(String),
}

impl From<DriverError> for SchemaDiffApiError {
    fn from(error: DriverError) -> Self {
        match error {
            DriverError::NotSupported(message) => Self::Validation(message),
            other => Self::Driver(other),
        }
    }
}

// ---------------------------------------------------------------- 端点与端口

/// 已解析的会话端点：宿主完成租约与门面解析后传入。
pub struct SchemaDiffEndpoint {
    pub driver: Arc<dyn DatabaseDriver>,
    pub handle: ConnectionHandle,
    pub config: ConnectionConfig,
}

/// 跨方言计划准备所需的一对 sync 适配器。
pub struct PlanSyncAdapters {
    pub source: Arc<dyn SyncSourceAdapter>,
    pub target: Arc<dyn SyncTargetAdapter>,
}

/// 单表对比预览所需的四个适配器（源/目标方言各自的读、写两侧）。
pub struct ComparisonSyncAdapters {
    pub source_of_source: Arc<dyn SyncSourceAdapter>,
    pub source_of_target: Arc<dyn SyncSourceAdapter>,
    pub target_of_source: Arc<dyn SyncTargetAdapter>,
    pub target_of_target: Arc<dyn SyncTargetAdapter>,
}

/// sync 适配器解析端口（宿主实现，桌面挂 `state.sync_adapters` 注册表）。
///
/// 端口不做业务判断；失败字符串原样映射为 `Validation`，与桌面既有
/// `ensure_pair` / `missing * sync adapter` 文本一致。
pub trait SchemaDiffAdapterHost: Send + Sync {
    /// 跨方言计划准备的适配器对；错误文本即 IPC 文本。
    fn resolve_plan_pair(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Result<PlanSyncAdapters, String>;
    /// 对比预览的四个适配器；任一缺失即 `None`（调用方回退原始对比）。
    fn comparison_adapters(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Option<ComparisonSyncAdapters>;
}

/// 对比预览端口：DDL 渲染与全类型读取由 data_transfer 提供，经宿主转发。
#[async_trait::async_trait]
pub trait ComparisonPreviewHost: Send + Sync {
    /// 适配器声明的全类型查询；失败按「无全类型」处理（桌面原 `.ok()`）。
    async fn full_column_types(
        &self,
        adapter: &dyn SyncSourceAdapter,
        driver: &dyn DatabaseDriver,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Option<HashMap<String, String>>;
    /// 以目标方言渲染建表 DDL 预览。
    fn create_table_ddl(&self, ir_table: &IRTable, target: &dyn SyncTargetAdapter) -> String;
}

// ---------------------------------------------------------------- 纯助手

pub fn is_table_missing_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("1146")
        || lower.contains("doesn't exist")
        || lower.contains("does not exist")
        || lower.contains("not found")
}

/// Convert a connection's configured database value to the catalog identifier
/// expected by metadata methods. SQLite stores a file path in the connection
/// config while its single default catalog is named `main`.
pub fn schema_catalog_database<'a>(
    database_type: &str,
    configured_database: Option<&'a str>,
) -> &'a str {
    if normalize_dialect(database_type) == "sqlite" {
        "main"
    } else {
        configured_database.unwrap_or_default()
    }
}

pub fn schema_catalog_scope(
    database_type: &str,
    configured_database: Option<&str>,
) -> Option<String> {
    if normalize_dialect(database_type) == "sqlite" {
        Some("main".into())
    } else {
        configured_database.map(str::to_owned)
    }
}

pub fn resolve_profile_table(dialect: &str, table: &str, schema: Option<&str>) -> String {
    let Some(schema) = schema.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolve_table_for_dialect(dialect, table);
    };
    let relation = table
        .trim()
        .rsplit_once('.')
        .map(|(_, name)| name)
        .unwrap_or_else(|| table.trim());
    format!("{schema}.{relation}")
}

fn dependency_relation_identity(dialect: &str, table: &str, schema_scope: Option<&str>) -> String {
    let table = table.trim();
    if uses_schema_scope(dialect) && !table.contains('.') {
        if let Some(schema) = schema_scope.filter(|value| !value.trim().is_empty()) {
            return format!("{}.{}", schema.trim(), table);
        }
    }
    table.to_string()
}

pub fn resolve_reviewed_table_snapshot(
    dialect: &str,
    table: &str,
    database_scope: &str,
    schema_scope: Option<&str>,
) -> (String, String, Option<String>) {
    if uses_schema_scope(dialect) {
        if let Some((schema, relation)) = table.rsplit_once('.') {
            return (
                relation.to_string(),
                database_scope.to_string(),
                Some(schema.to_string()),
            );
        }
    }
    if normalize_dialect(dialect) == "mysql" {
        if let Some((database, relation)) = table.rsplit_once('.') {
            return (relation.to_string(), database.to_string(), None);
        }
    }
    (
        table.to_string(),
        database_scope.to_string(),
        schema_scope.map(str::to_owned),
    )
}

pub fn has_table_drop_statement(plan: &SchemaDiffPlan) -> bool {
    plan.statements.iter().any(|statement| {
        statement
            .summary
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("DROP TABLE")
    })
}

pub fn validate_target_dependency_catalog(
    reviewed: &[(String, TableSchema)],
    current: &[(String, TableSchema)],
) -> Result<(), String> {
    fn catalog_map<'a>(
        snapshots: &'a [(String, TableSchema)],
    ) -> Result<BTreeMap<&'a str, &'a TableSchema>, String> {
        let mut catalog = BTreeMap::new();
        for (identity, snapshot) in snapshots {
            if catalog.insert(identity.as_str(), snapshot).is_some() {
                return Err(format!(
                    "Target dependency catalog contains duplicate relation identity `{identity}`"
                ));
            }
        }
        Ok(catalog)
    }

    let reviewed = catalog_map(reviewed)?;
    let current = catalog_map(current)?;
    if reviewed.len() != current.len()
        || reviewed
            .keys()
            .any(|identity| !current.contains_key(identity))
    {
        let added = current
            .keys()
            .filter(|identity| !reviewed.contains_key(**identity))
            .copied()
            .collect::<Vec<_>>();
        let removed = reviewed
            .keys()
            .filter(|identity| !current.contains_key(*identity))
            .copied()
            .collect::<Vec<_>>();
        return Err(format!(
            "Target dependency catalog changed after review (new relations: [{}]; removed relations: [{}]); compare again before dropping tables",
            added.join(", "),
            removed.join(", ")
        ));
    }

    for (identity, reviewed_snapshot) in reviewed {
        let current_snapshot = current
            .get(identity)
            .ok_or_else(|| format!("Target relation `{identity}` disappeared after review"))?;
        super::reviewed::validate_snapshot(identity, reviewed_snapshot, current_snapshot)?;
    }
    Ok(())
}

/// Object-only plans do not have the unified planner's SQL Server schema
/// mapper. Keep their source and target objects in the same configured schema
/// so a plan cannot render source-schema DDL or drop a same-named object from
/// another target schema.
pub fn sqlserver_object_scope_requirement(
    source_dialect: &str,
    target_dialect: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    source: &[SchemaObjectSnapshot],
    target: &[SchemaObjectSnapshot],
) -> Option<PlanRequirement> {
    if source_dialect != "sqlserver" || target_dialect != "sqlserver" {
        return None;
    }

    // SQL Server's driver-level default schema is dbo when no explicit schema
    // is configured. Object snapshots always carry their catalog schema.
    let source_scope = source_schema
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or("dbo");
    let target_scope = target_schema
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or("dbo");
    let source_matches_scope = source.iter().all(|object| {
        object.schema.as_deref() == Some(source_scope)
            && object
                .target_schema
                .as_deref()
                .is_none_or(|schema| schema == source_scope)
    });
    let target_matches_scope = target.iter().all(|object| {
        object.schema.as_deref() == Some(target_scope)
            && object
                .target_schema
                .as_deref()
                .is_none_or(|schema| schema == target_scope)
    });

    if source_scope == target_scope && source_matches_scope && target_matches_scope {
        return None;
    }

    Some(PlanRequirement::Unsupported {
        operation: "sqlserver-object-schema-scope".into(),
        reason: format!(
            "SQL Server object-only plans cannot rewrite object definitions across schemas. Source and target objects must both match the same configured schema; source scope is `{source_scope}`, target scope is `{target_scope}`."
        ),
    })
}

pub fn apply_sqlserver_object_scope_gate(
    plan: &mut SchemaDiffPlan,
    source_dialect: &str,
    target_dialect: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    source: &[SchemaObjectSnapshot],
    target: &[SchemaObjectSnapshot],
) {
    if let Some(requirement) = sqlserver_object_scope_requirement(
        source_dialect,
        target_dialect,
        source_schema,
        target_schema,
        source,
        target,
    ) {
        plan.requirements.push(requirement);
        plan.statements.clear();
    }
}

fn object_selector(object: &DatabaseObject) -> String {
    object
        .schema
        .as_deref()
        .map(|schema| format!("{schema}.{}", object.name))
        .unwrap_or_else(|| object.name.clone())
}

fn object_selector_with_metadata(object: &DatabaseObject) -> String {
    let base = object_selector(object);
    match ObjectKind::parse(&object.kind) {
        Some(ObjectKind::Function) | Some(ObjectKind::Procedure) => object
            .signature
            .as_deref()
            .map(|signature| format!("{base}({signature})"))
            .unwrap_or(base),
        Some(ObjectKind::Trigger) => object
            .target_name
            .as_deref()
            .map(|target| {
                let target = object
                    .target_schema
                    .as_deref()
                    .map(|schema| format!("{schema}.{target}"))
                    .unwrap_or_else(|| target.to_owned());
                format!("{base} ON {target}")
            })
            .unwrap_or(base),
        _ => base,
    }
}

pub fn select_schema_object_pair(
    source: &[DatabaseObject],
    target: &[DatabaseObject],
    requested: &[String],
) -> Result<(Vec<DatabaseObject>, Vec<DatabaseObject>), SchemaDiffApiError> {
    let mut source_selected = Vec::new();
    let mut target_selected = Vec::new();
    for raw in requested {
        let name = raw.trim();
        let source_matches = source
            .iter()
            .filter(|object| {
                object.name == name
                    || object_selector(object) == name
                    || object_selector_with_metadata(object) == name
            })
            .collect::<Vec<_>>();
        let target_matches = target
            .iter()
            .filter(|object| {
                object.name == name
                    || object_selector(object) == name
                    || object_selector_with_metadata(object) == name
            })
            .collect::<Vec<_>>();
        if source_matches.len() > 1
            || target_matches.len() > 1
            || (source_matches.is_empty() && target_matches.is_empty())
        {
            return Err(SchemaDiffApiError::Validation(format!(
                "Schema object selector `{name}` must identify exactly one available object"
            )));
        }
        if let Some(object) = source_matches.first() {
            source_selected.push((*object).clone());
        }
        if let Some(object) = target_matches.first() {
            target_selected.push((*object).clone());
        }
    }
    Ok((source_selected, target_selected))
}

// ---------------------------------------------------------------- 目录/快照读取

pub async fn fetch_target_table_schema(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<TableSchema, SchemaDiffApiError> {
    // 表结构读取走窄契约，`driver_type()` 仍是驱动专属规则。
    let catalog = LegacyCatalogReader::new(driver);
    let database = schema_catalog_database(&driver.driver_type(), Some(database));
    match catalog
        .get_table_schema(handle, table, database, schema)
        .await
    {
        Ok(schema) => Ok(schema),
        Err(e) => {
            let msg = e.to_string();
            if is_table_missing_error(&msg) {
                tracing::info!(%table, "target table does not exist, treating as empty schema");
                Ok(TableSchema {
                    table_name: table.to_string(),
                    columns: Vec::new(),
                    primary_keys: Vec::new(),
                    indexes: Vec::new(),
                    foreign_keys: Vec::new(),
                    check_constraints: Vec::new(),
                    table_options: datazen_driver_api::TableOptions::default(),
                })
            } else {
                Err(SchemaDiffApiError::Driver(e))
            }
        }
    }
}

pub async fn fetch_target_table_dependency_catalog(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    selected_snapshots: &[(String, TableSchema)],
) -> Result<Vec<(String, TableSchema)>, SchemaDiffApiError> {
    const CATALOG_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    let started_at = std::time::Instant::now();
    let metadata_database = schema_catalog_database(dialect, Some(database));
    // 目录读取走窄契约；外键可见性检查仍是驱动专属规则。
    let catalog = LegacyCatalogReader::new(driver);
    let (snapshots, table_count) = tokio::time::timeout(CATALOG_READ_TIMEOUT, async {
        let databases = if normalize_dialect(dialect) == "mysql" {
            if !driver
                .has_complete_foreign_key_catalog_visibility(handle)
                .await
                .map_err(SchemaDiffApiError::Driver)?
            {
                return Err(SchemaDiffApiError::Validation(
                    "MySQL target-only table drops require direct global SELECT or ALL PRIVILEGES on *.* with no partial REVOKE so every database dependency can be inspected; use a suitably privileged connection and retry.".into(),
                ));
            }
            let databases = catalog
                .get_databases(handle)
                .await
                .map_err(SchemaDiffApiError::Driver)?;
            if !databases.iter().any(|candidate| candidate == database) {
                return Err(SchemaDiffApiError::Validation(
                    "MySQL reported an incomplete database list despite global metadata visibility; target-only table drops are blocked.".into(),
                ));
            }
            databases
        } else {
            vec![metadata_database.to_string()]
        };
        // This must be a catalog read, not a union with individually fetched
        // selected tables: adding selected snapshots here could make an
        // incomplete catalog look complete and hide an inbound FK dependent.
        let mut snapshots = Vec::new();
        let mut seen = HashSet::new();
        let mut table_count = 0usize;

        for catalog_database in databases {
            let tables = catalog
                .get_tables(handle, &catalog_database, None)
                .await
                .map_err(SchemaDiffApiError::Driver)?;
            table_count += tables
                .iter()
                .filter(|table| matches!(&table.table_type, datazen_driver_api::TableType::Table))
                .count();
            for table in tables {
                if !matches!(&table.table_type, datazen_driver_api::TableType::Table) {
                    continue;
                }
                let relation = table
                    .schema
                    .as_deref()
                    .filter(|schema| !schema.is_empty())
                    .map(|schema| format!("{schema}.{}", table.name))
                    .unwrap_or_else(|| table.name.clone());
                let table_identity = if normalize_dialect(dialect) == "mysql"
                    && catalog_database != database
                {
                    format!("{catalog_database}.{relation}")
                } else {
                    relation
                };
                let table_identity =
                    dependency_relation_identity(dialect, &table_identity, schema_scope);
                if !seen.insert(table_identity.clone()) {
                    continue;
                }
                let mut schema = fetch_target_table_schema(
                    driver,
                    handle,
                    &table.name,
                    &catalog_database,
                    table.schema.as_deref(),
                )
                .await?;
                if normalize_dialect(dialect) == "mysql" {
                    for foreign_key in &mut schema.foreign_keys {
                        if let Some((referenced_database, referenced_table)) =
                            foreign_key.referenced_table.split_once('.')
                        {
                            if referenced_database == database {
                                foreign_key.referenced_table = referenced_table.to_string();
                            }
                        }
                    }
                }
                snapshots.push((table_identity, schema));
            }
        }
        for (selected_table, _) in selected_snapshots {
            let mut selected_identity =
                dependency_relation_identity(dialect, selected_table, schema_scope);
            if normalize_dialect(dialect) == "mysql" {
                if let Some((selected_database, relation)) = selected_identity.split_once('.') {
                    if selected_database == database {
                        selected_identity = relation.to_string();
                    }
                }
            }
            if snapshots
                .iter()
                .any(|(identity, _)| identity == &selected_identity)
            {
                continue;
            }

            if uses_schema_scope(dialect) && !selected_table.contains('.') {
                let candidates = snapshots
                    .iter()
                    .filter(|(identity, _)| {
                        identity.rsplit('.').next() == Some(selected_table.as_str())
                            && schema_scope
                                .map(str::trim)
                                .filter(|schema| !schema.is_empty())
                                .map(|schema| identity.starts_with(&format!("{schema}.")))
                                .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                match candidates.as_slice() {
                    [_] => continue,
                    [] => {
                        return Err(SchemaDiffApiError::Validation(format!(
                            "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
                        )));
                    }
                    _ => {
                        return Err(SchemaDiffApiError::Validation(format!(
                            "Selected target table `{selected_table}` has an ambiguous schema identity in the complete target dependency catalog"
                        )));
                    }
                }
            }

            return Err(SchemaDiffApiError::Validation(format!(
                "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
            )));
        }

        Ok::<_, SchemaDiffApiError>((snapshots, table_count))
    })
    .await
    .map_err(|_| {
        SchemaDiffApiError::Internal(
            "Reading the target foreign-key dependency catalog exceeded 30 seconds; scope the target selection or retry after reducing catalog load.".into(),
        )
    })??;

    tracing::info!(
        table_count,
        elapsed_ms = started_at.elapsed().as_millis(),
        "Schema Diff target dependency catalog read"
    );
    Ok(snapshots)
}

pub async fn ensure_distinct_schema_scope(
    source_driver: &dyn DatabaseDriver,
    source_handle: &ConnectionHandle,
    source_config: &ConnectionConfig,
    target_driver: &dyn DatabaseDriver,
    target_handle: &ConnectionHandle,
    target_config: &ConnectionConfig,
    source_schema_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<(), SchemaDiffApiError> {
    let source_identity = source_driver
        .physical_database_identity(
            source_handle,
            source_config.database.as_deref().unwrap_or_default(),
        )
        .await
        .ok()
        .flatten();
    let target_identity = target_driver
        .physical_database_identity(
            target_handle,
            target_config.database.as_deref().unwrap_or_default(),
        )
        .await
        .ok()
        .flatten();
    let is_sqlserver = normalize_dialect(&source_config.database_type) == "sqlserver"
        && normalize_dialect(&target_config.database_type) == "sqlserver";
    let same_physical_database = matches!(
        (source_identity.as_deref(), target_identity.as_deref()),
        (Some(source), Some(target)) if source == target
    );
    let source_schema = source_schema_scope
        .and_then(|schema| (!schema.trim().is_empty()).then_some(schema))
        .or_else(|| {
            source_config
                .schema
                .as_deref()
                .filter(|schema| !schema.trim().is_empty())
        });
    let target_schema = target_schema_scope
        .and_then(|schema| (!schema.trim().is_empty()).then_some(schema))
        .or_else(|| {
            target_config
                .schema
                .as_deref()
                .filter(|schema| !schema.trim().is_empty())
        });
    let (source_schema_identity, target_schema_identity) = if is_sqlserver && same_physical_database
    {
        let source_schema_identity = match source_schema {
            Some(schema) => source_driver
                .schema_scope_identity(
                    source_handle,
                    source_config.database.as_deref().unwrap_or_default(),
                    schema,
                )
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let target_schema_identity = match target_schema {
            Some(schema) => target_driver
                .schema_scope_identity(
                    target_handle,
                    target_config.database.as_deref().unwrap_or_default(),
                    schema,
                )
                .await
                .ok()
                .flatten(),
            None => None,
        };
        (source_schema_identity, target_schema_identity)
    } else {
        (None, None)
    };

    match super::reviewed::physical_database_scope(
        source_config,
        target_config,
        source_identity.as_deref(),
        target_identity.as_deref(),
        source_schema_scope,
        target_schema_scope,
        source_schema_identity.as_deref(),
        target_schema_identity.as_deref(),
    ) {
        super::reviewed::PhysicalDatabaseScope::Same => Err(SchemaDiffApiError::Validation(
            "Source and target must identify different database scopes".into(),
        )),
        super::reviewed::PhysicalDatabaseScope::Different => Ok(()),
        super::reviewed::PhysicalDatabaseScope::Unknown => {
            Err(SchemaDiffApiError::Validation(
                "Cannot verify that source and target are different database scopes; the driver must provide physical database identity".into(),
            ))
        }
    }
}

pub async fn list_schema_objects(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    kind: ObjectKind,
) -> Result<Vec<DatabaseObject>, SchemaDiffApiError> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "list_objects",
        serde_json::json!({ "kind": kind.as_str() }),
    )
    .await
    .map_err(SchemaDiffApiError::Driver)?;
    result
        .data
        .get("objects")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            SchemaDiffApiError::Internal(format!("invalid {} metadata: {error}", kind.as_str()))
        })
        .map(|objects| objects.unwrap_or_default())
}

pub async fn fetch_schema_object(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    object: &DatabaseObject,
) -> Result<SchemaObjectSnapshot, SchemaDiffApiError> {
    let kind = ObjectKind::parse(&object.kind).ok_or_else(|| {
        SchemaDiffApiError::Validation(format!("Unsupported schema object kind `{}`", object.kind))
    })?;
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "get_object_ddl",
        serde_json::json!({
            "kind": kind.as_str(),
            "name": object.name,
            "schema": object.schema,
            "signature": object.signature,
            "targetSchema": object.target_schema,
            "targetName": object.target_name,
        }),
    )
    .await
    .map_err(SchemaDiffApiError::Driver)?;
    let definition = result
        .data
        .get("ddl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mysql_view_metadata = result
        .data
        .get("viewMetadata")
        .cloned()
        .map(serde_json::from_value::<datazen_driver_api::MySqlViewMetadata>)
        .transpose()
        .map_err(|error| {
            SchemaDiffApiError::Internal(format!("invalid MySQL view creation metadata: {error}"))
        })?;
    if definition.is_empty() {
        return Err(SchemaDiffApiError::Validation(format!(
            "{} {} disappeared while it was being inspected",
            kind.as_str(),
            object.name
        )));
    }
    Ok(match kind {
        ObjectKind::View => {
            let mut snapshot =
                SchemaObjectSnapshot::view(object.schema.as_deref(), &object.name, &definition);
            snapshot.mysql_view_metadata = mysql_view_metadata;
            snapshot
        }
        ObjectKind::Function | ObjectKind::Procedure => SchemaObjectSnapshot::routine(
            kind,
            object.schema.as_deref(),
            &object.name,
            object.signature.as_deref(),
            &definition,
        ),
        ObjectKind::Trigger => SchemaObjectSnapshot::trigger(
            object.schema.as_deref(),
            &object.name,
            object.target_schema.as_deref(),
            object.target_name.as_deref().ok_or_else(|| {
                SchemaDiffApiError::Validation(format!(
                    "Trigger {} has no target relation",
                    object.name
                ))
            })?,
            &definition,
        ),
        ObjectKind::Sequence => {
            SchemaObjectSnapshot::sequence(object.schema.as_deref(), &object.name, &definition)
        }
        ObjectKind::Type => SchemaObjectSnapshot::type_definition(
            object.schema.as_deref(),
            &object.name,
            &definition,
        ),
        _ => {
            return Err(SchemaDiffApiError::Validation(format!(
                "{} migration is not supported",
                kind.as_str()
            )))
        }
    })
}

pub async fn read_target_object_catalog(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    complete: &mut bool,
) -> (Vec<DatabaseObject>, Vec<SchemaObjectDependencySnapshot>) {
    let metadata_database = schema_catalog_database(dialect, Some(database));
    // 表清单读取走窄契约；对象依赖查询仍是驱动命令通道。
    let catalog = LegacyCatalogReader::new(driver);
    let mut objects = Vec::new();
    for kind in UNIFIED_OBJECT_KINDS {
        match list_schema_objects(driver, handle, kind).await {
            Ok(list) => objects.extend(list),
            Err(error) => {
                *complete = false;
                tracing::warn!(kind = kind.as_str(), %error, "Unified target object catalog is incomplete");
            }
        }
    }
    let mut dependency_objects = objects.clone();
    match catalog.get_tables(handle, metadata_database, None).await {
        Ok(tables) => {
            dependency_objects.extend(
                tables
                    .into_iter()
                    .filter(|table| {
                        matches!(table.table_type, datazen_driver_api::TableType::Table)
                    })
                    .map(|table| DatabaseObject {
                        kind: ObjectKind::Table.as_str().into(),
                        schema: table.schema.or_else(|| {
                            if uses_schema_scope(dialect) {
                                schema_scope.map(str::to_owned)
                            } else {
                                Some(metadata_database.to_owned())
                            }
                        }),
                        name: table.name,
                        signature: None,
                        target_schema: None,
                        target_name: None,
                    }),
            );
        }
        Err(error) => {
            *complete = false;
            tracing::warn!(%error, "Unified target table catalog is incomplete");
        }
    }
    let mut identities = HashSet::new();
    let mut snapshots = Vec::with_capacity(dependency_objects.len());
    for object in &dependency_objects {
        if ObjectKind::parse(&object.kind).is_none() {
            *complete = false;
            continue;
        }
        let identity = object_identity(object);
        if !identities.insert(identity.clone()) {
            *complete = false;
            continue;
        }
        let snapshot = fetch_object_dependency_snapshot(driver, handle, object).await;
        snapshots.push(snapshot.unwrap_or_else(|| SchemaObjectDependencySnapshot {
            identity,
            dependencies: None,
            type_dependency_usages: None,
            sequence_dependency_usages: None,
        }));
    }
    (objects, snapshots)
}

pub async fn fetch_object_snapshot(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    object: &DatabaseObject,
) -> Result<SchemaObjectSnapshot, SchemaDiffApiError> {
    let mut snapshot = fetch_schema_object(driver, handle, object).await?;
    if let Some(dependencies) = fetch_object_dependency_snapshot(driver, handle, object).await {
        snapshot.dependencies = dependencies.dependencies;
        snapshot.sequence_dependency_usages = dependencies.sequence_dependency_usages;
    }
    Ok(snapshot)
}

pub async fn fetch_object_dependency_snapshot(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    object: &DatabaseObject,
) -> Option<SchemaObjectDependencySnapshot> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "get_object_dependencies",
        serde_json::json!({
            "kind": object.kind,
            "name": object.name,
            "schema": object.schema,
            "signature": object.signature,
            "targetSchema": object.target_schema,
            "targetName": object.target_name,
        }),
    )
    .await
    .ok()?;
    if result
        .data
        .get("complete")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return None;
    }
    let dependencies =
        serde_json::from_value::<Vec<DatabaseObject>>(result.data.get("dependencies")?.clone())
            .ok()?;
    let mut identities = dependencies
        .iter()
        .map(strict_object_identity)
        .collect::<Option<Vec<_>>>()?;
    identities.sort();
    identities.dedup();
    let type_dependency_usages = match result.data.get("typeDependencyUsages") {
        Some(value) => {
            let usages =
                serde_json::from_value::<Vec<TypeDependencyUsageWire>>(value.clone()).ok()?;
            let mut parsed = Vec::with_capacity(usages.len());
            for usage in usages {
                let kind = match usage.usage.as_str() {
                    "column_type" => super::object_identity::TypeDependencyUsageKind::ColumnType,
                    "expression" => super::object_identity::TypeDependencyUsageKind::Expression,
                    "constraint" => super::object_identity::TypeDependencyUsageKind::Constraint,
                    _ => return None,
                };
                let column_name = match kind {
                    super::object_identity::TypeDependencyUsageKind::ColumnType => {
                        let column = usage.column_name.filter(|name| !name.trim().is_empty())?;
                        Some(column)
                    }
                    super::object_identity::TypeDependencyUsageKind::Expression
                    | super::object_identity::TypeDependencyUsageKind::Constraint => {
                        if usage.column_name.is_some() {
                            return None;
                        }
                        None
                    }
                };
                parsed.push(super::object_identity::TypeDependencyUsage {
                    dependency: strict_object_identity(&usage.dependency)?,
                    usage: kind,
                    column_name,
                });
            }
            parsed.sort();
            parsed.dedup();
            if parsed.iter().any(|usage| {
                usage.dependency.kind != ObjectKind::Type || !identities.contains(&usage.dependency)
            }) {
                return None;
            }
            Some(parsed)
        }
        None => None,
    };
    let sequence_dependency_usages = match result.data.get("sequenceDependencyUsages") {
        Some(value) => {
            let usages =
                serde_json::from_value::<Vec<SequenceDependencyUsageWire>>(value.clone()).ok()?;
            let mut parsed = Vec::with_capacity(usages.len());
            for usage in usages {
                let sequence = strict_object_identity(&usage.sequence)?;
                let owner_table = strict_object_identity(&usage.owner_table)?;
                let column_name = usage.column_name.filter(|name| !name.trim().is_empty())?;
                if sequence.kind != ObjectKind::Sequence || owner_table.kind != ObjectKind::Table {
                    return None;
                }
                let usage_kind = match usage.usage.as_str() {
                    "column_default" => {
                        if object_identity(object).kind != ObjectKind::Table
                            || owner_table != object_identity(object)
                            || !identities.contains(&sequence)
                        {
                            return None;
                        }
                        super::object_identity::SequenceDependencyUsageKind::ColumnDefault
                    }
                    "owned_by" => {
                        if object_identity(object).kind != ObjectKind::Sequence
                            || sequence != object_identity(object)
                            || !identities.contains(&owner_table)
                        {
                            return None;
                        }
                        super::object_identity::SequenceDependencyUsageKind::OwnedBy
                    }
                    _ => return None,
                };
                parsed.push(super::object_identity::SequenceDependencyUsage {
                    sequence,
                    owner_table,
                    column_name,
                    usage: usage_kind,
                });
            }
            parsed.sort();
            parsed.dedup();
            Some(parsed)
        }
        None => None,
    };
    Some(SchemaObjectDependencySnapshot {
        identity: object_identity(object),
        dependencies: Some(identities),
        type_dependency_usages,
        sequence_dependency_usages,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeDependencyUsageWire {
    dependency: DatabaseObject,
    usage: String,
    column_name: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SequenceDependencyUsageWire {
    sequence: DatabaseObject,
    owner_table: DatabaseObject,
    column_name: Option<String>,
    usage: String,
}

fn strict_object_identity(
    object: &DatabaseObject,
) -> Option<super::object_identity::SchemaObjectIdentity> {
    Some(super::object_identity::SchemaObjectIdentity {
        kind: ObjectKind::parse(&object.kind)?,
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    })
}

async fn read_selected_object_catalog(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    selectors: &[DatabaseObject],
) -> Result<Vec<DatabaseObject>, SchemaDiffApiError> {
    let mut catalog = Vec::new();
    let kinds = selectors
        .iter()
        .filter_map(|selector| ObjectKind::parse(&selector.kind))
        .collect::<BTreeSet<_>>();
    for kind in kinds {
        catalog.extend(list_schema_objects(driver, handle, kind).await?);
    }
    Ok(catalog)
}

pub async fn read_target_table_identities(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
) -> Result<Vec<super::object_identity::SchemaObjectIdentity>, SchemaDiffApiError> {
    let metadata_database = schema_catalog_database(dialect, Some(database));
    // 表清单读取走窄契约，而不是整块驱动。
    let catalog = LegacyCatalogReader::new(driver);
    let tables = catalog
        .get_tables(handle, metadata_database, None)
        .await
        .map_err(SchemaDiffApiError::Driver)?;
    Ok(table_identities_from_catalog(
        &tables,
        dialect,
        schema_scope,
        Some(metadata_database),
    ))
}

pub async fn read_mysql_view_scope_context(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
) -> Option<MySqlViewScopeContext> {
    let result = driver
        .query(
            handle,
            "SELECT CURRENT_USER() AS `current_user`, @@character_set_client AS `character_set_client`, @@collation_connection AS `collation_connection`",
        )
        .await
        .ok()?;
    let read = |name: &str| {
        let index = result
            .columns
            .iter()
            .position(|column| column.name.eq_ignore_ascii_case(name))?;
        match result.rows.first()?.get(index)?.as_ref()? {
            datazen_driver_api::Value::String(value) => Some(value.clone()),
            datazen_driver_api::Value::Bytes(value) => {
                std::str::from_utf8(value).ok().map(str::to_owned)
            }
            _ => None,
        }
    };
    Some(MySqlViewScopeContext {
        current_user: read("current_user")?,
        character_set_client: read("character_set_client")?,
        collation_connection: read("collation_connection")?,
    })
}

const UNIFIED_OBJECT_KINDS: [ObjectKind; 6] = [
    ObjectKind::View,
    ObjectKind::Function,
    ObjectKind::Procedure,
    ObjectKind::Trigger,
    ObjectKind::Sequence,
    ObjectKind::Type,
];

// ---------------------------------------------------------------- 用例：表级计划

/// 表级计划准备请求（source = desired → target）。
#[allow(clippy::struct_field_names)]
pub struct PrepareTablePlanRequest<'a> {
    pub source: &'a SchemaDiffEndpoint,
    pub target: &'a SchemaDiffEndpoint,
    pub source_session_id: &'a str,
    pub target_session_id: &'a str,
    pub table_names: Vec<String>,
    pub target_table_names: Vec<String>,
    pub target_only_table_names: Vec<String>,
    pub allow_destructive: bool,
    pub include_indexes: Option<bool>,
    pub type_overrides: Option<Vec<ColumnTypeOverride>>,
    pub source_schema: Option<&'a str>,
    pub target_schema: Option<&'a str>,
    pub repository: &'a dyn ReviewedPlanRepository,
    pub scope: &'a OrganizationScope,
    pub adapters: &'a dyn SchemaDiffAdapterHost,
}

/// Prepare a DDL deploy plan (source = desired → target).
pub async fn prepare_table_plan(
    request: PrepareTablePlanRequest<'_>,
) -> Result<SchemaDiffPlan, SchemaDiffApiError> {
    let PrepareTablePlanRequest {
        source,
        target,
        source_session_id,
        target_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        allow_destructive,
        include_indexes,
        type_overrides,
        source_schema,
        target_schema,
        repository,
        scope,
        adapters,
    } = request;
    tracing::info!(
        %source_session_id,
        %target_session_id,
        tables = table_names.len(),
        allow_destructive,
        "prepare_schema_diff_plan"
    );

    if table_names.is_empty() && target_only_table_names.is_empty() {
        return Err(SchemaDiffApiError::Validation(
            "table_names and target_only_table_names must not both be empty".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(SchemaDiffApiError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }

    let src_config = &source.config;
    let tgt_config = &target.config;

    if tgt_config.read_only {
        return Err(SchemaDiffApiError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    let source_schema_scope = source_schema.or(src_config.schema.as_deref());
    let target_schema_scope = target_schema.or(tgt_config.schema.as_deref());
    if super::reviewed::same_endpoint(src_config, tgt_config)
        && source_schema_scope.map(str::trim) == target_schema_scope.map(str::trim)
    {
        return Err(SchemaDiffApiError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }

    let src_driver = source.driver.as_ref();
    let src_handle = &source.handle;
    let tgt_driver = target.driver.as_ref();
    let tgt_handle = &target.handle;
    // 表结构读取走窄契约，而不是整块驱动。
    let src_catalog = LegacyCatalogReader::new(src_driver);
    let tgt_catalog = LegacyCatalogReader::new(tgt_driver);
    let target_dependency_schema_scope = if uses_schema_scope(&tgt_config.database_type) {
        target_schema_scope.or(tgt_driver.default_schema())
    } else {
        None
    };
    ensure_distinct_schema_scope(
        src_driver,
        src_handle,
        src_config,
        tgt_driver,
        tgt_handle,
        tgt_config,
        source_schema_scope,
        target_schema_scope,
    )
    .await?;

    let mut pairs = Vec::new();
    for (table, target_table) in table_names.iter().zip(&target_table_names) {
        let src_table = resolve_profile_table(&src_config.database_type, table, source_schema);
        let tgt_table =
            resolve_profile_table(&tgt_config.database_type, target_table, target_schema);
        let src_schema_snapshot = src_catalog
            .get_table_schema(
                src_handle,
                &src_table,
                schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
                source_schema.or(src_config.schema.as_deref()),
            )
            .await?;
        let tgt_schema_snapshot = fetch_target_table_schema(
            tgt_driver,
            tgt_handle,
            &tgt_table,
            tgt_config.database.as_deref().unwrap_or_default(),
            target_schema.or(tgt_config.schema.as_deref()),
        )
        .await?;
        // DDL in the plan targets the target dialect, so the pair's table identifier must be
        // target-resolved. Using `table` here leaks the source's schema qualification into the
        // target DDL (e.g. `public.table` on MySQL) and breaks deploy.
        pairs.push((tgt_table, src_schema_snapshot, tgt_schema_snapshot));
    }

    let mut target_only_snapshots = Vec::new();
    let mut target_only_tables = Vec::new();
    let source_target_tables = pairs
        .iter()
        .map(|(table, _, _)| table.as_str())
        .collect::<HashSet<_>>();
    let mut seen_target_only_tables = HashSet::new();
    for table in &target_only_table_names {
        let tgt_table = resolve_profile_table(&tgt_config.database_type, table, target_schema);
        // Target-only selection is an explicit destructive request. Read the
        // real target schema and fail closed if the selected table disappeared;
        // do not turn it into an empty source schema sentinel.
        let tgt_schema_snapshot = tgt_catalog
            .get_table_schema(
                tgt_handle,
                &tgt_table,
                schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
                target_schema.or(tgt_config.schema.as_deref()),
            )
            .await?;
        if source_target_tables.contains(tgt_table.as_str()) {
            return Err(SchemaDiffApiError::Validation(format!(
                "Table `{tgt_table}` cannot be selected as both a source table and target-only table"
            )));
        }
        if !seen_target_only_tables.insert(tgt_table.clone()) {
            return Err(SchemaDiffApiError::Validation(format!(
                "Target-only table `{tgt_table}` was selected more than once"
            )));
        }
        target_only_tables.push(tgt_table.clone());
        target_only_snapshots.push((tgt_table, tgt_schema_snapshot));
    }

    let mut selected_target_snapshots = pairs
        .iter()
        .map(|(table, _, target)| (table.clone(), target.clone()))
        .collect::<Vec<_>>();
    selected_target_snapshots.extend(target_only_snapshots.iter().cloned());

    let (target_dependency_catalog, target_dependency_catalog_error) =
        if allow_destructive && !target_only_tables.is_empty() {
            match fetch_target_table_dependency_catalog(
                tgt_driver,
                tgt_handle,
                tgt_config.database.as_deref().unwrap_or_default(),
                &tgt_config.database_type,
                target_dependency_schema_scope,
                &selected_target_snapshots,
            )
            .await
            {
                Ok(catalog) => (Some(catalog), None),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let include_indexes = include_indexes.unwrap_or(true);

    let mut plan = if src_d != tgt_d {
        let pair = adapters
            .resolve_plan_pair(&src_config.database_type, &tgt_config.database_type)
            .map_err(SchemaDiffApiError::Validation)?;
        let src_adapter = pair.source;
        let tgt_adapter = pair.target;

        let tgt_is_mysql = tgt_d == "mysql";
        let mapper = |table: &str, source_type: &str, col_name: &str| -> Result<String, String> {
            for (tgt_tbl, src, _) in pairs.iter().filter(|(t, _, _)| t == table) {
                let matching_col = src
                    .columns
                    .iter()
                    .find(|c| c.name == col_name && c.data_type == source_type)
                    .or_else(|| src.columns.iter().find(|c| c.name == col_name));
                if let Some(col) = matching_col {
                    // 1. User explicit column type overrides take highest priority
                    if let Some(ref overrides) = type_overrides {
                        if let Some(ov) = overrides.iter().find(|o| {
                            resolve_profile_table(
                                &tgt_config.database_type,
                                &o.table,
                                target_schema,
                            ) == *tgt_tbl
                                && o.column == col_name
                        }) {
                            return Ok(ov.target_type.clone());
                        }
                    }

                    // 2. MySQL target: unbounded text columns default to pre-filled suggested VARCHAR(255)
                    if tgt_is_mysql && super::plan::is_source_unbounded_text(&col.data_type) {
                        return Ok("VARCHAR(255)".into());
                    }

                    let ir = src_adapter.column_to_ir(col, Some(source_type));
                    if col.default_value.is_some()
                        && !tgt_adapter.allows_column_default(&ir.ir_type)
                    {
                        if let Some(fallback) = tgt_adapter.default_capable_type_for(&ir.ir_type) {
                            return Ok(tgt_adapter.ir_type_to_native(&fallback));
                        }
                    }
                    return Ok(tgt_adapter.ir_type_to_native(&ir.ir_type));
                }
            }
            Err(format!(
                "cannot map type `{source_type}` for column `{col_name}`"
            ))
        };

        super::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            super::plan::PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: Some(&mapper),
                cross_dialect: true,
            },
        )
    } else {
        super::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            super::plan::PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: None,
                cross_dialect: false,
            },
        )
    };

    if let Some(error) = target_dependency_catalog_error.as_deref() {
        for requirement in &mut plan.requirements {
            if let PlanRequirement::Unsupported { operation, reason } = requirement {
                if operation == "target-only-table-drop-order" {
                    *reason = format!(
                        "Could not inspect the target foreign-key dependency catalog ({error}). Confirm catalog access, select dependent tables, or remove their foreign keys before dropping a parent."
                    );
                }
            }
        }
        plan.statements.clear();
    }

    let has_complete_target_dependency_catalog =
        allow_destructive && !target_only_tables.is_empty() && target_dependency_catalog.is_some();
    let frozen_target_snapshots = target_dependency_catalog
        .clone()
        .unwrap_or_else(|| selected_target_snapshots.clone());

    if has_complete_target_dependency_catalog {
        super::reviewed::freeze_with_dependency_catalog(
            repository,
            scope,
            &mut plan,
            target_session_id.to_string(),
            tgt_handle,
            tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
            target_dependency_schema_scope.map(str::to_owned),
        )
        .await
        .map_err(SchemaDiffApiError::Validation)?;
    } else {
        super::reviewed::freeze(
            repository,
            scope,
            &mut plan,
            target_session_id.to_string(),
            tgt_handle,
            tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
        )
        .await
        .map_err(SchemaDiffApiError::Validation)?;
    }

    tracing::info!(
        statements = plan.statements.len(),
        warnings = plan.warnings.len(),
        "prepare_schema_diff_plan OK"
    );
    Ok(plan)
}

// ---------------------------------------------------------------- 用例：统一计划

/// 统一（表 + 对象）计划准备请求。
pub struct PrepareUnifiedPlanRequest<'a> {
    pub source: &'a SchemaDiffEndpoint,
    pub target: &'a SchemaDiffEndpoint,
    pub source_session_id: &'a str,
    pub target_session_id: &'a str,
    pub table_names: Vec<String>,
    pub target_table_names: Vec<String>,
    pub target_only_table_names: Vec<String>,
    pub source_objects: Vec<DatabaseObject>,
    pub target_objects: Vec<DatabaseObject>,
    pub source_schema: Option<&'a str>,
    pub target_schema: Option<&'a str>,
    pub allow_destructive: bool,
    pub include_indexes: Option<bool>,
    pub type_overrides: Option<Vec<ColumnTypeOverride>>,
    pub repository: &'a dyn ReviewedPlanRepository,
    pub scope: &'a OrganizationScope,
    pub adapters: &'a dyn SchemaDiffAdapterHost,
}

pub async fn prepare_unified_plan(
    request: PrepareUnifiedPlanRequest<'_>,
) -> Result<SchemaDiffPlan, SchemaDiffApiError> {
    let PrepareUnifiedPlanRequest {
        source,
        target,
        source_session_id,
        target_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        source_objects,
        target_objects,
        source_schema,
        target_schema,
        allow_destructive,
        include_indexes,
        type_overrides,
        repository,
        scope,
        adapters,
    } = request;
    super::plan_request::validate_plan_selection(
        &table_names,
        &target_table_names,
        &target_only_table_names,
        &source_objects,
        &target_objects,
    )
    .map_err(|error| SchemaDiffApiError::Validation(error.to_string()))?;

    let src_config = &source.config;
    let tgt_config = &target.config;
    if normalize_dialect(&src_config.database_type) != normalize_dialect(&tgt_config.database_type)
        && source_objects.is_empty()
        && target_objects.is_empty()
    {
        // 跨方言且未选对象：回退表级计划（类型映射需要适配器对）。
        return prepare_table_plan(PrepareTablePlanRequest {
            source,
            target,
            source_session_id,
            target_session_id,
            table_names,
            target_table_names,
            target_only_table_names,
            allow_destructive,
            include_indexes,
            type_overrides,
            source_schema,
            target_schema,
            repository,
            scope,
            adapters,
        })
        .await;
    }

    if tgt_config.read_only {
        return Err(SchemaDiffApiError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    let source_schema_scope = source_schema.or(src_config.schema.as_deref());
    let target_schema_scope = target_schema.or(tgt_config.schema.as_deref());
    if super::reviewed::same_endpoint(src_config, tgt_config)
        && source_schema_scope.map(str::trim) == target_schema_scope.map(str::trim)
    {
        return Err(SchemaDiffApiError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }

    let source_driver = source.driver.as_ref();
    let source_handle = &source.handle;
    let target_driver = target.driver.as_ref();
    let target_handle = &target.handle;
    // 表结构读取走窄契约，而不是整块驱动。
    let source_catalog_reader = LegacyCatalogReader::new(source_driver);
    let target_catalog_reader = LegacyCatalogReader::new(target_driver);
    let target_dependency_schema_scope = if uses_schema_scope(&tgt_config.database_type) {
        target_schema_scope.or(target_driver.default_schema())
    } else {
        None
    };
    ensure_distinct_schema_scope(
        source_driver,
        source_handle,
        src_config,
        target_driver,
        target_handle,
        tgt_config,
        source_schema_scope,
        target_schema_scope,
    )
    .await?;

    let source_selectors = unique_object_selectors(&source_objects, "source")
        .map_err(|error| SchemaDiffApiError::Validation(error.to_string()))?;
    let target_selectors = unique_object_selectors(&target_objects, "target")
        .map_err(|error| SchemaDiffApiError::Validation(error.to_string()))?;
    let source_catalog =
        read_selected_object_catalog(source_driver, source_handle, &source_selectors).await?;
    let mut target_catalog_complete = true;
    let (target_object_list, target_object_catalog) = read_target_object_catalog(
        target_driver,
        target_handle,
        schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
        &tgt_config.database_type,
        target_dependency_schema_scope,
        &mut target_catalog_complete,
    )
    .await;
    let selected_source = resolve_object_selectors(&source_selectors, &source_catalog, "source")
        .map_err(|error| SchemaDiffApiError::Validation(error.to_string()))?;
    let selected_target =
        resolve_object_selectors(&target_selectors, &target_object_list, "target")
            .map_err(|error| SchemaDiffApiError::Validation(error.to_string()))?;
    let target_mysql_view_scope_context = if normalize_dialect(&src_config.database_type) == "mysql"
        && normalize_dialect(&tgt_config.database_type) == "mysql"
        && (selected_source
            .iter()
            .any(|object| ObjectKind::parse(&object.kind) == Some(ObjectKind::View))
            || selected_target
                .iter()
                .any(|object| ObjectKind::parse(&object.kind) == Some(ObjectKind::View)))
    {
        read_mysql_view_scope_context(target_driver, target_handle).await
    } else {
        None
    };

    let mut source_snapshots = Vec::with_capacity(selected_source.len());
    for object in &selected_source {
        source_snapshots.push(fetch_object_snapshot(source_driver, source_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(selected_target.len());
    for object in &selected_target {
        target_snapshots.push(fetch_object_snapshot(target_driver, target_handle, object).await?);
    }

    let mut table_pairs = Vec::new();
    let mut source_table_snapshots = Vec::new();
    let mut source_table_dependency_catalog = Vec::new();
    let mut planner_source_table_dependencies = Vec::new();
    let mut source_to_target_table = HashMap::new();
    for (source_table, target_table) in table_names.iter().zip(&target_table_names) {
        let source_table =
            resolve_profile_table(&src_config.database_type, source_table, source_schema);
        let target_table =
            resolve_profile_table(&tgt_config.database_type, target_table, target_schema);
        let source_schema_snapshot = source_catalog_reader
            .get_table_schema(
                source_handle,
                &source_table,
                schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
                source_schema_scope,
            )
            .await?;
        source_table_snapshots.push((source_table.clone(), source_schema_snapshot.clone()));
        let source_table_identity = table_identity_for_target(
            &source_table,
            &src_config.database_type,
            source_schema_scope,
            Some(schema_catalog_database(
                &src_config.database_type,
                src_config.database.as_deref(),
            )),
        );
        let target_table_identity = table_identity_for_target(
            &target_table,
            &tgt_config.database_type,
            target_dependency_schema_scope,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
        );
        source_to_target_table.insert(source_table_identity.clone(), target_table_identity.clone());
        let source_table_object = DatabaseObject {
            kind: ObjectKind::Table.as_str().into(),
            schema: source_table_identity.schema.clone(),
            name: source_table_identity.name.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        let source_dependency_snapshot =
            fetch_object_dependency_snapshot(source_driver, source_handle, &source_table_object)
                .await;
        let source_dependencies = source_dependency_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.dependencies.clone());
        let sequence_dependency_usages = source_dependency_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.sequence_dependency_usages.clone());
        let type_dependency_usages =
            source_dependency_snapshot.and_then(|snapshot| snapshot.type_dependency_usages);
        source_table_dependency_catalog.push(SchemaObjectDependencySnapshot {
            identity: source_table_identity.clone(),
            dependencies: source_dependencies.clone(),
            type_dependency_usages: type_dependency_usages.clone(),
            sequence_dependency_usages: sequence_dependency_usages.clone(),
        });
        planner_source_table_dependencies.push(SchemaObjectDependencySnapshot {
            identity: target_table_identity,
            dependencies: source_dependencies,
            type_dependency_usages,
            sequence_dependency_usages,
        });
        let target_schema_snapshot = fetch_target_table_schema(
            target_driver,
            target_handle,
            &target_table,
            tgt_config.database.as_deref().unwrap_or_default(),
            target_schema_scope,
        )
        .await?;
        table_pairs.push((target_table, source_schema_snapshot, target_schema_snapshot));
    }

    for snapshot in &mut planner_source_table_dependencies {
        if let Some(dependencies) = snapshot.dependencies.as_mut() {
            for dependency in dependencies {
                if let Some(target_identity) = source_to_target_table.get(dependency) {
                    *dependency = target_identity.clone();
                }
            }
        }
        if let Some(usages) = snapshot.sequence_dependency_usages.as_mut() {
            for usage in usages {
                if let Some(target_identity) = source_to_target_table.get(&usage.owner_table) {
                    usage.owner_table = target_identity.clone();
                }
            }
        }
    }

    let mut planner_source_snapshots = source_snapshots.clone();
    for snapshot in &mut planner_source_snapshots {
        let target_object_scope = if uses_schema_scope(&tgt_config.database_type) {
            target_dependency_schema_scope
        } else {
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            ))
        };
        if snapshot.kind == ObjectKind::View && snapshot.schema.as_deref() != target_object_scope {
            // Cross-scope view dependencies must retain their source identities
            // until the target driver's scope mapper returns exact mapped pairs.
            continue;
        }
        if let Some(dependencies) = snapshot.dependencies.as_mut() {
            for dependency in dependencies {
                if let Some(target_identity) = source_to_target_table.get(dependency) {
                    *dependency = target_identity.clone();
                }
            }
        }
        if let Some(usages) = snapshot.sequence_dependency_usages.as_mut() {
            for usage in usages {
                if let Some(target_identity) = source_to_target_table.get(&usage.owner_table) {
                    usage.owner_table = target_identity.clone();
                }
            }
        }
    }

    let mut target_only_tables = Vec::new();
    let mut selected_target_table_snapshots = table_pairs
        .iter()
        .map(|(table, _, schema)| (table.clone(), schema.clone()))
        .collect::<Vec<_>>();
    for table in &target_only_table_names {
        let table = resolve_profile_table(&tgt_config.database_type, table, target_schema);
        let snapshot = target_catalog_reader
            .get_table_schema(
                target_handle,
                &table,
                schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
                target_schema_scope,
            )
            .await?;
        if table_pairs.iter().any(|(paired, _, _)| paired == &table) {
            return Err(SchemaDiffApiError::Validation(format!(
                "Table `{table}` cannot be selected both as a source table and target-only table"
            )));
        }
        target_only_tables.push(table.clone());
        selected_target_table_snapshots.push((table.clone(), snapshot.clone()));
    }

    let has_selected_type_drop = selected_target.iter().any(|target| {
        ObjectKind::parse(&target.kind) == Some(ObjectKind::Type)
            && !selected_source
                .iter()
                .any(|source| object_identity(source) == object_identity(target))
    });
    let needs_full_table_catalog = !target_only_tables.is_empty() || has_selected_type_drop;
    let mut target_dependency_tables = selected_target_table_snapshots.clone();
    let mut target_table_identity_catalog = Vec::new();
    let mut has_complete_target_dependency_catalog = false;
    if needs_full_table_catalog {
        match fetch_target_table_dependency_catalog(
            target_driver,
            target_handle,
            tgt_config.database.as_deref().unwrap_or_default(),
            &tgt_config.database_type,
            target_dependency_schema_scope,
            &selected_target_table_snapshots,
        )
        .await
        {
            Ok(catalog) => {
                target_table_identity_catalog = catalog
                    .iter()
                    .map(|(table, _)| {
                        table_identity_for_target(
                            table,
                            &tgt_config.database_type,
                            target_dependency_schema_scope,
                            Some(schema_catalog_database(
                                &tgt_config.database_type,
                                tgt_config.database.as_deref(),
                            )),
                        )
                    })
                    .collect();
                target_dependency_tables = catalog;
                has_complete_target_dependency_catalog = true;
            }
            Err(error) => {
                target_catalog_complete = false;
                tracing::warn!(%error, "Unified plan could not read complete target table catalog");
            }
        }
    }
    if target_table_identity_catalog.is_empty() {
        match read_target_table_identities(
            target_driver,
            target_handle,
            schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
            &tgt_config.database_type,
            target_dependency_schema_scope,
        )
        .await
        {
            Ok(identities) => target_table_identity_catalog = identities,
            Err(error) => {
                target_catalog_complete = false;
                tracing::warn!(%error, "Unified plan could not read target table identities");
            }
        }
    }

    let mut target_object_snapshots = Vec::with_capacity(target_object_catalog.len());
    for dependency_snapshot in &target_object_catalog {
        target_object_snapshots.push(catalog_entry_snapshot(dependency_snapshot));
    }
    for (object, snapshot) in selected_target.iter().zip(target_snapshots.iter()) {
        let identity = object_identity(object);
        if let Some(entry) = target_object_snapshots
            .iter_mut()
            .find(|entry| entry.identity() == identity)
        {
            entry.definition = snapshot.definition.clone();
            entry.dependencies = snapshot.dependencies.clone();
            entry.sequence_dependency_usages = snapshot.sequence_dependency_usages.clone();
        }
    }

    let mut target_identity_set = target_table_identity_catalog
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    target_identity_set.extend(
        target_object_catalog
            .iter()
            .map(|snapshot| snapshot.identity.clone()),
    );

    let migration = datazen_driver_api::LegacySchemaMigration::new(target_driver);
    let Some(renderer) = migration.migration_renderer() else {
        return Err(SchemaDiffApiError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = migration.migration_capabilities() else {
        return Err(SchemaDiffApiError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let source_object_scope = if uses_schema_scope(&src_config.database_type) {
        source_schema_scope.or(source_driver.default_schema())
    } else {
        Some(schema_catalog_database(
            &src_config.database_type,
            src_config.database.as_deref(),
        ))
    };
    let mut plan = build_unified_schema_diff_plan_with_source_scope(
        &table_pairs,
        &target_only_tables,
        &planner_source_snapshots,
        &target_snapshots,
        &target_object_snapshots,
        &planner_source_table_dependencies,
        &target_identity_set.into_iter().collect::<Vec<_>>(),
        target_catalog_complete,
        &target_dependency_tables,
        &src_config.database_type,
        &tgt_config.database_type,
        target_dependency_schema_scope,
        Some(schema_catalog_database(
            &tgt_config.database_type,
            tgt_config.database.as_deref(),
        )),
        allow_destructive,
        include_indexes.unwrap_or(true),
        type_overrides.as_deref().unwrap_or_default(),
        renderer.as_ref(),
        capabilities.as_ref(),
        source_object_scope,
        target_mysql_view_scope_context.as_ref(),
    );
    super::reviewed::freeze_with_unified_catalog(
        repository,
        scope,
        &mut plan,
        target_session_id.to_string(),
        target_handle,
        tgt_config,
        target_dependency_tables,
        target_snapshots,
        target_object_catalog,
        target_table_identity_catalog,
        has_complete_target_dependency_catalog,
        target_catalog_complete,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        target_schema_scope.map(str::to_owned),
        target_dependency_schema_scope.map(str::to_owned),
        ReviewedSourceSnapshot {
            session: source_session_id.to_string(),
            pool: source_handle.pool_id.clone(),
            identity: super::reviewed::identity(src_config),
            database_scope: src_config.database.clone(),
            schema_scope: source_schema_scope.map(str::to_owned),
            table_snapshots: source_table_snapshots,
            object_snapshots: source_snapshots,
            table_dependency_catalog: source_table_dependency_catalog,
        },
        target_mysql_view_scope_context,
    )
    .await
    .map_err(SchemaDiffApiError::Validation)?;
    Ok(plan)
}

// ---------------------------------------------------------------- 用例：复核消费与部署前验证

/// 消费复核计划并完成部署前目标侧验证。
///
/// 顺序与桌面原内联实现一致：`reviewed::consume` → 表 DROP 的完整目录
/// 复核（或逐表快照复核）→ 对象快照复核 → 对象目录/表身份复核 → MySQL
/// 视图创建上下文复核。源侧复核在 [`revalidate_source_snapshot`]，由宿主
/// 在本函数之后调用（与原顺序一致）。
pub async fn consume_and_verify_reviewed_plan(
    repository: &dyn ReviewedPlanRepository,
    scope: &OrganizationScope,
    plan: &SchemaDiffPlan,
    target: &SchemaDiffEndpoint,
    target_session_id: &str,
    effective_target_database: Option<&str>,
    effective_target_schema: Option<&str>,
) -> Result<ReviewedPlan, SchemaDiffApiError> {
    let driver = target.driver.as_ref();
    let handle = &target.handle;
    let config = &target.config;
    let reviewed = super::reviewed::consume(
        repository,
        scope,
        plan,
        target_session_id,
        handle,
        config,
        effective_target_database,
        effective_target_schema,
    )
    .await
    .map_err(SchemaDiffApiError::Validation)?;

    if has_table_drop_statement(&reviewed.plan) {
        if !reviewed.has_complete_target_dependency_catalog {
            return Err(SchemaDiffApiError::Validation(
                "Cannot verify the complete target dependency catalog for this table drop; compare again with full catalog visibility".into(),
            ));
        }
        let current_catalog = fetch_target_table_dependency_catalog(
            driver,
            handle,
            effective_target_database.unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &[],
        )
        .await?;
        validate_target_dependency_catalog(&reviewed.snapshots, &current_catalog)
            .map_err(SchemaDiffApiError::Validation)?;
    } else {
        for (table, snapshot) in &reviewed.snapshots {
            let (relation, snapshot_database, snapshot_schema) = resolve_reviewed_table_snapshot(
                &config.database_type,
                table,
                effective_target_database.unwrap_or_default(),
                effective_target_schema,
            );
            let current = fetch_target_table_schema(
                driver,
                handle,
                &relation,
                &snapshot_database,
                snapshot_schema.as_deref(),
            )
            .await?;
            super::reviewed::validate_snapshot(table, snapshot, &current)
                .map_err(SchemaDiffApiError::Validation)?;
        }
    }
    for snapshot in &reviewed.object_snapshots {
        let object = DatabaseObject {
            kind: snapshot.kind.as_str().into(),
            schema: snapshot.schema.clone(),
            name: snapshot.name.clone(),
            signature: snapshot.signature.clone(),
            target_schema: snapshot.target_schema.clone(),
            target_name: snapshot.target_name.clone(),
        };
        let current = fetch_schema_object(driver, handle, &object).await?;
        super::reviewed::validate_object_snapshot(snapshot, &current)
            .map_err(SchemaDiffApiError::Validation)?;
    }
    if reviewed.has_complete_target_object_catalog {
        let mut current_catalog_complete = true;
        let (_, current_object_catalog) = read_target_object_catalog(
            driver,
            handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &mut current_catalog_complete,
        )
        .await;
        if !current_catalog_complete {
            return Err(SchemaDiffApiError::Validation(
                "Target object dependency catalog can no longer be proved complete; compare again"
                    .into(),
            ));
        }
        super::reviewed::validate_object_dependency_catalog(
            &reviewed.object_dependency_catalog,
            &current_object_catalog,
        )
        .map_err(SchemaDiffApiError::Validation)?;
        let current_table_catalog = read_target_table_identities(
            driver,
            handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
        )
        .await?;
        super::reviewed::validate_table_identity_catalog(
            &reviewed.target_table_identity_catalog,
            &current_table_catalog,
        )
        .map_err(SchemaDiffApiError::Validation)?;
    }
    if let Some(expected_context) = reviewed.target_mysql_view_scope_context.as_ref() {
        let current_context = read_mysql_view_scope_context(driver, handle).await;
        if current_context.as_ref() != Some(expected_context) {
            return Err(SchemaDiffApiError::Validation(
                "Target MySQL view creation context changed after review; compare again".into(),
            ));
        }
    }
    Ok(reviewed)
}

/// 部署前对源侧快照的再次验证（源端点由宿主按
/// `reviewed.source_snapshot.session` 解析后传入）。
pub async fn revalidate_source_snapshot(
    source: &SchemaDiffEndpoint,
    reviewed: &ReviewedPlan,
) -> Result<(), SchemaDiffApiError> {
    let Some(snapshot) = reviewed.source_snapshot.as_ref() else {
        return Ok(());
    };
    let driver = source.driver.as_ref();
    let handle = &source.handle;
    let config = &source.config;
    super::reviewed::validate_source_connection_snapshot(
        snapshot,
        &snapshot.session,
        &handle.pool_id,
        config,
    )
    .map_err(SchemaDiffApiError::Validation)?;

    for (table, table_snapshot) in &snapshot.table_snapshots {
        let (relation, database, schema) = resolve_reviewed_table_snapshot(
            &config.database_type,
            table,
            config.database.as_deref().unwrap_or_default(),
            snapshot.schema_scope.as_deref(),
        );
        let current =
            fetch_target_table_schema(driver, handle, &relation, &database, schema.as_deref())
                .await?;
        super::reviewed::validate_snapshot(table, table_snapshot, &current)
            .map_err(SchemaDiffApiError::Validation)?;
    }

    for object_snapshot in &snapshot.object_snapshots {
        let object = DatabaseObject {
            kind: object_snapshot.kind.as_str().into(),
            schema: object_snapshot.schema.clone(),
            name: object_snapshot.name.clone(),
            signature: object_snapshot.signature.clone(),
            target_schema: object_snapshot.target_schema.clone(),
            target_name: object_snapshot.target_name.clone(),
        };
        let current = fetch_object_snapshot(driver, handle, &object).await?;
        super::reviewed::validate_object_snapshot(object_snapshot, &current)
            .map_err(SchemaDiffApiError::Validation)?;
        super::reviewed::validate_object_dependency_catalog(
            &[SchemaObjectDependencySnapshot {
                identity: object_snapshot.identity(),
                dependencies: object_snapshot.dependencies.clone(),
                type_dependency_usages: None,
                sequence_dependency_usages: object_snapshot.sequence_dependency_usages.clone(),
            }],
            &[SchemaObjectDependencySnapshot {
                identity: current.identity(),
                dependencies: current.dependencies,
                type_dependency_usages: None,
                sequence_dependency_usages: current.sequence_dependency_usages,
            }],
        )
        .map_err(SchemaDiffApiError::Validation)?;
    }

    let mut current_table_dependencies =
        Vec::with_capacity(snapshot.table_dependency_catalog.len());
    for catalog_snapshot in &snapshot.table_dependency_catalog {
        let object = DatabaseObject {
            kind: catalog_snapshot.identity.kind.as_str().into(),
            schema: catalog_snapshot.identity.schema.clone(),
            name: catalog_snapshot.identity.name.clone(),
            signature: catalog_snapshot.identity.signature.clone(),
            target_schema: catalog_snapshot.identity.target_schema.clone(),
            target_name: catalog_snapshot.identity.target_name.clone(),
        };
        let mut current = fetch_object_dependency_snapshot(driver, handle, &object).await;
        if catalog_snapshot.dependencies.is_some() && current.is_none() {
            return Err(SchemaDiffApiError::Validation(format!(
                "Source dependency visibility for `{}` is no longer complete; compare again",
                catalog_snapshot.identity.display_key()
            )));
        }
        if let Some(current) = current.as_mut() {
            current.identity = catalog_snapshot.identity.clone();
        }
        current_table_dependencies.push(current.unwrap_or_else(|| {
            SchemaObjectDependencySnapshot {
                identity: catalog_snapshot.identity.clone(),
                dependencies: None,
                type_dependency_usages: None,
                sequence_dependency_usages: None,
            }
        }));
    }
    super::reviewed::validate_object_dependency_catalog(
        &snapshot.table_dependency_catalog,
        &current_table_dependencies,
    )
    .map_err(SchemaDiffApiError::Validation)?;
    Ok(())
}

// ---------------------------------------------------------------- 用例：单表对比

/// 单表对比结果：与桌面此前手工拼装的 JSON 逐键一致（camelCase，DDL
/// 可选键缺席时不出现）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchemaComparison {
    pub table: String,
    pub missing_on_target: Vec<super::types::ColumnSnapshot>,
    pub extra_on_target: Vec<super::types::ColumnSnapshot>,
    pub added: Vec<super::types::ColumnSnapshot>,
    pub removed: Vec<super::types::ColumnSnapshot>,
    pub changed: Vec<super::types::ChangedColumnDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ddl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_ddl: Option<String>,
}

/// Compare column-level schema differences for a single table.
pub async fn compare_table_schemas(
    source: &SchemaDiffEndpoint,
    target: &SchemaDiffEndpoint,
    source_table_name: &str,
    target_table_name: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    adapters: &dyn SchemaDiffAdapterHost,
    preview: &dyn ComparisonPreviewHost,
) -> Result<TableSchemaComparison, SchemaDiffApiError> {
    let src_config = &source.config;
    let tgt_config = &target.config;
    let src_driver = source.driver.as_ref();
    let src_handle = &source.handle;
    let tgt_driver = target.driver.as_ref();
    let tgt_handle = &target.handle;

    let src_table = resolve_table_for_dialect(&src_config.database_type, source_table_name);
    let tgt_table = resolve_table_for_dialect(&tgt_config.database_type, target_table_name);

    // 表结构读取走窄契约，而不是整块驱动。
    let src_catalog = LegacyCatalogReader::new(src_driver);
    let src_schema_snapshot = src_catalog
        .get_table_schema(
            src_handle,
            &src_table,
            schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
            source_schema.or(src_config.schema.as_deref()),
        )
        .await?;
    let tgt_schema_snapshot = fetch_target_table_schema(
        tgt_driver,
        tgt_handle,
        &tgt_table,
        tgt_config.database.as_deref().unwrap_or_default(),
        target_schema.or(tgt_config.schema.as_deref()),
    )
    .await?;

    // Source = desired: missingOnTarget → ADD, extraOnTarget → DROP.
    // `added`/`removed` kept as aliases for one release.
    let mut source_ddl: Option<String> = None;
    let mut target_ddl: Option<String> = None;
    let mut ir_diff: Option<TableColumnDiff> = None;

    if let Some(comparison) =
        adapters.comparison_adapters(&src_config.database_type, &tgt_config.database_type)
    {
        let src_full_types = preview
            .full_column_types(
                comparison.source_of_source.as_ref(),
                src_driver,
                src_handle,
                &src_table,
            )
            .await;
        let tgt_full_types = if tgt_schema_snapshot.columns.is_empty() {
            None
        } else {
            preview
                .full_column_types(
                    comparison.source_of_target.as_ref(),
                    tgt_driver,
                    tgt_handle,
                    &tgt_table,
                )
                .await
        };

        let src_ir = comparison
            .source_of_source
            .table_to_ir(&src_schema_snapshot, src_full_types.as_ref());
        let tgt_ir = comparison
            .source_of_target
            .table_to_ir(&tgt_schema_snapshot, tgt_full_types.as_ref());
        ir_diff = Some(diff_table_schemas_ir(source_table_name, &src_ir, &tgt_ir));
        source_ddl = Some(preview.create_table_ddl(&src_ir, comparison.target_of_source.as_ref()));
        target_ddl = if tgt_ir.columns.is_empty() {
            None
        } else {
            Some(preview.create_table_ddl(&tgt_ir, comparison.target_of_target.as_ref()))
        };
    }

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let normalizer_holder = if src_d == tgt_d {
        // Type equivalence is a structural-migration rule; read it through the
        // narrow SchemaMigration contract instead of the whole driver.
        datazen_driver_api::create_driver(&tgt_d).and_then(|d| {
            datazen_driver_api::LegacySchemaMigration::new(d.as_ref()).type_normalizer()
        })
    } else {
        None
    };
    let normalizer = normalizer_holder.as_deref();

    let diff = ir_diff.unwrap_or_else(|| {
        diff_table_schemas(
            source_table_name,
            &src_schema_snapshot,
            &tgt_schema_snapshot,
            normalizer,
        )
    });

    tracing::info!(%source_table_name, %target_table_name, "compare_table_schemas OK");
    Ok(TableSchemaComparison {
        table: source_table_name.to_string(),
        missing_on_target: diff.missing_on_target,
        extra_on_target: diff.extra_on_target,
        added: diff.added,
        removed: diff.removed,
        changed: diff.changed,
        source_ddl,
        target_ddl,
    })
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
