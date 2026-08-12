//! Vector store sync adapter (Qdrant-style collections).

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct VectorSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(VectorSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["vector"],
        create,
    }
}

const DISTANCE_METRICS: &[&str] = &["cosine", "euclid", "dot", "manhattan"];

fn vector_type_to_ir(raw: &str) -> IRType {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();

    if lower == "string" {
        return IRType::Text;
    }
    if lower == "vector" {
        return IRType::Other("vector".into());
    }
    if lower.starts_with("float[") && lower.ends_with(']') {
        return IRType::Other(trimmed.to_string());
    }
    if DISTANCE_METRICS.contains(&lower.as_str()) {
        return IRType::Text;
    }
    IRType::Other(trimmed.to_string())
}

impl SyncSourceAdapter for VectorSyncAdapter {
    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = vector_type_to_ir(raw);

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column.default_value.as_deref().and_then(|d| {
                let d = d.trim();
                if d.is_empty() {
                    return None;
                }
                Some(IRDefault::Literal(d.to_string()))
            }),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

impl SyncTargetAdapter for VectorSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Float32 | IRType::Float64 => "DOUBLE".into(),
            IRType::Int8
            | IRType::Int16
            | IRType::Int32
            | IRType::Int64
            | IRType::Decimal { .. }
            | IRType::Bool
            | IRType::Bit { .. } => "DOUBLE".into(),
            IRType::Binary { .. } | IRType::Blob => "TEXT".into(),
            IRType::Other(s) if s.starts_with("float[") => "TEXT".into(),
            IRType::Other(_) => "TEXT".into(),
            _ => "TEXT".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::CurrentTimestamp | IRDefault::RawExpression(_) => None,
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

    fn pk_col(name: &str, data_type: &str) -> ColumnSchema {
        let mut c = col(name, data_type);
        c.is_primary_key = true;
        c.nullable = false;
        c
    }

    #[test]
    fn vector_float_array_to_ir() {
        let ir = VectorSyncAdapter.column_to_ir(&col("vector", "float[128]"), None);
        assert_eq!(ir.ir_type, IRType::Other("float[128]".into()));
    }

    #[test]
    fn vector_id_string_to_ir() {
        let ir = VectorSyncAdapter.column_to_ir(&pk_col("id", "string"), None);
        assert_eq!(ir.ir_type, IRType::Text);
        assert!(ir.is_primary_key);
    }

    #[test]
    fn vector_distance_metric_to_text() {
        let ir = VectorSyncAdapter.column_to_ir(&col("distance", "Cosine"), None);
        assert_eq!(ir.ir_type, IRType::Text);
    }

    #[test]
    fn vector_target_degrades_types() {
        let adapter = VectorSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Float64), "DOUBLE");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Other("float[256]".into())),
            "TEXT"
        );
        assert_eq!(adapter.ir_type_to_native(&IRType::Text), "TEXT");
        assert!(adapter.supports_primary_key());
    }

    #[test]
    fn vector_format_literal() {
        let adapter = VectorSyncAdapter;
        assert_eq!(adapter.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            adapter.format_literal(&Some(Value::Float(1.5)), &IRType::Float64),
            "1.5"
        );
    }
}
