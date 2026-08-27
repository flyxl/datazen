//! Unit tests for Data Transfer IPC helpers.

use crate::data_transfer::mapping::auto_map_columns;
use crate::data_transfer::{classify_transfer_pair, TableMapping};
use datazen_driver_api::{ColumnSchema, TableSchema};

#[test]
fn pairing_classifies_ir_and_unsupported() {
    let ok = classify_transfer_pair("postgresql", "mysql");
    assert!(ok.supported);
    assert_eq!(ok.path, "ir");

    let bad = classify_transfer_pair("postgresql", "redis");
    assert!(!bad.supported);
}

#[test]
fn auto_map_columns_matches_names_only() {
    let src = TableSchema {
        table_name: "t".into(),
        columns: vec![
            ColumnSchema {
                name: "a".into(),
                data_type: "int".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "b".into(),
                data_type: "text".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let tgt = TableSchema {
        table_name: "t".into(),
        columns: vec![ColumnSchema {
            name: "a".into(),
            data_type: "integer".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let maps = auto_map_columns(&src, &tgt);
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].source_column, "a");
}

#[test]
fn table_mapping_auto_sets_same_name() {
    let m = TableMapping::auto("users");
    assert_eq!(m.source_table, "users");
    assert_eq!(m.target_table, "users");
    assert!(m.enabled);
    assert!(!m.create_new);
}
