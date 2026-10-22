//! Schema Diff command unit tests.
//!
//! The test bodies are moved verbatim out of the former `schema_diff` god
//! module and grouped by responsibility so that no single file grows past the
//! repository's 800-line budget.

use super::*;
use crate::schema_diff::types::{PlanStatement, RollbackCompleteness, StatementRisk};
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, TableInfo, TableType,
};
use std::collections::HashMap;

mod catalog;
mod plan_deploy;
mod selectors;

fn pg_fk(name: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: vec!["parent_id".into()],
        referenced_table: "public.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    }
}

fn pg_parent_schema(name: &str) -> datazen_driver_api::TableSchema {
    datazen_driver_api::TableSchema {
        table_name: name.into(),
        columns: vec![ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        }],
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn pg_child_schema(name: &str, include_fk: bool) -> datazen_driver_api::TableSchema {
    datazen_driver_api::TableSchema {
        table_name: name.into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "parent_id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        foreign_keys: if include_fk {
            vec![pg_fk("fk_child_parent")]
        } else {
            Vec::new()
        },
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn target_table(name: &str, schema: Option<&str>) -> TableInfo {
    TableInfo {
        name: name.into(),
        schema: schema.map(str::to_owned),
        table_type: TableType::Table,
        row_count: None,
    }
}
