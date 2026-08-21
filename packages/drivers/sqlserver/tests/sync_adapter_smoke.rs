//! SQL Server sync adapter smoke (no live server).

use datazen_driver_api::{ColumnSchema, IRType, SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_sqlserver::SqlServerSyncAdapter;

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
fn sqlserver_nvarchar_and_bit_to_ir() {
    let adapter = SqlServerSyncAdapter;
    assert_eq!(
        adapter
            .column_to_ir(&col("name", "nvarchar(100)"), None)
            .ir_type,
        IRType::Varchar { length: Some(100) }
    );
    assert_eq!(
        adapter.column_to_ir(&col("active", "bit"), None).ir_type,
        IRType::Bool
    );
}

#[test]
fn sqlserver_quote_ident_and_target_types() {
    let adapter = SqlServerSyncAdapter;
    assert_eq!(adapter.quote_ident("col]name"), "[col]]name]");
    assert_eq!(adapter.ir_type_to_native(&IRType::Int32), "INT");
    assert_eq!(adapter.auto_increment_keyword(), Some("IDENTITY(1,1)"));
}
