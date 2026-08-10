//! MongoDB sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct MongodbSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(MongodbSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["mongodb"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn mongo_type_to_ir(raw: &str) -> IRType {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "string" => IRType::Text,
        "int32" | "int" => IRType::Int32,
        "int64" | "long" => IRType::Int64,
        "double" | "float" => IRType::Float64,
        "decimal128" => IRType::Decimal {
            precision: 0,
            scale: 0,
        },
        "bool" | "boolean" => IRType::Bool,
        "datetime" | "date" | "timestamp" => IRType::Timestamp { with_timezone: false },
        "objectid" => IRType::Varchar { length: Some(24) },
        "document" | "object" => IRType::Json,
        "array" => IRType::Json,
        "binary" => IRType::Blob,
        "null" | "unknown" | "other" => IRType::Text,
        _ => IRType::Other(raw.to_string()),
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for MongodbSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = mongo_type_to_ir(raw);

        let default_expr = column.default_value.as_deref().and_then(|d| {
            let d = d.trim();
            if d.is_empty() {
                return None;
            }
            Some(IRDefault::Literal(d.to_string()))
        });

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr,
            is_primary_key: column.is_primary_key || column.name == "_id",
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for MongodbSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "bool".into(),
            IRType::Int8 | IRType::Int16 | IRType::Int32 => "int".into(),
            IRType::Int64 => "long".into(),
            IRType::Float32 | IRType::Float64 => "double".into(),
            IRType::Decimal { .. } => "decimal".into(),
            IRType::Char { .. } | IRType::Varchar { .. } | IRType::Text | IRType::Uuid => {
                "string".into()
            }
            IRType::Binary { .. } | IRType::Blob => "binData".into(),
            IRType::Date | IRType::Time { .. } | IRType::Timestamp { .. } => "date".into(),
            IRType::Json => "object".into(),
            IRType::Bit { .. } => "bool".into(),
            IRType::Other(_) => "string".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("new Date()".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(_) => None,
        }
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
                b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
            ),
        }
    }

    fn supports_primary_key(&self) -> bool {
        true
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
    fn mongo_bson_types() {
        let a = MongodbSyncAdapter;
        assert_eq!(a.column_to_ir(&col("name", "string"), None).ir_type, IRType::Text);
        assert_eq!(a.column_to_ir(&col("n", "int"), None).ir_type, IRType::Int32);
        assert_eq!(a.column_to_ir(&col("n", "Int64"), None).ir_type, IRType::Int64);
        assert_eq!(a.column_to_ir(&col("n", "double"), None).ir_type, IRType::Float64);
        assert_eq!(a.column_to_ir(&col("ok", "bool"), None).ir_type, IRType::Bool);
    }

    #[test]
    fn mongo_complex_types() {
        let a = MongodbSyncAdapter;
        assert_eq!(a.column_to_ir(&col("meta", "object"), None).ir_type, IRType::Json);
        assert_eq!(a.column_to_ir(&col("tags", "array"), None).ir_type, IRType::Json);
        assert_eq!(
            a.column_to_ir(&col("id", "objectId"), None).ir_type,
            IRType::Varchar { length: Some(24) }
        );
        assert_eq!(
            a.column_to_ir(&col("price", "decimal128"), None).ir_type,
            IRType::Decimal { precision: 0, scale: 0 }
        );
    }

    #[test]
    fn mongo_id_is_primary_key() {
        let a = MongodbSyncAdapter;
        assert!(a.column_to_ir(&col("_id", "objectId"), None).is_primary_key);
        assert!(a.supports_primary_key());
    }

    #[test]
    fn mongo_target_roundtrip() {
        let a = MongodbSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "bool");
        assert_eq!(a.ir_type_to_native(&IRType::Json), "object");
        assert_eq!(a.ir_type_to_native(&IRType::Int64), "long");
    }

    #[test]
    fn mongo_format_literal() {
        let a = MongodbSyncAdapter;
        assert_eq!(a.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            a.format_literal(&Some(Value::String("a'b".into())), &IRType::Text),
            "'a''b'"
        );
    }
}
