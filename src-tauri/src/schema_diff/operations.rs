//! Dialect-neutral schema migration operations.

use super::types::{ColumnSnapshot, StatementRisk};
use crate::db::IndexInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOperation {
    CreateTable { table: String, columns: Vec<ColumnSnapshot>, primary_keys: Vec<String> },
    AddColumn { table: String, column: ColumnSnapshot },
    DropColumn { table: String, column: ColumnSnapshot },
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

impl MigrationOperation {
    pub fn risk(&self) -> StatementRisk {
        match self {
            Self::DropColumn { .. } | Self::DropPrimaryKey { .. } | Self::DropIndex { .. } => StatementRisk::Destructive,
            Self::AlterColumnType { .. } | Self::SetNullable { nullable: false, .. } | Self::SetAutoIncrement { .. } => StatementRisk::Rewrite,
            _ => StatementRisk::Additive,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::CreateTable { table, .. } | Self::AddPrimaryKey { table, .. } | Self::DropPrimaryKey { table, .. } => format!("table:{table}"),
            Self::AddColumn { table, column } | Self::DropColumn { table, column } => format!("column:{table}.{}", column.name),
            Self::AlterColumnType { table, column, .. } | Self::SetNullable { table, column, .. } |
            Self::SetDefault { table, column, .. } | Self::SetComment { table, column, .. } |
            Self::SetAutoIncrement { table, column, .. } => format!("column:{table}.{column}"),
            Self::CreateIndex { table, index } | Self::DropIndex { table, index } => format!("index:{table}.{}", index.name),
        }
    }
}
