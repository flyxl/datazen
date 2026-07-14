//! Trino / Presto sync adapter.

use crate::db::{ColumnSchema, Value};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ir::{IRColumn, IRDefault, IRType};

pub struct TrinoSyncAdapter;

// ── helpers ────────────────────────────────────────────────────────

fn parse_length(s: &str, prefix: &str) -> Option<u32> {
    s.strip_prefix(prefix)
        .and_then(|r| r.trim().strip_prefix('('))
        .and_then(|r| r.strip_suffix(')'))
        .and_then(|n| n.trim().parse().ok())
}

fn parse_precision(s: &str, prefix: &str) -> (u8, u8) {
    if let Some(rest) = s.strip_prefix(prefix) {
        let rest = rest.trim();
        if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').collect();
            let p = parts.first().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            let s = parts.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            return (p, s);
        }
    }
    (0, 0)
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for TrinoSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let lower = raw.trim().to_lowercase();

        let ir_type = if lower.starts_with("varchar") {
            let len = parse_length(&lower, "varchar");
            IRType::Varchar { length: len }
        } else if lower.starts_with("char(") {
            let len = parse_length(&lower, "char").unwrap_or(1);
            IRType::Char { length: len }
        } else if lower.starts_with("decimal") {
            let (p, s) = parse_precision(&lower, "decimal");
            IRType::Decimal { precision: p, scale: s }
        } else if lower.starts_with("timestamp") {
            let tz = lower.contains("with time zone");
            IRType::Timestamp { with_timezone: tz }
        } else if lower.starts_with("time") {
            let tz = lower.contains("with time zone");
            IRType::Time { with_timezone: tz }
        } else if lower.starts_with("array") || lower.starts_with("map") || lower.starts_with("row") {
            IRType::Json
        } else {
            match lower.as_str() {
                "boolean" => IRType::Bool,
                "tinyint" => IRType::Int8,
                "smallint" => IRType::Int16,
                "integer" | "int" => IRType::Int32,
                "bigint" => IRType::Int64,
                "real" => IRType::Float32,
                "double" => IRType::Float64,
                "varbinary" => IRType::Blob,
                "date" => IRType::Date,
                "json" => IRType::Json,
                "uuid" => IRType::Uuid,
                _ => IRType::Other(raw.to_string()),
            }
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: None,
            is_primary_key: column.is_primary_key,
            is_auto_increment: false,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for TrinoSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "boolean".into(),
            IRType::Int8 => "tinyint".into(),
            IRType::Int16 => "smallint".into(),
            IRType::Int32 => "integer".into(),
            IRType::Int64 => "bigint".into(),
            IRType::Float32 => "real".into(),
            IRType::Float64 => "double".into(),
            IRType::Decimal { precision: 0, .. } => "decimal".into(),
            IRType::Decimal { precision, scale } => format!("decimal({precision},{scale})"),
            IRType::Char { length } => format!("char({length})"),
            IRType::Varchar { length: Some(n) } => format!("varchar({n})"),
            IRType::Varchar { length: None } | IRType::Text => "varchar".into(),
            IRType::Binary { .. } | IRType::Blob => "varbinary".into(),
            IRType::Date => "date".into(),
            IRType::Time { with_timezone: false } => "time".into(),
            IRType::Time { with_timezone: true } => "time with time zone".into(),
            IRType::Timestamp { with_timezone: false } => "timestamp".into(),
            IRType::Timestamp { with_timezone: true } => "timestamp with time zone".into(),
            IRType::Json => "json".into(),
            IRType::Uuid => "uuid".into(),
            IRType::Bit { .. } => "boolean".into(),
            IRType::Other(_) => "varchar".into(),
        }
    }

    fn format_default(&self, _default: &IRDefault) -> Option<String> {
        // Trino connectors generally do not support DEFAULT values in DDL.
        None
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("TIMESTAMP '{}'", s),
            Some(Value::Json(j)) => format!("JSON '{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "X'{}'",
                    b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
                )
            }
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
    fn trino_varchar_to_ir() {
        let ir = TrinoSyncAdapter.column_to_ir(&col("name", "varchar(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn trino_unbounded_varchar() {
        let ir = TrinoSyncAdapter.column_to_ir(&col("bio", "varchar"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: None });
    }

    #[test]
    fn trino_array_to_json() {
        let ir = TrinoSyncAdapter.column_to_ir(&col("tags", "array(varchar)"), None);
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn trino_uuid() {
        let ir = TrinoSyncAdapter.column_to_ir(&col("id", "uuid"), None);
        assert_eq!(ir.ir_type, IRType::Uuid);
    }

    #[test]
    fn trino_timestamp_with_tz() {
        let ir = TrinoSyncAdapter
            .column_to_ir(&col("ts", "timestamp(3) with time zone"), None);
        assert_eq!(ir.ir_type, IRType::Timestamp { with_timezone: true });
    }

    #[test]
    fn trino_no_primary_key() {
        assert!(!TrinoSyncAdapter.supports_primary_key());
    }

    #[test]
    fn trino_target_types() {
        let a = TrinoSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Text), "varchar");
        assert_eq!(a.ir_type_to_native(&IRType::Blob), "varbinary");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "uuid");
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "boolean");
        assert_eq!(
            a.ir_type_to_native(&IRType::Decimal { precision: 10, scale: 2 }),
            "decimal(10,2)"
        );
    }

    #[test]
    fn trino_no_default_support() {
        assert_eq!(
            TrinoSyncAdapter.format_default(&IRDefault::CurrentTimestamp),
            None
        );
    }
}
