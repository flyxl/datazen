//! InfluxDB sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct InfluxDbSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(InfluxDbSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["influxdb"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn influx_type_to_ir(raw: &str) -> IRType {
    match raw.trim().to_lowercase().as_str() {
        "float" => IRType::Float64,
        "integer" => IRType::Int64,
        "string" => IRType::Text,
        "boolean" | "bool" => IRType::Bool,
        _ => IRType::Other(raw.to_string()),
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for InfluxDbSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = influx_type_to_ir(raw);

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
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for InfluxDbSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "boolean".into(),
            IRType::Int8 | IRType::Int16 | IRType::Int32 | IRType::Int64 => "integer".into(),
            IRType::Float32 | IRType::Float64 => "float".into(),
            IRType::Decimal { .. } => "float".into(),
            IRType::Char { .. }
            | IRType::Varchar { .. }
            | IRType::Text
            | IRType::Date
            | IRType::Time { .. }
            | IRType::Timestamp { .. }
            | IRType::Json
            | IRType::Uuid
            | IRType::Bit { .. } => "string".into(),
            IRType::Binary { .. } | IRType::Blob => "string".into(),
            IRType::Other(_) => "string".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("now()".into()),
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
    fn influx_field_types() {
        let a = InfluxDbSyncAdapter;
        assert_eq!(a.column_to_ir(&col("v", "float"), None).ir_type, IRType::Float64);
        assert_eq!(a.column_to_ir(&col("v", "integer"), None).ir_type, IRType::Int64);
        assert_eq!(a.column_to_ir(&col("host", "string"), None).ir_type, IRType::Text);
        assert_eq!(a.column_to_ir(&col("ok", "boolean"), None).ir_type, IRType::Bool);
    }

    #[test]
    fn influx_unknown_type() {
        let a = InfluxDbSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("x", "tag"), None).ir_type,
            IRType::Other("tag".into())
        );
    }

    #[test]
    fn influx_target_roundtrip() {
        let a = InfluxDbSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Float64), "float");
        assert_eq!(a.ir_type_to_native(&IRType::Int64), "integer");
        assert_eq!(a.ir_type_to_native(&IRType::Text), "string");
        assert!(!a.supports_primary_key());
    }

    #[test]
    fn influx_format_literal() {
        let a = InfluxDbSyncAdapter;
        assert_eq!(a.format_literal(&None, &IRType::Float64), "NULL");
        assert_eq!(
            a.format_literal(&Some(Value::Float(1.5)), &IRType::Float64),
            "1.5"
        );
    }
}
