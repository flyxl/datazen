//! ClickHouse sync adapter smoke (no live server).

use datazen_driver_api::{ColumnSchema, IRType, SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_clickhouse::ClickHouseSyncAdapter;

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
fn clickhouse_nullable_string_to_ir() {
    let adapter = ClickHouseSyncAdapter;
    let ir = adapter.column_to_ir(&col("name", "Nullable(String)"), None);
    assert_eq!(ir.ir_type, IRType::Text);
    assert!(ir.nullable);
}

#[test]
fn clickhouse_ir_type_to_native_and_quote_ident() {
    let adapter = ClickHouseSyncAdapter;
    assert_eq!(adapter.ir_type_to_native(&IRType::Uuid), "UUID");
    assert_eq!(
        adapter.ir_type_to_native(&IRType::Char { length: 8 }),
        "FixedString(8)"
    );
    assert_eq!(adapter.quote_ident("events"), "`events`");
    assert!(!adapter.supports_primary_key());
}
