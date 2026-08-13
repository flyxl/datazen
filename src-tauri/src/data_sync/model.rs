//! Core Data Synchronization types (Navicat-style Diff Sync).
//!
//! Hard product gate: same dialect family + identical structure + identical PK.
//! This module only models the domain; pairing/schema checks live in later slices.

use datazen_driver_api::Value;
use serde::{Deserialize, Serialize};

use super::error::DataSyncError;

pub type Row = Vec<Option<Value>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub connection_id: String,
    pub database: String,
    pub schema: Option<String>,
}

impl Endpoint {
    pub fn new(
        connection_id: impl Into<String>,
        database: impl Into<String>,
        schema: Option<String>,
    ) -> Self {
        Self {
            connection_id: connection_id.into(),
            database: database.into(),
            schema,
        }
    }

    pub fn normalized_schema(&self) -> Option<&str> {
        self.schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Same connection + database + schema (self-sync of one database).
    pub fn same_database_as(&self, other: &Endpoint) -> bool {
        self.connection_id == other.connection_id
            && self.database == other.database
            && self.normalized_schema() == other.normalized_schema()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum MatchingStrategy {
    #[default]
    PrimaryKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LargeValueMode {
    #[default]
    Full,
    Hash,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptions {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub matching_strategy: MatchingStrategy,
    pub batch_size: u32,
    pub large_value_mode: LargeValueMode,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            insert: true,
            update: true,
            delete: false,
            matching_strategy: MatchingStrategy::PrimaryKey,
            batch_size: 1000,
            large_value_mode: LargeValueMode::Full,
        }
    }
}

impl SyncOptions {
    pub fn validate(&self) -> Result<(), DataSyncError> {
        if self.batch_size == 0 {
            return Err(DataSyncError::validation(
                "batchSize must be greater than 0",
            ));
        }
        if !self.insert && !self.update && !self.delete {
            return Err(DataSyncError::validation(
                "at least one of insert/update/delete must be enabled",
            ));
        }
        Ok(())
    }

    pub fn allows(&self, operation: ChangeOperation) -> bool {
        match operation {
            ChangeOperation::Insert => self.insert,
            ChangeOperation::Update => self.update,
            ChangeOperation::Delete => self.delete,
            ChangeOperation::Unchanged => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    pub source_column: String,
    pub target_column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableMapping {
    pub source_table: String,
    pub target_table: String,
    pub enabled: bool,
    #[serde(default)]
    pub matching_columns: Vec<ColumnMapping>,
}

impl TableMapping {
    pub fn auto(source_table: impl Into<String>) -> Self {
        let name = source_table.into();
        Self {
            target_table: name.clone(),
            source_table: name,
            enabled: true,
            matching_columns: Vec::new(),
        }
    }

    pub fn mapped(source_table: impl Into<String>, target_table: impl Into<String>) -> Self {
        Self {
            source_table: source_table.into(),
            target_table: target_table.into(),
            enabled: true,
            matching_columns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TableMappingStatus {
    Matched,
    UnmappedSource,
    UnmappedTarget,
    Disabled,
    Incompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncTask {
    pub id: String,
    pub source: Endpoint,
    pub target: Endpoint,
    pub options: SyncOptions,
    pub mappings: Vec<TableMapping>,
}

impl SyncTask {
    pub fn new(
        id: impl Into<String>,
        source: Endpoint,
        target: Endpoint,
        options: SyncOptions,
        mappings: Vec<TableMapping>,
    ) -> Result<Self, DataSyncError> {
        let task = Self {
            id: id.into(),
            source,
            target,
            options,
            mappings,
        };
        task.validate()?;
        Ok(task)
    }

    pub fn validate(&self) -> Result<(), DataSyncError> {
        if self.id.trim().is_empty() {
            return Err(DataSyncError::validation("sync task id is required"));
        }
        if self.source.connection_id.trim().is_empty()
            || self.target.connection_id.trim().is_empty()
        {
            return Err(DataSyncError::validation(
                "source and target connection ids are required",
            ));
        }
        if self.source.database.trim().is_empty() || self.target.database.trim().is_empty() {
            return Err(DataSyncError::validation(
                "source and target databases are required",
            ));
        }
        if self.source.same_database_as(&self.target) {
            return Err(DataSyncError::validation(
                "self-sync of the same database is not allowed",
            ));
        }
        self.options.validate()?;
        Ok(())
    }

    pub fn enabled_mappings(&self) -> impl Iterator<Item = &TableMapping> {
        self.mappings.iter().filter(|m| m.enabled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChangeOperation {
    Insert,
    Update,
    Delete,
    Unchanged,
}

impl ChangeOperation {
    pub fn default_selected(self, options: &SyncOptions) -> bool {
        match self {
            Self::Insert | Self::Update => options.allows(self),
            Self::Delete => false,
            Self::Unchanged => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    pub operation: ChangeOperation,
    pub key: Vec<Value>,
    pub source_row: Option<Row>,
    pub target_row: Option<Row>,
    pub changed_columns: Vec<String>,
    pub selected: bool,
}

impl RowChange {
    pub fn insert(key: Vec<Value>, source_row: Row, options: &SyncOptions) -> Self {
        let operation = ChangeOperation::Insert;
        Self {
            selected: operation.default_selected(options),
            operation,
            key,
            source_row: Some(source_row),
            target_row: None,
            changed_columns: Vec::new(),
        }
    }

    pub fn update(
        key: Vec<Value>,
        source_row: Row,
        target_row: Row,
        changed_columns: Vec<String>,
        options: &SyncOptions,
    ) -> Self {
        let operation = ChangeOperation::Update;
        Self {
            selected: operation.default_selected(options),
            operation,
            key,
            source_row: Some(source_row),
            target_row: Some(target_row),
            changed_columns,
        }
    }

    pub fn delete(key: Vec<Value>, target_row: Row, options: &SyncOptions) -> Self {
        let operation = ChangeOperation::Delete;
        Self {
            selected: operation.default_selected(options),
            operation,
            key,
            source_row: None,
            target_row: Some(target_row),
            changed_columns: Vec::new(),
        }
    }

    pub fn unchanged(key: Vec<Value>, source_row: Row, target_row: Row) -> Self {
        Self {
            operation: ChangeOperation::Unchanged,
            key,
            source_row: Some(source_row),
            target_row: Some(target_row),
            changed_columns: Vec::new(),
            selected: false,
        }
    }

    pub fn eligible_for_changeset(&self, options: &SyncOptions) -> bool {
        self.selected
            && self.operation != ChangeOperation::Unchanged
            && options.allows(self.operation)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableResult {
    pub source_table: String,
    pub target_table: String,
    pub status: TableMappingStatus,
    pub incompatible_reason: Option<String>,
    pub rows: Vec<RowChange>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl TableResult {
    pub fn matched(
        source_table: impl Into<String>,
        target_table: impl Into<String>,
        rows: Vec<RowChange>,
    ) -> Self {
        Self {
            source_table: source_table.into(),
            target_table: target_table.into(),
            status: TableMappingStatus::Matched,
            incompatible_reason: None,
            rows,
            warnings: Vec::new(),
        }
    }

    pub fn incompatible(
        source_table: impl Into<String>,
        target_table: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            source_table: source_table.into(),
            target_table: target_table.into(),
            status: TableMappingStatus::Incompatible,
            incompatible_reason: Some(reason.into()),
            rows: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn disabled(source_table: impl Into<String>, target_table: impl Into<String>) -> Self {
        Self {
            source_table: source_table.into(),
            target_table: target_table.into(),
            status: TableMappingStatus::Disabled,
            incompatible_reason: None,
            rows: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn unmapped_source(source_table: impl Into<String>) -> Self {
        let name = source_table.into();
        Self {
            source_table: name,
            target_table: String::new(),
            status: TableMappingStatus::UnmappedSource,
            incompatible_reason: None,
            rows: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn unmapped_target(target_table: impl Into<String>) -> Self {
        let name = target_table.into();
        Self {
            source_table: String::new(),
            target_table: name,
            status: TableMappingStatus::UnmappedTarget,
            incompatible_reason: None,
            rows: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn insert_count(&self) -> usize {
        self.count_op(ChangeOperation::Insert)
    }

    pub fn update_count(&self) -> usize {
        self.count_op(ChangeOperation::Update)
    }

    pub fn delete_count(&self) -> usize {
        self.count_op(ChangeOperation::Delete)
    }

    pub fn unchanged_row_count(&self) -> usize {
        self.count_op(ChangeOperation::Unchanged)
    }

    fn count_op(&self, op: ChangeOperation) -> usize {
        self.rows.iter().filter(|r| r.operation == op).count()
    }

    pub fn has_row_differences(&self) -> bool {
        self.status == TableMappingStatus::Matched
            && self
                .rows
                .iter()
                .any(|r| r.operation != ChangeOperation::Unchanged)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonResult {
    pub tables: Vec<TableResult>,
}

impl ComparisonResult {
    pub fn new(tables: Vec<TableResult>) -> Self {
        Self { tables }
    }

    pub fn insert_count(&self) -> usize {
        self.tables.iter().map(TableResult::insert_count).sum()
    }

    pub fn update_count(&self) -> usize {
        self.tables.iter().map(TableResult::update_count).sum()
    }

    pub fn delete_count(&self) -> usize {
        self.tables.iter().map(TableResult::delete_count).sum()
    }

    pub fn unchanged_table_count(&self) -> usize {
        self.tables
            .iter()
            .filter(|t| t.status == TableMappingStatus::Matched && !t.has_row_differences())
            .count()
    }

    pub fn incompatible_count(&self) -> usize {
        self.tables
            .iter()
            .filter(|t| t.status == TableMappingStatus::Incompatible)
            .count()
    }

    pub fn tables_with_differences(&self) -> usize {
        self.tables
            .iter()
            .filter(|t| t.has_row_differences())
            .count()
    }
}

/// Values compare only within the same variant. `0` ≠ `"0"`, `NULL` ≠ `''`.
pub fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        (Value::Timestamp(a), Value::Timestamp(b)) => a == b,
        (Value::Json(a), Value::Json(b)) => a == b,
        _ => false,
    }
}

pub fn optional_values_equal(left: &Option<Value>, right: &Option<Value>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => values_equal(a, b),
        (None, Some(Value::Null)) | (Some(Value::Null), None) => true,
        _ => false,
    }
}

pub fn keys_equal(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| values_equal(a, b))
}

pub fn rows_equal(left: &[Option<Value>], right: &[Option<Value>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| optional_values_equal(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src() -> Endpoint {
        Endpoint::new("conn-a", "db_a", Some("public".into()))
    }

    fn tgt() -> Endpoint {
        Endpoint::new("conn-b", "db_b", Some("public".into()))
    }

    #[test]
    fn default_options_match_prd() {
        let opts = SyncOptions::default();
        assert!(opts.insert);
        assert!(opts.update);
        assert!(!opts.delete);
        assert_eq!(opts.batch_size, 1000);
        assert_eq!(opts.matching_strategy, MatchingStrategy::PrimaryKey);
        assert_eq!(opts.large_value_mode, LargeValueMode::Full);
        opts.validate().unwrap();
    }

    #[test]
    fn options_reject_empty_ops_and_zero_batch() {
        let mut opts = SyncOptions::default();
        opts.batch_size = 0;
        assert!(opts.validate().is_err());
        opts.batch_size = 10;
        opts.insert = false;
        opts.update = false;
        opts.delete = false;
        assert!(opts.validate().is_err());
    }

    #[test]
    fn self_sync_same_database_is_rejected() {
        let err = SyncTask::new(
            "t1",
            src(),
            Endpoint::new("conn-a", "db_a", Some(" public ".into())),
            SyncOptions::default(),
            vec![TableMapping::auto("users")],
        )
        .unwrap_err();
        assert!(err.to_string().contains("self-sync"));
    }

    #[test]
    fn same_connection_different_database_is_allowed() {
        let task = SyncTask::new(
            "t1",
            src(),
            Endpoint::new("conn-a", "db_other", Some("public".into())),
            SyncOptions::default(),
            vec![TableMapping::auto("users")],
        )
        .unwrap();
        assert_eq!(task.enabled_mappings().count(), 1);
    }

    #[test]
    fn empty_schema_normalizes_like_none() {
        let a = Endpoint::new("c", "db", Some("".into()));
        let b = Endpoint::new("c", "db", None);
        assert!(a.same_database_as(&b));
    }

    #[test]
    fn task_requires_ids_and_databases() {
        assert!(SyncTask::new("  ", src(), tgt(), SyncOptions::default(), vec![],).is_err());
        assert!(SyncTask::new(
            "t",
            Endpoint::new("", "db", None),
            tgt(),
            SyncOptions::default(),
            vec![],
        )
        .is_err());
        assert!(SyncTask::new(
            "t",
            Endpoint::new("c", "", None),
            tgt(),
            SyncOptions::default(),
            vec![],
        )
        .is_err());
    }

    #[test]
    fn delete_not_selected_by_default_even_when_enabled() {
        let mut opts = SyncOptions::default();
        opts.delete = true;
        let del = RowChange::delete(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &opts,
        );
        let ins = RowChange::insert(
            vec![Value::Integer(2)],
            vec![Some(Value::Integer(2))],
            &opts,
        );
        let upd = RowChange::update(
            vec![Value::Integer(3)],
            vec![Some(Value::Integer(3))],
            vec![Some(Value::Integer(4))],
            vec!["age".into()],
            &opts,
        );
        let same = RowChange::unchanged(
            vec![Value::Integer(5)],
            vec![Some(Value::Integer(5))],
            vec![Some(Value::Integer(5))],
        );
        assert!(!del.selected);
        assert!(ins.selected);
        assert!(upd.selected);
        assert!(!same.selected);
        assert!(!del.eligible_for_changeset(&opts));
        assert!(ins.eligible_for_changeset(&opts));
        let mut chosen = del.clone();
        chosen.selected = true;
        assert!(chosen.eligible_for_changeset(&opts));
    }

    #[test]
    fn insert_not_eligible_when_option_disabled() {
        let mut opts = SyncOptions::default();
        opts.insert = false;
        opts.delete = true;
        let mut ins = RowChange::insert(
            vec![Value::Integer(1)],
            vec![Some(Value::Integer(1))],
            &opts,
        );
        ins.selected = true;
        assert!(!ins.eligible_for_changeset(&opts));
        assert!(!opts.allows(ChangeOperation::Unchanged));
    }

    #[test]
    fn comparison_summary_separates_tables_and_rows() {
        let opts = SyncOptions::default();
        let users = TableResult::matched(
            "users",
            "users",
            vec![
                RowChange::insert(
                    vec![Value::Integer(1)],
                    vec![Some(Value::Integer(1))],
                    &opts,
                ),
                RowChange::update(
                    vec![Value::Integer(2)],
                    vec![Some(Value::Integer(2))],
                    vec![Some(Value::Integer(9))],
                    vec!["n".into()],
                    &opts,
                ),
                RowChange::unchanged(
                    vec![Value::Integer(3)],
                    vec![Some(Value::Integer(3))],
                    vec![Some(Value::Integer(3))],
                ),
            ],
        );
        let orders = TableResult::matched("orders", "orders", vec![]);
        let skip = TableResult::incompatible("logs", "logs", "missing primary key");
        let result = ComparisonResult::new(vec![
            users,
            orders,
            skip,
            TableResult::disabled("tmp", "tmp"),
            TableResult::unmapped_source("orphan_src"),
            TableResult::unmapped_target("orphan_tgt"),
        ]);
        assert_eq!(result.insert_count(), 1);
        assert_eq!(result.update_count(), 1);
        assert_eq!(result.delete_count(), 0);
        assert_eq!(result.unchanged_table_count(), 1);
        assert_eq!(result.incompatible_count(), 1);
        assert_eq!(result.tables_with_differences(), 1);
        assert_eq!(result.tables[0].unchanged_row_count(), 1);
        assert!(!result.tables[0].warnings.is_empty() || result.tables[0].warnings.is_empty());
    }

    #[test]
    fn values_do_not_cross_types() {
        assert!(values_equal(&Value::Null, &Value::Null));
        assert!(!values_equal(
            &Value::Integer(0),
            &Value::String("0".into())
        ));
        assert!(!values_equal(&Value::Null, &Value::String(String::new())));
        assert!(values_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(values_equal(&Value::Float(1.5), &Value::Float(1.5)));
        assert!(!values_equal(&Value::Float(1.0), &Value::Integer(1)));
        assert!(values_equal(
            &Value::Bytes(vec![1, 2]),
            &Value::Bytes(vec![1, 2])
        ));
        assert!(values_equal(
            &Value::Timestamp("t".into()),
            &Value::Timestamp("t".into())
        ));
        assert!(values_equal(
            &Value::Json(serde_json::json!({"a": 1})),
            &Value::Json(serde_json::json!({"a": 1}))
        ));
        assert!(optional_values_equal(&None, &Some(Value::Null)));
        assert!(!optional_values_equal(&None, &Some(Value::Integer(1))));
        assert!(keys_equal(
            &[Value::Integer(1), Value::String("a".into())],
            &[Value::Integer(1), Value::String("a".into())]
        ));
        assert!(!keys_equal(&[Value::Integer(1)], &[Value::Integer(2)]));
        assert!(rows_equal(
            &[Some(Value::Integer(1)), None],
            &[Some(Value::Integer(1)), Some(Value::Null)]
        ));
    }

    #[test]
    fn serde_roundtrip_task_and_mapping_status() {
        let task = SyncTask::new(
            "task-1",
            src(),
            tgt(),
            SyncOptions::default(),
            vec![TableMapping::mapped("customers", "clients"), {
                let mut m = TableMapping::auto("skip_me");
                m.enabled = false;
                m.matching_columns.push(ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                });
                m
            }],
        )
        .unwrap();
        let json = serde_json::to_string(&task).unwrap();
        let back: SyncTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back, task);
        assert_eq!(
            serde_json::to_string(&TableMappingStatus::UnmappedSource).unwrap(),
            "\"UNMAPPED_SOURCE\""
        );
        assert_eq!(
            serde_json::to_string(&ChangeOperation::Insert).unwrap(),
            "\"INSERT\""
        );
        let hash = LargeValueMode::Hash;
        assert_ne!(hash, LargeValueMode::Full);
    }

    #[test]
    fn row_change_serde_keeps_operation() {
        let opts = SyncOptions::default();
        let row = RowChange::insert(
            vec![Value::Integer(7)],
            vec![Some(Value::String("x".into()))],
            &opts,
        );
        let json = serde_json::to_string(&row).unwrap();
        let back: RowChange = serde_json::from_str(&json).unwrap();
        assert_eq!(back.operation, ChangeOperation::Insert);
        assert_eq!(back.key.len(), 1);
    }
}
