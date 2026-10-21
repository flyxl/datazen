use datazen_driver_api::{ColumnSchema, TableSchema};
use datazen_schema_diff::{
    diff_table_schemas, ir::diff_to_operations, operations::MigrationOperation,
};

fn col(name: &str, ty: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: ty.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}
fn schema(columns: Vec<ColumnSchema>) -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns,
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

#[test]
fn postgres_int4_integer_not_reported_as_changed() {
    use datazen_driver_postgres::PostgresTypeNormalizer;
    let normalizer = PostgresTypeNormalizer;
    let src = schema(vec![col("id", "int4")]);
    let tgt = schema(vec![col("id", "integer")]);
    let diff = diff_table_schemas("users", &src, &tgt, Some(&normalizer));
    assert!(diff.changed.is_empty(), "int4 vs integer should match");
}

#[test]
fn postgres_type_alias_does_not_emit_alter_column_type() {
    use datazen_driver_postgres::PostgresTypeNormalizer;
    let normalizer = PostgresTypeNormalizer;
    let mut src_col = col("id", "integer");
    src_col.data_type = "int4".into();
    let mut tgt_col = col("id", "integer");
    tgt_col.data_type = "integer".into();
    let s = schema(vec![src_col]);
    let t = schema(vec![tgt_col]);
    let ops = diff_to_operations("t", &s, &t, Some(&normalizer));
    assert!(!ops
        .iter()
        .any(|op| matches!(op, MigrationOperation::AlterColumnType { .. })));
}
