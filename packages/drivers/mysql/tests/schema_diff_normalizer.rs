use datazen_driver_api::{ColumnSchema, TableSchema};
use datazen_modules::migration::schema_diff::diff_table_schemas;

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
fn mysql_int_display_width_not_reported_as_changed() {
    use datazen_driver_mysql::MysqlTypeNormalizer;
    let normalizer = MysqlTypeNormalizer;
    let src = schema(vec![col("id", "int(11)")]);
    let tgt = schema(vec![col("id", "int")]);
    let diff = diff_table_schemas("users", &src, &tgt, Some(&normalizer));
    assert!(diff.changed.is_empty(), "int(11) vs int should match");
}
