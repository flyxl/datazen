//! HBase sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct HBaseSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(HBaseSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["hbase"],
        create,
    }
}

fn hbase_type_to_ir(raw: &str) -> IRType {
    match raw.trim().to_lowercase().as_str() {
        "family" => IRType::Other("family".into()),
        _ => IRType::Text,
    }
}

impl SyncSourceAdapter for HBaseSyncAdapter {
    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = hbase_type_to_ir(raw);

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: column.comment.clone(),
        }
    }
}

impl SyncTargetAdapter for HBaseSyncAdapter {
    fn ir_type_to_native(&self, _ir_type: &IRType) -> String {
        "TEXT".into()
    }

    fn format_default(&self, _default: &IRDefault) -> Option<String> {
        None
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(b)) => if *b { "true" } else { "false" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => format!(
                "'\\x{}'",
                b.iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<String>()
            ),
        }
    }

    fn supports_primary_key(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn hbase_family_to_ir() {
        let ir = HBaseSyncAdapter.column_to_ir(&col("cf1", "family"), None);
        assert_eq!(ir.ir_type, IRType::Other("family".into()));
        assert!(!ir.is_primary_key);
    }

    #[test]
    fn hbase_unknown_to_text() {
        let ir = HBaseSyncAdapter.column_to_ir(&col("row", "string"), None);
        assert_eq!(ir.ir_type, IRType::Text);
    }

    #[test]
    fn hbase_target_degrades_to_text() {
        let adapter = HBaseSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Int32), "TEXT");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Other("family".into())),
            "TEXT"
        );
        assert!(!adapter.supports_primary_key());
    }

    #[test]
    fn hbase_format_literal() {
        let adapter = HBaseSyncAdapter;
        assert_eq!(adapter.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            adapter.format_literal(&Some(Value::String("a'b".into())), &IRType::Text),
            "'a''b'"
        );
    }
}
