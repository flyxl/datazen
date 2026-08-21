//! MongoDB sync adapter smoke (no live server).

use datazen_driver_api::{ColumnSchema, IRType, SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_mongodb::MongodbSyncAdapter;

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
fn mongodb_column_to_ir_maps_bson_types() {
    let adapter = MongodbSyncAdapter;
    assert_eq!(
        adapter.column_to_ir(&col("name", "string"), None).ir_type,
        IRType::Text
    );
    assert_eq!(
        adapter.column_to_ir(&col("meta", "object"), None).ir_type,
        IRType::Json
    );
    assert!(
        adapter
            .column_to_ir(&col("_id", "objectId"), None)
            .is_primary_key
    );
}

#[test]
fn mongodb_ir_type_to_native_roundtrip() {
    let adapter = MongodbSyncAdapter;
    assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "bool");
    assert_eq!(adapter.ir_type_to_native(&IRType::Json), "object");
    assert_eq!(adapter.ir_type_to_native(&IRType::Int64), "long");
}
