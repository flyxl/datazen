//! Elasticsearch sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct ElasticsearchSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(ElasticsearchSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["elasticsearch"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn es_type_to_ir(raw: &str) -> IRType {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "text" | "search_as_you_type" => IRType::Text,
        "keyword" => IRType::Varchar { length: None },
        "long" => IRType::Int64,
        "integer" => IRType::Int32,
        "short" => IRType::Int16,
        "byte" => IRType::Int8,
        "double" => IRType::Float64,
        "float" => IRType::Float32,
        "half_float" => IRType::Float32,
        "boolean" | "bool" => IRType::Bool,
        "date" | "date_nanos" => IRType::Timestamp { with_timezone: false },
        "binary" => IRType::Blob,
        "object" | "nested" | "flattened" => IRType::Json,
        "geo_point" | "geo_shape" | "dense_vector" => IRType::Other(raw.to_string()),
        _ if lower.starts_with("geo_") => IRType::Other(raw.to_string()),
        _ => IRType::Other(raw.to_string()),
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for ElasticsearchSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = es_type_to_ir(raw);

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

impl SyncTargetAdapter for ElasticsearchSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "boolean".into(),
            IRType::Int8 | IRType::Int16 => "short".into(),
            IRType::Int32 => "integer".into(),
            IRType::Int64 => "long".into(),
            IRType::Float32 => "float".into(),
            IRType::Float64 => "double".into(),
            IRType::Decimal { .. } => "double".into(),
            IRType::Char { .. } | IRType::Varchar { .. } => "keyword".into(),
            IRType::Text => "text".into(),
            IRType::Binary { .. } | IRType::Blob => "binary".into(),
            IRType::Date | IRType::Time { .. } | IRType::Timestamp { .. } => "date".into(),
            IRType::Json => "object".into(),
            IRType::Uuid => "keyword".into(),
            IRType::Bit { .. } => "boolean".into(),
            IRType::Other(_) => "keyword".into(),
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
    fn es_text_and_keyword() {
        let a = ElasticsearchSyncAdapter;
        assert_eq!(a.column_to_ir(&col("body", "text"), None).ir_type, IRType::Text);
        assert_eq!(
            a.column_to_ir(&col("status", "keyword"), None).ir_type,
            IRType::Varchar { length: None }
        );
    }

    #[test]
    fn es_numeric_types() {
        let a = ElasticsearchSyncAdapter;
        assert_eq!(a.column_to_ir(&col("n", "long"), None).ir_type, IRType::Int64);
        assert_eq!(a.column_to_ir(&col("n", "integer"), None).ir_type, IRType::Int32);
        assert_eq!(a.column_to_ir(&col("n", "double"), None).ir_type, IRType::Float64);
    }

    #[test]
    fn es_object_and_geo() {
        let a = ElasticsearchSyncAdapter;
        assert_eq!(a.column_to_ir(&col("meta", "object"), None).ir_type, IRType::Json);
        assert_eq!(
            a.column_to_ir(&col("loc", "geo_point"), None).ir_type,
            IRType::Other("geo_point".into())
        );
    }

    #[test]
    fn es_target_roundtrip() {
        let a = ElasticsearchSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "boolean");
        assert_eq!(a.ir_type_to_native(&IRType::Json), "object");
        assert_eq!(a.ir_type_to_native(&IRType::Text), "text");
        assert!(!a.supports_primary_key());
    }

    #[test]
    fn es_format_literal() {
        let a = ElasticsearchSyncAdapter;
        assert_eq!(a.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            a.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "true"
        );
    }
}
