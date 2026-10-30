//! Shared request helpers for Data Sync IPC, plus the write-SQL use cases
//! descended from the desktop host (host-boundary F4c-1).
//!
//! `api` 模块的写法沿用边界方案 §3：用例函数只收纯数据与已构造好的
//! 适配器/方言，不依赖 Tauri / Axum / `AppState`；宿主侧只保留租约与
//! 注册表查找（经 [`BatchSqlHost`] 端口）。错误走 `DataSyncError`，经宿主
//! 既有 `From<DataSyncError> for CommandError` 映射表转 IPC，文本逐字节
//! 不变。

use std::collections::HashMap;
use std::sync::Arc;

use datazen_driver_api::{
    DatabaseDriver, IRType, SqlDialect, SyncSourceAdapter, SyncTargetAdapter, TableSchema, Value,
};
use serde::Deserialize;

use super::job::artifact::RelationIdentity;
use super::{
    generate_table_sql_with_qualified_table_and_policy, types_eq::types_equivalent,
    ChangeOperation, ConflictPolicy, DataSyncError, IdentityInsertTarget, LargeValueMode,
    MatchingStrategy, RowChange, SqlStatement, SyncOptions, SyncSourceFilter, TableChangeSet,
    TableMapping, TableResult,
};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptionsInput {
    pub insert: Option<bool>,
    pub update: Option<bool>,
    pub delete: Option<bool>,
    pub matching_strategy: Option<MatchingStrategy>,
    pub batch_size: Option<u32>,
    pub large_value_mode: Option<LargeValueMode>,
    pub conflict_policy: Option<ConflictPolicy>,
}

impl SyncOptionsInput {
    pub fn into_options(self) -> SyncOptions {
        let mut opts = SyncOptions::default();
        if let Some(v) = self.insert {
            opts.insert = v;
        }
        if let Some(v) = self.update {
            opts.update = v;
        }
        if let Some(v) = self.delete {
            opts.delete = v;
        }
        if let Some(v) = self.matching_strategy {
            opts.matching_strategy = v;
        }
        if let Some(v) = self.batch_size {
            opts.batch_size = v;
        }
        if let Some(v) = self.large_value_mode {
            opts.large_value_mode = v;
        }
        if let Some(v) = self.conflict_policy {
            opts.conflict_policy = v;
        }
        opts
    }
}

pub fn resolve_options(input: Option<SyncOptionsInput>) -> SyncOptions {
    input.unwrap_or_default().into_options()
}

/// Normalize selected databases for self-sync checks (UI selection wins over config default).
pub fn resolve_db_name(selected: Option<&str>, config_default: Option<&str>) -> String {
    selected
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(config_default.map(str::trim).filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

pub fn is_self_sync(
    source_db_session_id: &str,
    target_db_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> bool {
    if source_db_session_id != target_db_session_id {
        return false;
    }
    if source_database != target_database {
        return false;
    }
    normalize_schema(source_schema) == normalize_schema(target_schema)
}

fn normalize_schema(schema: Option<&str>) -> Option<&str> {
    schema.map(str::trim).filter(|s| !s.is_empty())
}

/// Keep only tables whose `TableInfo.schema` matches (case-sensitive, PostgreSQL).
pub fn filter_tables_by_schema(
    tables: Vec<datazen_driver_api::TableInfo>,
    schema: Option<&str>,
) -> Vec<datazen_driver_api::TableInfo> {
    match normalize_schema(schema) {
        Some(s) => tables
            .into_iter()
            .filter(|t| t.schema.as_deref() == Some(s))
            .collect(),
        None => tables,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct GenerateSqlRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub tables: Vec<TableResult>,
    pub options: SyncOptionsInput,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMappingInput {
    pub source_table: String,
    pub target_table: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl From<TableMappingInput> for TableMapping {
    fn from(value: TableMappingInput) -> Self {
        TableMapping {
            source_table: value.source_table,
            target_table: value.target_table,
            enabled: value.enabled,
            matching_columns: Vec::new(),
            source_filter: None,
        }
    }
}

/// Everything one data sync comparison needs: the two sessions, the scope
/// and the selection.
///
/// The desktop window and the workflow migration step ask for the same
/// comparison, so the shape of the ask belongs to the feature: one contract
/// for the desktop host and a web host alike.
#[derive(Debug, Clone)]
pub struct SyncComparisonRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    /// The source tables to compare.
    pub tables: Vec<String>,
    /// Client-chosen job id; the host mints one when it is empty.
    pub job_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub options: SyncOptions,
    /// The table mapping (source/target pairs) behind the comparison.
    pub mappings: Vec<TableMapping>,
    /// Per-source-table filters.
    pub source_filters: std::collections::HashMap<String, SyncSourceFilter>,
}

// ---------------------------------------------------------------------------
// Write-SQL assembly (host-boundary F4c-1)
// ---------------------------------------------------------------------------

/// Borrowed inputs for one table's write-SQL assembly.
///
/// Both callers — the reviewed-batch apply path and the SQL-preview command —
/// gather these from their own state; the assembly itself never sees a host.
pub struct WriteSqlContext<'a> {
    /// Canonical Data Sync family of the target (`mysql` / `postgresql` / …).
    pub family: &'a str,
    pub database: &'a str,
    pub schema: Option<&'a str>,
    /// The target's live table schema (compare-time cache or live read).
    pub schema_obj: &'a TableSchema,
    /// Source-side adapter: renders preview literal IR types.
    pub preview_source: &'a dyn SyncSourceAdapter,
    /// Target-side adapter: qualifies, quotes and renders literals.
    pub preview_target: &'a dyn SyncTargetAdapter,
    /// Placeholder rules for the live target driver.
    pub dialect: &'a dyn SqlDialect,
    /// Whether the live driver needs a session toggle for IDENTITY_INSERT.
    pub identity_insert_requires_session_toggle: bool,
}

/// Owned per-relation write-SQL inputs, gathered by the host that owns the
/// leases and the adapter registry.
pub struct BatchSqlContext {
    pub family: String,
    pub database: String,
    pub schema: Option<String>,
    pub schema_obj: TableSchema,
    /// Canonical projection cached at compare time (`mapped_table_schemas`).
    pub columns: Vec<String>,
    pub column_types: HashMap<String, String>,
    pub pk_columns: Vec<String>,
    pub preview_source: Arc<dyn SyncSourceAdapter>,
    pub preview_target: Arc<dyn SyncTargetAdapter>,
    /// The live target driver: placeholder syntax and identity-insert toggles
    /// are read from the connection that will run the statements, never from
    /// a cached one.
    pub target_driver: Arc<dyn DatabaseDriver>,
}

/// F4c-1 port: resolve one target relation's cached compare-time metadata,
/// its live adapters and the live target driver.
///
/// 实现方持有租约与注册表（桌面 = `HostDataSync`）；端口不做业务判断，
/// 失败一律 `DataSyncError`。调用顺序上它位于复核门禁**之后**：空批次与
/// 未复核行在触碰任何宿主状态前就被拒绝，与下沉前的宿主实现一致。
pub trait BatchSqlHost: Send + Sync {
    fn write_sql_context(&self, target_table: &str) -> Result<BatchSqlContext, DataSyncError>;
}

/// SQL generation for one reviewed batch.
///
/// The caller hands over the rows it is about to write and receives the
/// statements for the live target dialect. Nothing here re-reads schemas or
/// re-derives a projection: those were cached by `mapped_table_schemas` during
/// the compare Job, and §5.1 forbids executing anything that was not in the
/// reviewed selection.
pub fn generate_reviewed_batch_sql(
    host: &dyn BatchSqlHost,
    relation: &RelationIdentity,
    source_table: &str,
    target_table: &str,
    changes: Vec<RowChange>,
    options: &SyncOptions,
) -> Result<Vec<SqlStatement>, DataSyncError> {
    options.validate()?;
    if relation.table != target_table {
        return Err(DataSyncError::validation(format!(
            "batch relation {} does not match target table {target_table}",
            relation.table
        )));
    }
    // The handler only ever sends reviewed, selected rows. Anything else would
    // silently shrink the batch below the committed range, so it is rejected
    // rather than dropped.
    if let Some(change) = changes
        .iter()
        .find(|change| !change.selected || change.operation == ChangeOperation::Unchanged)
    {
        return Err(DataSyncError::validation(format!(
            "batch for {} carries an unreviewed {} row; compare again",
            target_table,
            match change.operation {
                ChangeOperation::Insert => "insert",
                ChangeOperation::Update => "update",
                ChangeOperation::Delete => "delete",
                ChangeOperation::Unchanged => "unchanged",
            }
        )));
    }
    if changes.is_empty() {
        return Ok(Vec::new());
    }
    let ctx = host.write_sql_context(target_table)?;
    let dialect = datazen_driver_api::LegacySqlDialect::new(ctx.target_driver.as_ref());
    let borrowed = WriteSqlContext {
        family: &ctx.family,
        database: &ctx.database,
        schema: ctx.schema.as_deref(),
        schema_obj: &ctx.schema_obj,
        preview_source: ctx.preview_source.as_ref(),
        preview_target: ctx.preview_target.as_ref(),
        dialect: &dialect,
        identity_insert_requires_session_toggle: ctx
            .target_driver
            .explicit_identity_insert_requires_session_toggle(),
    };
    // The canonical projection cached at compare time, in column order.
    let cached_types = ctx
        .columns
        .iter()
        .map(|column| ctx.column_types.get(column).cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let change_set = TableChangeSet {
        source_table: source_table.to_string(),
        target_table: target_table.to_string(),
        changes,
    };
    generate_write_sql_for_table(
        &borrowed,
        &change_set,
        &ctx.columns,
        &cached_types,
        &ctx.pk_columns,
        options.conflict_policy,
    )
}

/// Assemble the DML for one table: sqlserver preflight, projection
/// re-validation, preview literals, qualification, identity-insert stamping.
///
/// The reviewed-batch path and the SQL-preview command share this core; they
/// differ only in where the schema and projection came from, never in how the
/// statements are built.
pub fn generate_write_sql_for_table(
    ctx: &WriteSqlContext<'_>,
    change_set: &TableChangeSet,
    columns: &[String],
    cached_types: &[String],
    primary_keys: &[String],
    conflict_policy: ConflictPolicy,
) -> Result<Vec<SqlStatement>, DataSyncError> {
    if ctx.family == "sqlserver" {
        sqlserver_write_preflight(
            ctx.schema_obj,
            change_set,
            conflict_policy,
            ctx.preview_target.supports_explicit_identity_values(),
        )?;
        if ctx.preview_target.supports_explicit_identity_values() {
            validate_projected_identity_values(ctx.schema_obj, columns, change_set)?;
        }
    }
    let column_types = resolve_projection_types(
        columns,
        cached_types,
        primary_keys,
        ctx.schema_obj,
        ctx.family,
    )?;
    let preview_types = ctx
        .schema_obj
        .columns
        .iter()
        .map(|column| {
            let ir = ctx
                .preview_source
                .column_to_ir(column, Some(&column.data_type));
            (column.name.clone(), ir.ir_type)
        })
        .collect::<HashMap<String, IRType>>();
    let preview_literal = |column: &str, value: &Option<Value>, _data_type: Option<&str>| {
        let ir_type = preview_types.get(column).ok_or_else(|| {
            DataSyncError::validation(format!(
                "target preview metadata for column '{column}' is missing; compare again"
            ))
        })?;
        Ok(ctx.preview_target.format_literal(value, ir_type))
    };
    let qualified_table =
        ctx.preview_target
            .qualify_relation(ctx.database, ctx.schema, &change_set.target_table);
    let statements = generate_table_sql_with_qualified_table_and_policy(
        change_set,
        &qualified_table,
        primary_keys,
        columns,
        &column_types,
        |name| ctx.preview_target.quote_ident(name),
        |index, data_type| {
            ctx.dialect
                .parameter_placeholder(index, data_type)
                .map_err(|error| DataSyncError::validation(error.to_string()))
        },
        conflict_policy,
        preview_literal,
    )?;
    let has_insert = change_set
        .changes
        .iter()
        .any(|change| change.operation == ChangeOperation::Insert);
    let identity_insert_target = if has_insert {
        identity_insert_target_for_projection(
            ctx.family,
            ctx.schema_obj,
            columns,
            ctx.database,
            ctx.schema,
            &change_set.target_table,
            ctx.preview_target.supports_explicit_identity_values(),
            ctx.identity_insert_requires_session_toggle,
        )
    } else {
        None
    };
    Ok(statements
        .into_iter()
        .map(|mut statement| {
            if statement.operation == ChangeOperation::Insert {
                statement.identity_insert = identity_insert_target.clone();
            }
            statement
        })
        .collect())
}

/// Re-validate the compare-time projection against the target schema; what
/// turns a stale plan into a "compare again" rejection instead of a wrong
/// write.
fn resolve_projection_types(
    columns: &[String],
    cached_types: &[String],
    primary_keys: &[String],
    schema: &TableSchema,
    family: &str,
) -> Result<Vec<String>, DataSyncError> {
    let unique: std::collections::HashSet<_> = columns.iter().collect();
    if columns.is_empty()
        || unique.len() != columns.len()
        || columns.len() != schema.columns.len()
        || cached_types.len() != columns.len()
        || primary_keys != schema.effective_primary_keys()
    {
        return Err(DataSyncError::validation(
            "comparison projection is stale or missing; compare again",
        ));
    }
    columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let column = schema
                .columns
                .iter()
                .find(|c| &c.name == name)
                .ok_or_else(|| {
                    DataSyncError::validation(format!(
                        "target column {name} changed; compare again"
                    ))
                })?;
            if !types_equivalent(family, &cached_types[index], &column.data_type) {
                return Err(DataSyncError::validation(format!(
                    "target type for {name} changed; compare again"
                )));
            }
            Ok(column.data_type.clone())
        })
        .collect()
}

fn sqlserver_write_preflight(
    schema: &TableSchema,
    changes: &TableChangeSet,
    conflict_policy: ConflictPolicy,
    target_supports_explicit_identity: bool,
) -> Result<(), DataSyncError> {
    let has_insert = changes
        .changes
        .iter()
        .any(|change| change.operation == ChangeOperation::Insert);
    if has_insert && !target_supports_explicit_identity {
        if let Some(column) = schema
            .columns
            .iter()
            .find(|column| column.is_auto_increment)
        {
            return Err(DataSyncError::validation(format!(
                "SQL Server Data Sync cannot insert selected rows into identity column '{}' because scoped IDENTITY_INSERT support is unavailable",
                column.name
            )));
        }
    }

    let needs_optimistic_condition = conflict_policy != ConflictPolicy::Force
        && changes.changes.iter().any(|change| {
            matches!(
                change.operation,
                ChangeOperation::Update | ChangeOperation::Delete
            )
        });
    if needs_optimistic_condition {
        let primary_keys = schema.effective_primary_keys();
        for column in &schema.columns {
            if primary_keys.iter().any(|key| key == &column.name) {
                continue;
            }
            let base_type = column
                .data_type
                .trim()
                .split(['(', ' ', ','])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                base_type.as_str(),
                "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext"
            ) {
                return Err(DataSyncError::validation(format!(
                    "SQL Server optimistic write conditions cannot verify column '{}' exactly because collation and trailing-space comparison semantics are unavailable; choose an explicit conflict policy that does not compare the prior row or use a table without this column type",
                    column.name
                )));
            }
            if matches!(
                base_type.as_str(),
                "xml" | "image" | "geography" | "geometry" | "hierarchyid" | "sql_variant"
            ) {
                return Err(DataSyncError::validation(format!(
                    "SQL Server optimistic write conditions do not support equality for column '{}' of type '{}'; refusing to generate UPDATE/DELETE SQL",
                    column.name, column.data_type
                )));
            }
        }
    }
    Ok(())
}

fn identity_insert_target_for_projection(
    family: &str,
    schema: &TableSchema,
    projection_columns: &[String],
    database: &str,
    target_schema: Option<&str>,
    table: &str,
    supports_explicit_values: bool,
    requires_session_toggle: bool,
) -> Option<IdentityInsertTarget> {
    if !family.eq_ignore_ascii_case("sqlserver")
        || !supports_explicit_values
        || !requires_session_toggle
    {
        return None;
    }
    let has_projected_identity = schema.columns.iter().any(|column| {
        column.is_auto_increment && projection_columns.iter().any(|name| name == &column.name)
    });
    has_projected_identity.then(|| IdentityInsertTarget {
        database: database.to_string(),
        schema: target_schema.map(str::to_string),
        table: table.to_string(),
    })
}

fn validate_projected_identity_values(
    schema: &TableSchema,
    projection_columns: &[String],
    changes: &TableChangeSet,
) -> Result<(), DataSyncError> {
    let identities = schema
        .columns
        .iter()
        .filter(|column| {
            column.is_auto_increment && projection_columns.iter().any(|name| name == &column.name)
        })
        .filter_map(|column| {
            projection_columns
                .iter()
                .position(|name| name == &column.name)
                .map(|index| (column.name.as_str(), index))
        })
        .collect::<Vec<_>>();
    for (column, index) in identities {
        for change in changes
            .changes
            .iter()
            .filter(|change| change.operation == ChangeOperation::Insert)
        {
            let value = change
                .source_row
                .as_ref()
                .and_then(|row| row.get(index))
                .and_then(Option::as_ref);
            if value.is_none() || matches!(value, Some(Value::Null)) {
                return Err(DataSyncError::validation(format!(
                    "SQL Server Data Sync cannot safely insert selected rows because identity column '{column}' has a missing or NULL source value"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod write_sql_tests {
    use super::*;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::ColumnSchema;

    fn sqlserver_schema(identity: bool) -> TableSchema {
        TableSchema {
            table_name: "target".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: identity,
                },
                ColumnSchema {
                    name: "label".into(),
                    data_type: "nvarchar(64)".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    fn insert_changes(id: Option<Value>) -> TableChangeSet {
        TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![id, Some(Value::String("x".into()))]),
                target_row: None,
                changed_columns: Vec::new(),
                selected: true,
            }],
        }
    }

    // ---- 自宿主 apply.rs 原样迁入（错误改走 DataSyncError，文本不变） ----

    #[test]
    fn sqlserver_write_preflight_rejects_identity_inserts_without_session_support() {
        let schema = sqlserver_schema(true);
        let error = sqlserver_write_preflight(
            &schema,
            &insert_changes(Some(Value::Integer(1))),
            ConflictPolicy::Abort,
            false,
        )
        .expect_err("IDENTITY_INSERT must not be silently skipped");
        assert!(error.to_string().contains("IDENTITY_INSERT"), "{error}");
    }

    #[test]
    fn sqlserver_write_preflight_rejects_unverifiable_string_conditions() {
        let schema = sqlserver_schema(false);
        let changes = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Update,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("new".into())),
                ]),
                target_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("old".into())),
                ]),
                changed_columns: vec!["label".into()],
                selected: true,
            }],
        };
        let error = sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Abort, false)
            .expect_err("case/trailing-space changes require exact SQL equality");
        assert!(
            error.to_string().contains("collation and trailing-space"),
            "{error}"
        );
        assert!(sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Force, false).is_ok());
    }

    #[test]
    fn identity_insert_runtime_target_requires_projected_identity_and_session_toggle() {
        let schema = sqlserver_schema(true);
        let projection = vec!["id".to_string(), "label".to_string()];
        let target = identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &projection,
            "target_db",
            Some("dbo"),
            "target",
            true,
            true,
        )
        .expect("projected identity insert must carry its runtime session target");
        assert_eq!(target.database, "target_db");
        assert_eq!(target.schema.as_deref(), Some("dbo"));
        assert_eq!(target.table, "target");

        assert!(identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &["label".into()],
            "target_db",
            Some("dbo"),
            "target",
            true,
            true,
        )
        .is_none());
        assert!(identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &projection,
            "target_db",
            Some("dbo"),
            "target",
            true,
            false,
        )
        .is_none());
    }

    #[test]
    fn explicit_identity_insert_rejects_missing_or_null_source_identity_values() {
        let schema = sqlserver_schema(true);
        let projection = vec!["id".to_string(), "label".to_string()];
        assert!(
            validate_projected_identity_values(&schema, &projection, &insert_changes(None))
                .unwrap_err()
                .to_string()
                .contains("missing or NULL")
        );
        assert!(validate_projected_identity_values(
            &schema,
            &projection,
            &insert_changes(Some(Value::Null))
        )
        .unwrap_err()
        .to_string()
        .contains("missing or NULL"));
        assert!(validate_projected_identity_values(
            &schema,
            &projection,
            &insert_changes(Some(Value::Integer(1)))
        )
        .is_ok());
        assert!(validate_projected_identity_values(
            &schema,
            &["label".into()],
            &insert_changes(None)
        )
        .is_ok());
    }

    // ---- 自宿主 apply.rs 迁入：投影漂移拒绝（改走切片签名） ----

    #[test]
    fn reordered_target_columns_keep_canonical_values_and_reject_drift() {
        use super::super::{generate_table_sql, ChangeSet, ComparisonResult, TableResult};

        let opts = SyncOptions::default();
        let mut result = TableResult::matched(
            "source",
            "target",
            vec![RowChange::update(
                vec![Value::Integer(1)],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(10)),
                    Some(Value::Integer(20)),
                ],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(9)),
                    Some(Value::Integer(20)),
                ],
                vec!["a".into()],
                &opts,
            )],
        );
        result.columns = vec!["id".into(), "a".into(), "b".into()];
        result.column_types = vec!["INT".into(); 3];
        result.primary_keys = vec!["id".into()];
        let mut schema = TableSchema {
            table_name: "target".into(),
            columns: ["id", "b", "a"]
                .iter()
                .map(|name| ColumnSchema {
                    name: (*name).into(),
                    data_type: "INT".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: *name == "id",
                    is_auto_increment: false,
                })
                .collect(),
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let types = resolve_projection_types(
            &result.columns,
            &result.column_types,
            &result.primary_keys,
            &schema,
            "mysql",
        )
        .unwrap();
        let set =
            ChangeSet::from_comparison("t", &ComparisonResult::new(vec![result.clone()]), &opts);
        let statements = generate_table_sql(
            &set.tables[0],
            None,
            &result.primary_keys,
            &result.columns,
            &types,
            |name| format!("`{name}`"),
            |_, _| "?".into(),
        )
        .unwrap();
        assert!(matches!(statements[0].parameters[0], Value::Integer(10)));
        assert_eq!(
            statements[0].sql,
            "UPDATE `target` SET `a` = ? WHERE `id` = ? AND (`a` = ? OR (`a` IS NULL AND ? IS NULL)) AND (`b` = ? OR (`b` IS NULL AND ? IS NULL))"
        );
        schema.primary_keys = vec!["b".into()];
        assert!(resolve_projection_types(
            &result.columns,
            &result.column_types,
            &result.primary_keys,
            &schema,
            "mysql"
        )
        .is_err());
        schema.primary_keys = vec!["id".into()];
        schema.columns[2].data_type = "TEXT".into();
        assert!(resolve_projection_types(
            &result.columns,
            &result.column_types,
            &result.primary_keys,
            &schema,
            "mysql"
        )
        .is_err());
        result.columns.clear();
        assert!(resolve_projection_types(
            &result.columns,
            &result.column_types,
            &result.primary_keys,
            &schema,
            "mysql"
        )
        .is_err());
    }

    // ---- 新增：复核门禁先于端口（下沉后的顺序契约） ----

    /// 一个绝不能被触碰的端口：门禁（复核校验/空批次/关系匹配）必须
    /// 在任何宿主状态查询之前完成拒绝。
    struct UntouchedHost;

    impl BatchSqlHost for UntouchedHost {
        fn write_sql_context(&self, _target_table: &str) -> Result<BatchSqlContext, DataSyncError> {
            panic!("the reviewed-batch gates must reject before any host lookup");
        }
    }

    fn relation_for(table: &str) -> RelationIdentity {
        RelationIdentity {
            database: "app".into(),
            schema: None,
            table: table.into(),
        }
    }

    fn reviewed_insert(id: Option<Value>, selected: bool) -> RowChange {
        RowChange {
            operation: ChangeOperation::Insert,
            key: vec![Value::Integer(1)],
            source_row: Some(vec![id, Some(Value::String("x".into()))]),
            target_row: None,
            changed_columns: Vec::new(),
            selected,
        }
    }

    #[test]
    fn an_unreviewed_row_is_rejected_before_any_host_lookup() {
        let error = generate_reviewed_batch_sql(
            &UntouchedHost,
            &relation_for("target"),
            "source",
            "target",
            vec![reviewed_insert(Some(Value::Integer(1)), false)],
            &SyncOptions::default(),
        )
        .expect_err("an unreviewed row must never reach statement assembly");
        assert!(
            error
                .to_string()
                .contains("carries an unreviewed insert row"),
            "{error}"
        );
    }

    #[test]
    fn a_batch_relation_mismatch_is_rejected_before_any_host_lookup() {
        let error = generate_reviewed_batch_sql(
            &UntouchedHost,
            &relation_for("other"),
            "source",
            "target",
            vec![reviewed_insert(Some(Value::Integer(1)), true)],
            &SyncOptions::default(),
        )
        .expect_err("the relation identity must match the target table");
        assert!(
            error
                .to_string()
                .contains("does not match target table target"),
            "{error}"
        );
    }

    #[test]
    fn an_empty_batch_returns_no_statements_without_any_host_lookup() {
        let statements = generate_reviewed_batch_sql(
            &UntouchedHost,
            &relation_for("target"),
            "source",
            "target",
            Vec::new(),
            &SyncOptions::default(),
        )
        .expect("an empty reviewed batch is a no-op, not an error");
        assert!(statements.is_empty());
    }

    // ---- 新增：装配 happy path（哑适配器 + mock 驱动） ----

    struct DummySource;

    impl datazen_driver_api::SyncSourceAdapter for DummySource {
        fn column_to_ir(
            &self,
            column: &ColumnSchema,
            _native_full_type: Option<&str>,
        ) -> datazen_driver_api::IRColumn {
            datazen_driver_api::IRColumn {
                name: column.name.clone(),
                ir_type: datazen_driver_api::IRType::Int32,
                nullable: column.nullable,
                default_expr: None,
                is_primary_key: column.is_primary_key,
                is_auto_increment: column.is_auto_increment,
                comment: None,
            }
        }
    }

    struct DummyTarget;

    impl datazen_driver_api::SyncTargetAdapter for DummyTarget {
        fn ir_type_to_native(&self, _ir_type: &datazen_driver_api::IRType) -> String {
            "INT".into()
        }
        fn format_default(&self, _default: &datazen_driver_api::IRDefault) -> Option<String> {
            None
        }
        fn format_literal(
            &self,
            _value: &Option<Value>,
            _ir_type: &datazen_driver_api::IRType,
        ) -> String {
            "NULL".into()
        }
        fn quote_ident(&self, name: &str) -> String {
            format!("\"{name}\"")
        }
        fn qualify_relation(&self, database: &str, schema: Option<&str>, table: &str) -> String {
            match schema {
                Some(schema) => format!("\"{database}\".\"{schema}\".\"{table}\""),
                None => format!("\"{database}\".\"{table}\""),
            }
        }
    }

    fn mock_mysql() -> std::sync::Arc<MockDriver> {
        MockDriver::new(
            "mysql",
            MockDriverOptions {
                parameterized_writes: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn a_reviewed_insert_assembles_a_parameterized_write() {
        let schema = sqlserver_schema(false);
        let driver = mock_mysql();
        let dialect = datazen_driver_api::LegacySqlDialect::new(driver.as_ref());
        let ctx = WriteSqlContext {
            family: "mysql",
            database: "app",
            schema: None,
            schema_obj: &schema,
            preview_source: &DummySource,
            preview_target: &DummyTarget,
            dialect: &dialect,
            identity_insert_requires_session_toggle: false,
        };
        let change_set = insert_changes(Some(Value::Integer(1)));
        let statements = generate_write_sql_for_table(
            &ctx,
            &change_set,
            &["id".to_string(), "label".to_string()],
            &["int".to_string(), "nvarchar(64)".to_string()],
            &["id".to_string()],
            ConflictPolicy::Abort,
        )
        .expect("a matched projection with a primary key must assemble");
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].operation, ChangeOperation::Insert);
        assert!(
            statements[0]
                .sql
                .starts_with("INSERT INTO \"app\".\"target\""),
            "{}",
            statements[0].sql
        );
        assert!(
            statements[0].sql.contains("?"),
            "parameterized writes must bind values, not inline them: {}",
            statements[0].sql
        );
        assert!(
            statements[0].identity_insert.is_none(),
            "non-sqlserver families never carry an identity-insert target"
        );
    }

    #[test]
    fn a_stale_projection_is_refused_as_compare_again() {
        let schema = sqlserver_schema(false);
        let driver = mock_mysql();
        let dialect = datazen_driver_api::LegacySqlDialect::new(driver.as_ref());
        let ctx = WriteSqlContext {
            family: "mysql",
            database: "app",
            schema: None,
            schema_obj: &schema,
            preview_source: &DummySource,
            preview_target: &DummyTarget,
            dialect: &dialect,
            identity_insert_requires_session_toggle: false,
        };
        let change_set = insert_changes(Some(Value::Integer(1)));
        let error = generate_write_sql_for_table(
            &ctx,
            &change_set,
            &["id".to_string(), "label".to_string(), "extra".to_string()],
            &[
                "int".to_string(),
                "nvarchar(64)".to_string(),
                "int".to_string(),
            ],
            &["id".to_string()],
            ConflictPolicy::Abort,
        )
        .expect_err("a projection that no longer matches the schema must be refused");
        assert_eq!(
            error.to_string(),
            "comparison projection is stale or missing; compare again"
        );
    }

    #[test]
    fn sync_options_input_overrides_defaults() {
        let input = SyncOptionsInput {
            insert: Some(false),
            update: Some(true),
            delete: Some(true),
            matching_strategy: None,
            batch_size: Some(50),
            large_value_mode: None,
            conflict_policy: None,
        };
        let opts = resolve_options(Some(input));
        assert!(!opts.insert);
        assert!(opts.update);
        assert!(opts.delete);
        assert_eq!(opts.batch_size, 50);
    }
}
