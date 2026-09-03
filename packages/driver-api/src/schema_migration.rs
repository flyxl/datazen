//! Dialect-neutral schema migration contracts exposed by the driver API.

use crate::{ColumnSchema, IndexInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOperation {
    AddColumn { table: String, column: MigrationColumn },
    DropColumn { table: String, column: MigrationColumn },
    AlterColumnType { table: String, column: String, from: String, to: String },
    SetNullable { table: String, column: String, nullable: bool },
    SetDefault { table: String, column: String, from: Option<String>, to: Option<String> },
    SetComment { table: String, column: String, from: Option<String>, to: Option<String> },
    SetAutoIncrement { table: String, column: String, from: bool, to: bool },
    AddPrimaryKey { table: String, columns: Vec<String> },
    DropPrimaryKey { table: String, columns: Vec<String> },
    CreateIndex { table: String, index: IndexInfo },
    DropIndex { table: String, index: IndexInfo },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationRisk { Additive, Rewrite, Destructive }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRequirement {
    Backfill { table: String, column: String, reason: String },
    Unsupported { operation: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatement {
    pub sql: String,
    pub risk: MigrationRisk,
    pub rollback_sql: Option<String>,
    pub summary: String,
}

pub trait MigrationRenderer: Send + Sync {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String>;
}

pub trait MigrationCapabilities: Send + Sync {
    fn supports(&self, operation: &MigrationOperation) -> bool;
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool { !self.supports(operation) }
}

pub fn migration_column(column: &ColumnSchema) -> MigrationColumn {
    MigrationColumn {
        name: column.name.clone(),
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        default_value: column.default_value.clone(),
        comment: column.comment.clone(),
        is_auto_increment: column.is_auto_increment,
    }
}
