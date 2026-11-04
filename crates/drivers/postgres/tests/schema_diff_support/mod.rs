use datazen_driver_api::{ColumnSchema, TableSchema};

pub fn col(name: &str, ty: &str) -> ColumnSchema {
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

pub fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: cols,
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}
