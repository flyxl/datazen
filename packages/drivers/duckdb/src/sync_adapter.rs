//! DuckDB sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct DuckDbSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(DuckDbSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["duckdb"],
        create,
    }
}

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
            let p = parts
                .first()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            let scale = parts
                .get(1)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            return (p, scale);
        }
    }
    (0, 0)
}

fn duckdb_type_to_ir(raw: &str) -> IRType {
    let upper = raw.trim().to_uppercase();
    if upper.starts_with("VARCHAR") || upper.starts_with("CHAR") {
        let len = parse_length(&upper, "VARCHAR").or_else(|| parse_length(&upper, "CHAR"));
        return IRType::Varchar { length: len };
    }
    if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
        let prefix = if upper.starts_with("DECIMAL") {
            "DECIMAL"
        } else {
            "NUMERIC"
        };
        let (p, s) = parse_precision(&upper, prefix);
        return IRType::Decimal {
            precision: p,
            scale: s,
        };
    }
    match upper.as_str() {
        "INTEGER" | "INT" | "INT4" | "SIGNED" => IRType::Int32,
        "BIGINT" | "INT8" | "HUGEINT" => IRType::Int64,
        "SMALLINT" | "INT2" | "TINYINT" | "INT1" => IRType::Int16,
        "DOUBLE" | "FLOAT8" | "FLOAT" => IRType::Float64,
        "REAL" | "FLOAT4" => IRType::Float32,
        "BOOLEAN" | "BOOL" => IRType::Bool,
        "TIMESTAMP" | "TIMESTAMPTZ" | "DATETIME" => IRType::Timestamp {
            with_timezone: upper.contains("TZ"),
        },
        "DATE" => IRType::Date,
        "BLOB" | "BYTEA" | "BINARY" => IRType::Blob,
        "UUID" => IRType::Uuid,
        "JSON" => IRType::Json,
        "TEXT" | "STRING" => IRType::Text,
        _ => IRType::Other(raw.to_string()),
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for DuckDbSyncAdapter {
    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let ir_type = duckdb_type_to_ir(raw);

        let default_expr = column.default_value.as_deref().and_then(|d| {
            let d = d.trim();
            if d.is_empty() {
                return None;
            }
            if d == "CURRENT_TIMESTAMP" || d.eq_ignore_ascii_case("now()") {
                return Some(IRDefault::CurrentTimestamp);
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

impl SyncTargetAdapter for DuckDbSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "BOOLEAN".into(),
            IRType::Int8 | IRType::Int16 => "SMALLINT".into(),
            IRType::Int32 => "INTEGER".into(),
            IRType::Int64 => "BIGINT".into(),
            IRType::Float32 => "REAL".into(),
            IRType::Float64 => "DOUBLE".into(),
            IRType::Decimal { precision: 0, .. } => "DECIMAL".into(),
            IRType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            IRType::Char { length } => format!("VARCHAR({length})"),
            IRType::Varchar { length: Some(n) } => format!("VARCHAR({n})"),
            IRType::Varchar { length: None } | IRType::Text => "VARCHAR".into(),
            IRType::Binary { .. } | IRType::Blob => "BLOB".into(),
            IRType::Date => "DATE".into(),
            IRType::Time { .. } => "VARCHAR".into(),
            IRType::Timestamp {
                with_timezone: true,
            } => "TIMESTAMPTZ".into(),
            IRType::Timestamp {
                with_timezone: false,
            } => "TIMESTAMP".into(),
            IRType::Json => "JSON".into(),
            IRType::Uuid => "UUID".into(),
            IRType::Bit { .. } => "INTEGER".into(),
            IRType::Other(_) => "VARCHAR".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("CURRENT_TIMESTAMP".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(_) => None,
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
            Some(Value::Bytes(b)) => format!(
                "'\\x{}'",
                b.iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<String>()
            ),
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
    fn duckdb_varchar_to_ir() {
        let ir = DuckDbSyncAdapter.column_to_ir(&col("name", "VARCHAR(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn duckdb_integer_and_double() {
        let ir = DuckDbSyncAdapter.column_to_ir(&col("id", "INTEGER"), None);
        assert_eq!(ir.ir_type, IRType::Int32);
        let ir = DuckDbSyncAdapter.column_to_ir(&col("score", "DOUBLE"), None);
        assert_eq!(ir.ir_type, IRType::Float64);
    }

    #[test]
    fn duckdb_json_and_uuid() {
        let ir = DuckDbSyncAdapter.column_to_ir(&col("meta", "JSON"), None);
        assert_eq!(ir.ir_type, IRType::Json);
        let ir = DuckDbSyncAdapter.column_to_ir(&col("uid", "UUID"), None);
        assert_eq!(ir.ir_type, IRType::Uuid);
    }

    #[test]
    fn duckdb_target_roundtrip() {
        let adapter = DuckDbSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "BOOLEAN");
        assert_eq!(adapter.ir_type_to_native(&IRType::Json), "JSON");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Varchar { length: Some(50) }),
            "VARCHAR(50)"
        );
    }

    #[test]
    fn duckdb_format_literal() {
        let adapter = DuckDbSyncAdapter;
        assert_eq!(adapter.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            adapter.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "TRUE"
        );
    }
}
