//! VictoriaMetrics sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct VictoriaMetricsSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(VictoriaMetricsSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["victoriametrics"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn vm_type_to_ir(raw: &str) -> IRType {
    match raw.trim().to_lowercase().as_str() {
        "float" | "double" | "number" => IRType::Float64,
        "string" | "label" | "text" => IRType::Text,
        "bool" | "boolean" => IRType::Bool,
        "integer" | "int" => IRType::Int64,
        _ => IRType::Other(raw.to_string()),
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for VictoriaMetricsSyncAdapter {
    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = vm_type_to_ir(raw);

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

impl SyncTargetAdapter for VictoriaMetricsSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Float32 | IRType::Float64 | IRType::Decimal { .. } => "float".into(),
            IRType::Int8 | IRType::Int16 | IRType::Int32 | IRType::Int64 => "string".into(),
            IRType::Bool => "string".into(),
            IRType::Char { .. }
            | IRType::Varchar { .. }
            | IRType::Text
            | IRType::Date
            | IRType::Time { .. }
            | IRType::Timestamp { .. }
            | IRType::Json
            | IRType::Uuid
            | IRType::Binary { .. }
            | IRType::Blob
            | IRType::Bit { .. } => "string".into(),
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
    fn vm_label_and_value_types() {
        let a = VictoriaMetricsSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("job", "string"), None).ir_type,
            IRType::Text
        );
        assert_eq!(
            a.column_to_ir(&col("value", "float"), None).ir_type,
            IRType::Float64
        );
    }

    #[test]
    fn vm_unknown_maps_to_other() {
        let a = VictoriaMetricsSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("x", "histogram"), None).ir_type,
            IRType::Other("histogram".into())
        );
    }

    #[test]
    fn vm_target_degrades_to_string() {
        let a = VictoriaMetricsSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Text), "string");
        assert_eq!(a.ir_type_to_native(&IRType::Float64), "float");
        assert_eq!(a.ir_type_to_native(&IRType::Int64), "string");
        assert!(!a.supports_primary_key());
    }

    #[test]
    fn vm_format_literal() {
        let a = VictoriaMetricsSyncAdapter;
        assert_eq!(a.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            a.format_literal(&Some(Value::String("host-1".into())), &IRType::Text),
            "'host-1'"
        );
    }
}
