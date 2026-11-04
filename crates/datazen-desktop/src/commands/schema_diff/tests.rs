//! Schema Diff command unit tests.
//!
//! The test bodies are moved verbatim out of the former `schema_diff` god
//! module and grouped by responsibility so that no single file grows past the
//! repository's 800-line budget.
//!
//! host-boundary F4c-3：算法测试（目录读取、选择器解析、范围门）随实现
//! 下沉 `modules` 的 `schema_diff::api` 测试；本树保留的是经宿主装配的
//! 计划/部署/档案集成测试与端口映射测试。

use super::*;
use crate::schema_diff::SchemaDiffProfile;
use crate::testing::mock_driver::MockDriverOptions;
use datazen_driver_api::{
    ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, TableInfo, TableType,
};
use std::collections::HashMap;

mod plan_deploy;

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
