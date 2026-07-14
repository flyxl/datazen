//! PostgreSQL sync adapter.

use crate::db::{ColumnSchema, Value};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ir::{IRColumn, IRDefault, IRType};

pub struct PgSyncAdapter;

// ── helpers ────────────────────────────────────────────────────────

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

fn parse_char_length(s: &str, prefix: &str) -> Option<u32> {
    s.strip_prefix(prefix)
        .and_then(|r| r.trim().strip_prefix('('))
        .and_then(|r| r.strip_suffix(')'))
        .and_then(|n| n.trim().parse().ok())
}

fn parse_pg_default(raw: &str, col: &ColumnSchema) -> Option<IRDefault> {
    let d = raw.trim();
    if d.is_empty() {
        return None;
    }
    if d.contains("nextval(") {
        return None;
    }
    if d == "now()" || d == "CURRENT_TIMESTAMP" || d == "current_timestamp" {
        return Some(IRDefault::CurrentTimestamp);
    }
    if d.contains("::") {
        let stripped = d.split("::").next().unwrap_or(d);
        return Some(IRDefault::Literal(stripped.to_string()));
    }
    let _ = col;
    Some(IRDefault::Literal(d.to_string()))
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for PgSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let lower = raw.trim().to_lowercase();

        let ir_type = if lower.ends_with("[]") || lower == "array" {
            IRType::Json
        } else if lower.starts_with("character varying") {
            let len = parse_char_length(&lower, "character varying");
            IRType::Varchar { length: len }
        } else if lower.starts_with("character(") || lower == "character" {
            let len = parse_char_length(&lower, "character").unwrap_or(1);
            IRType::Char { length: len }
        } else if lower.starts_with("numeric") || lower.starts_with("decimal") {
            let prefix = if lower.starts_with("numeric") { "numeric" } else { "decimal" };
            let (p, s) = parse_precision(&lower, prefix);
            IRType::Decimal { precision: p, scale: s }
        } else if lower.starts_with("bit varying") {
            IRType::Blob
        } else if lower.starts_with("bit(") || lower == "bit" {
            let len = parse_char_length(&lower, "bit").unwrap_or(1);
            IRType::Bit { length: len }
        } else {
            match lower.as_str() {
                "integer" | "int" | "int4" => IRType::Int32,
                "bigint" | "int8" => IRType::Int64,
                "smallint" | "int2" => IRType::Int16,
                "text" => IRType::Text,
                "boolean" | "bool" => IRType::Bool,
                "real" | "float4" => IRType::Float32,
                "double precision" | "float8" => IRType::Float64,
                "bytea" => IRType::Blob,
                "json" | "jsonb" => IRType::Json,
                "uuid" => IRType::Uuid,
                "date" => IRType::Date,
                "time without time zone" | "time" => IRType::Time { with_timezone: false },
                "time with time zone" | "timetz" => IRType::Time { with_timezone: true },
                "timestamp without time zone" | "timestamp" => {
                    IRType::Timestamp { with_timezone: false }
                }
                "timestamp with time zone" | "timestamptz" => {
                    IRType::Timestamp { with_timezone: true }
                }
                "inet" => IRType::Varchar { length: Some(45) },
                "cidr" => IRType::Varchar { length: Some(43) },
                "macaddr" | "macaddr8" => IRType::Varchar { length: Some(17) },
                "interval" => IRType::Varchar { length: Some(255) },
                "money" => IRType::Decimal { precision: 19, scale: 2 },
                "oid" => IRType::Int32,
                "xml" => IRType::Text,
                _ => IRType::Other(raw.to_string()),
            }
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column
                .default_value
                .as_deref()
                .and_then(|d| parse_pg_default(d, column)),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for PgSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "boolean".into(),
            IRType::Int8 => "smallint".into(),
            IRType::Int16 => "smallint".into(),
            IRType::Int32 => "integer".into(),
            IRType::Int64 => "bigint".into(),
            IRType::Float32 => "real".into(),
            IRType::Float64 => "double precision".into(),
            IRType::Decimal { precision: 0, .. } => "numeric".into(),
            IRType::Decimal { precision, scale } => format!("numeric({precision},{scale})"),
            IRType::Char { length } => format!("character({length})"),
            IRType::Varchar { length: Some(n) } => format!("character varying({n})"),
            IRType::Varchar { length: None } | IRType::Text => "text".into(),
            IRType::Binary { .. } | IRType::Blob => "bytea".into(),
            IRType::Date => "date".into(),
            IRType::Time { with_timezone: false } => "time without time zone".into(),
            IRType::Time { with_timezone: true } => "time with time zone".into(),
            IRType::Timestamp { with_timezone: false } => "timestamp without time zone".into(),
            IRType::Timestamp { with_timezone: true } => "timestamp with time zone".into(),
            IRType::Json => "jsonb".into(),
            IRType::Uuid => "uuid".into(),
            IRType::Bit { length } => format!("bit({length})"),
            IRType::Other(_) => "text".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("now()".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(s) => Some(s.clone()),
        }
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "'\\x{}'",
                    b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
                )
            }
        }
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
    fn pg_varchar_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("name", "character varying");
        let ir = adapter.column_to_ir(&c, Some("character varying(255)"));
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(255) });
    }

    #[test]
    fn pg_numeric_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("price", "numeric");
        let ir = adapter.column_to_ir(&c, Some("numeric(10,2)"));
        assert_eq!(ir.ir_type, IRType::Decimal { precision: 10, scale: 2 });
    }

    #[test]
    fn pg_array_to_json() {
        let adapter = PgSyncAdapter;
        let c = col("tags", "text[]");
        let ir = adapter.column_to_ir(&c, Some("text[]"));
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn pg_bool_to_ir() {
        let adapter = PgSyncAdapter;
        let c = col("active", "boolean");
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn pg_timestamp_tz() {
        let adapter = PgSyncAdapter;
        let c = col("created", "timestamptz");
        let ir = adapter.column_to_ir(&c, Some("timestamp with time zone"));
        assert_eq!(ir.ir_type, IRType::Timestamp { with_timezone: true });
    }

    #[test]
    fn pg_default_nextval_skipped() {
        let adapter = PgSyncAdapter;
        let mut c = col("id", "integer");
        c.default_value = Some("nextval('users_id_seq'::regclass)".into());
        let ir = adapter.column_to_ir(&c, None);
        assert!(ir.default_expr.is_none());
    }

    #[test]
    fn pg_default_now() {
        let adapter = PgSyncAdapter;
        let mut c = col("created", "timestamp");
        c.default_value = Some("now()".into());
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(ir.default_expr, Some(IRDefault::CurrentTimestamp));
    }

    #[test]
    fn pg_default_cast_stripped() {
        let adapter = PgSyncAdapter;
        let mut c = col("status", "text");
        c.default_value = Some("'active'::text".into());
        let ir = adapter.column_to_ir(&c, None);
        assert_eq!(
            ir.default_expr,
            Some(IRDefault::Literal("'active'".into()))
        );
    }

    #[test]
    fn pg_target_roundtrip() {
        let adapter = PgSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "boolean");
        assert_eq!(adapter.ir_type_to_native(&IRType::Int32), "integer");
        assert_eq!(adapter.ir_type_to_native(&IRType::Json), "jsonb");
        assert_eq!(adapter.ir_type_to_native(&IRType::Uuid), "uuid");
        assert_eq!(adapter.ir_type_to_native(&IRType::Blob), "bytea");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Varchar { length: Some(100) }),
            "character varying(100)"
        );
    }
}
