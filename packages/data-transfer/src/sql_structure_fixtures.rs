//! Fake adapters and builders shared by the SQL-file structure-plan tests.
//!
//! The target fakes deliberately inherit every `SyncTargetAdapter` default
//! except the flag a test is exercising, so a change to the shipped defaults
//! stays visible in these tests instead of being frozen into a fake.

use std::collections::HashMap;

use datazen_driver_api::{ColumnSchema, IndexInfo, TableSchema};

use crate::model::{
    ColumnMapping, Endpoint, TableInspectResult, TableMapping, TableMappingStatus, TransferJob,
    TransferMode, TransferOptions, WriteMode,
};
use datazen_driver_api::Value;
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ir::{IRColumn, IRDefault, IRType};

/// Source adapter that only implements the mandatory projection hook: table-level
/// objects keep the catalog shape the tests build.
pub struct Source;

impl SyncSourceAdapter for Source {
    fn column_to_ir(&self, column: &ColumnSchema, _native_full_type: Option<&str>) -> IRColumn {
        IRColumn {
            name: column.name.clone(),
            ir_type: IRType::Int32,
            nullable: column.nullable,
            default_expr: None,
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: None,
        }
    }
}

/// Target adapter with the shipped trait defaults: schema-scoped index/FK names
/// and case-folded object names. SQL Server keeps exactly these defaults.
pub struct Target;

impl Target {
    pub fn default_target() -> Self {
        Target
    }
}

impl SyncTargetAdapter for Target {
    fn ir_type_to_native(&self, _: &IRType) -> String {
        "INTEGER".into()
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::Literal(value) => Some(value.clone()),
            IRDefault::CurrentTimestamp | IRDefault::RawExpression(_) => None,
        }
    }
    fn format_literal(&self, _: &Option<Value>, _: &IRType) -> String {
        "NULL".into()
    }
}

/// Target whose secondary-index names are local to their table (MySQL-shaped).
/// Names still fold case, so this only isolates the table-scoping rule.
pub struct TableScopedIndexTarget;

impl SyncTargetAdapter for TableScopedIndexTarget {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        Target::default_target().ir_type_to_native(ir_type)
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        Target::default_target().format_default(default)
    }
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String {
        Target::default_target().format_literal(value, ir_type)
    }
    fn index_names_are_table_scoped(&self) -> bool {
        true
    }
}

/// Target whose quoted object names keep their case (PostgreSQL-shaped).
pub struct CaseSensitiveTarget;

impl SyncTargetAdapter for CaseSensitiveTarget {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        Target::default_target().ir_type_to_native(ir_type)
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        Target::default_target().format_default(default)
    }
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String {
        Target::default_target().format_literal(value, ir_type)
    }
    fn object_names_are_case_sensitive(&self) -> bool {
        true
    }
}

/// Target that spells auto-increment with the SQL Server `IDENTITY` keyword.
pub struct IdentityTarget;

impl SyncTargetAdapter for IdentityTarget {
    fn ir_type_to_native(&self, _: &IRType) -> String {
        "INTEGER".into()
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::Literal(value) => Some(value.clone()),
            IRDefault::CurrentTimestamp | IRDefault::RawExpression(_) => None,
        }
    }
    fn format_literal(&self, _: &Option<Value>, _: &IRType) -> String {
        "NULL".into()
    }
    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("IDENTITY(1,1)")
    }
}

pub fn column(name: &str, primary: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: "integer".into(),
        nullable: false,
        default_value: None,
        comment: None,
        is_primary_key: primary,
        is_auto_increment: false,
    }
}

pub fn schema(name: &str, columns: &[(&str, bool)]) -> TableSchema {
    let primary_keys = columns
        .iter()
        .filter(|(_, primary)| *primary)
        .map(|(name, _)| (*name).to_string())
        .collect();
    TableSchema {
        table_name: name.into(),
        columns: columns
            .iter()
            .map(|(name, primary)| column(name, *primary))
            .collect(),
        primary_keys,
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

/// Add one secondary (non-primary) index to a source schema.
pub fn with_index(
    mut schema: TableSchema,
    name: &str,
    columns: &[&str],
    is_unique: bool,
) -> TableSchema {
    schema.indexes.push(IndexInfo {
        name: name.into(),
        columns: columns.iter().map(|name| (*name).into()).collect(),
        is_unique,
        is_primary: false,
        index_type: "btree".into(),
    });
    schema
}

/// Add one index flagged as the primary key to a source schema.
pub fn with_primary_index(
    mut schema: TableSchema,
    name: &str,
    columns: &[&str],
) -> TableSchema {
    schema.indexes.push(IndexInfo {
        name: name.into(),
        columns: columns.iter().map(|name| (*name).into()).collect(),
        is_unique: true,
        is_primary: true,
        index_type: "clustered".into(),
    });
    schema
}

/// Same as [`mapping`] but without any column mapping, so constraint columns
/// reach the target renderer exactly as the source catalog reported them.
pub fn mapping_without_columns(source: &str, target: &str) -> TableMapping {
    mapping(source, target, &[])
}

pub fn mapping(source: &str, target: &str, columns: &[(&str, &str)]) -> TableMapping {
    TableMapping {
        source_table: source.into(),
        target_table: target.into(),
        create_new: true,
        enabled: true,
        column_mappings: columns
            .iter()
            .map(|(source, target)| ColumnMapping {
                source_column: (*source).into(),
                target_column: (*target).into(),
                skip: false,
                target_native_type: None,
            })
            .collect(),
        ddl_override: None,
        source_filter: None,
        recordset: None,
    }
}

/// Same as [`mapping`], but marks the source column as skipped in the target.
pub fn mapping_with_skipped_column(
    source: &str,
    target: &str,
    columns: &[(&str, &str)],
    skipped: &str,
) -> TableMapping {
    let mut mapping = mapping(source, target, columns);
    for column in mapping.column_mappings.iter_mut() {
        if column.source_column == skipped {
            column.skip = true;
        }
    }
    mapping
}

pub fn inspected(
    source: &str,
    target: &str,
    columns: &[(&str, &str)],
) -> TableInspectResult {
    TableInspectResult {
        source_table: source.into(),
        target_table: target.into(),
        status: TableMappingStatus::CreateNew,
        create_new: true,
        enabled: true,
        column_mappings: columns
            .iter()
            .map(|(source, target)| ColumnMapping {
                source_column: (*source).into(),
                target_column: (*target).into(),
                skip: false,
                target_native_type: None,
            })
            .collect(),
        source_columns: Vec::new(),
        source_primary_keys: Vec::new(),
        target_columns: Vec::new(),
        source_column_types: HashMap::new(),
        target_column_types: HashMap::new(),
        incompatible_reason: None,
        source_row_count: None,
        recordset: None,
    }
}

pub fn job(tables: Vec<TableMapping>) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: "source".into(),
            database: "source_db".into(),
            schema: Some("public".into()),
        },
        target: Some(Endpoint {
            db_session_id: "target".into(),
            database: "target_db".into(),
            schema: Some("archive".into()),
        }),
        sql_file_target: None,
        mode: TransferMode::StructureAndData,
        write_mode: WriteMode::Insert,
        tables,
        options: TransferOptions::default(),
    }
}
