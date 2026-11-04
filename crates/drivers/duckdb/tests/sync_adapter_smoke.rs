//! DuckDB sync adapter smoke (no live server).

use datazen_driver_api::{ColumnSchema, IRType, SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_duckdb::DuckDbSyncAdapter;

fn col(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

#[test]
fn duckdb_varchar_and_json_to_ir() {
    let adapter = DuckDbSyncAdapter;
    assert_eq!(
        adapter
            .column_to_ir(&col("name", "VARCHAR(50)"), None)
            .ir_type,
        IRType::Varchar { length: Some(50) }
    );
    assert_eq!(
        adapter.column_to_ir(&col("meta", "JSON"), None).ir_type,
        IRType::Json
    );
}

#[test]
fn duckdb_ir_type_to_native_roundtrip() {
    let adapter = DuckDbSyncAdapter;
    assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "BOOLEAN");
    assert_eq!(
        adapter.ir_type_to_native(&IRType::Varchar { length: Some(50) }),
        "VARCHAR(50)"
    );
}
