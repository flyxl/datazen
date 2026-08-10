//! SQL Server sync adapter.

use crate::db::{ColumnSchema, Value};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::adapter_registry::{SyncAdapterFactory, SyncAdapterRegistry};
use crate::sync::ir::{IRColumn, IRDefault, IRType};
use std::sync::Arc;

pub struct SqlServerSyncAdapter;

fn register(registry: &SyncAdapterRegistry, db_type: crate::db::DatabaseType) {
    registry.register_both(db_type, Arc::new(SqlServerSyncAdapter));
}

inventory::submit! {
    SyncAdapterFactory {
        db_types: &["sqlserver"],
        register,
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
            let p = parts.first().and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            let s = parts.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            return (p, s);
        }
    }
    (0, 0)
}

fn parse_sqlserver_default(raw: &str) -> Option<IRDefault> {
    let d = raw.trim();
    if d.is_empty() {
        return None;
    }
    // Strip wrapping parentheses SQL Server often emits: ((0)), ('x')
    let mut unwrapped = d;
    while unwrapped.starts_with('(') && unwrapped.ends_with(')') && unwrapped.len() > 2 {
        unwrapped = &unwrapped[1..unwrapped.len() - 1];
    }
    let u = unwrapped.trim();
    if u.eq_ignore_ascii_case("getdate()")
        || u.eq_ignore_ascii_case("sysdatetime()")
        || u.eq_ignore_ascii_case("current_timestamp")
    {
        return Some(IRDefault::CurrentTimestamp);
    }
    Some(IRDefault::Literal(u.to_string()))
}

fn base_type(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    // Drop length/precision suffix for match, keep full string for parsers.
    lower
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for SqlServerSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let lower = base_type(raw);

        let ir_type = if lower.starts_with("nvarchar") {
            let len = parse_length(&lower, "nvarchar");
            IRType::Varchar { length: len }
        } else if lower.starts_with("varchar") {
            let len = parse_length(&lower, "varchar");
            IRType::Varchar { length: len }
        } else if lower.starts_with("nchar") {
            let len = parse_length(&lower, "nchar").unwrap_or(1);
            IRType::Char { length: len }
        } else if lower.starts_with("char(") || lower == "char" {
            let len = parse_length(&lower, "char").unwrap_or(1);
            IRType::Char { length: len }
        } else if lower.starts_with("decimal") {
            let (p, s) = parse_precision(&lower, "decimal");
            IRType::Decimal { precision: p, scale: s }
        } else if lower.starts_with("numeric") {
            let (p, s) = parse_precision(&lower, "numeric");
            IRType::Decimal { precision: p, scale: s }
        } else if lower.starts_with("varbinary") {
            let len = parse_length(&lower, "varbinary");
            IRType::Binary { length: len }
        } else if lower.starts_with("binary") {
            let len = parse_length(&lower, "binary");
            IRType::Binary { length: len }
        } else if lower == "bit" || lower.starts_with("bit(") {
            IRType::Bool
        } else if lower == "tinyint" {
            IRType::Int8
        } else if lower == "smallint" {
            IRType::Int16
        } else if lower == "int" || lower == "integer" {
            IRType::Int32
        } else if lower == "bigint" {
            IRType::Int64
        } else if lower == "real" {
            IRType::Float32
        } else if lower == "float" || lower.starts_with("float(") {
            IRType::Float64
        } else if lower == "text" || lower == "ntext" {
            IRType::Text
        } else if lower == "image" {
            IRType::Blob
        } else if lower == "date" {
            IRType::Date
        } else if lower == "time" || lower.starts_with("time(") {
            IRType::Time { with_timezone: false }
        } else if lower == "datetime"
            || lower == "datetime2"
            || lower.starts_with("datetime2(")
            || lower == "smalldatetime"
        {
            IRType::Timestamp { with_timezone: false }
        } else if lower == "uniqueidentifier" {
            IRType::Uuid
        } else if lower == "xml" {
            IRType::Text
        } else {
            IRType::Other(raw.to_string())
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column
                .default_value
                .as_deref()
                .and_then(parse_sqlserver_default),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for SqlServerSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "BIT".into(),
            IRType::Int8 => "TINYINT".into(),
            IRType::Int16 => "SMALLINT".into(),
            IRType::Int32 => "INT".into(),
            IRType::Int64 => "BIGINT".into(),
            IRType::Float32 => "REAL".into(),
            IRType::Float64 => "FLOAT".into(),
            IRType::Decimal { precision: 0, .. } => "DECIMAL(38,18)".into(),
            IRType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            IRType::Char { length } => format!("NCHAR({length})"),
            IRType::Varchar { length: Some(n) } => format!("NVARCHAR({n})"),
            IRType::Varchar { length: None } | IRType::Text => "NVARCHAR(MAX)".into(),
            IRType::Binary { length: Some(n) } => format!("VARBINARY({n})"),
            IRType::Binary { length: None } | IRType::Blob => "VARBINARY(MAX)".into(),
            IRType::Date => "DATE".into(),
            IRType::Time { .. } => "TIME".into(),
            IRType::Timestamp { .. } => "DATETIME2".into(),
            IRType::Json => "NVARCHAR(MAX)".into(),
            IRType::Uuid => "UNIQUEIDENTIFIER".into(),
            IRType::Bit { .. } => "BIT".into(),
            IRType::Other(_) => "NVARCHAR(MAX)".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("SYSDATETIME()".into()),
            IRDefault::Literal(s) => Some(s.clone()),
            IRDefault::RawExpression(s) => Some(s.clone()),
        }
    }

    fn format_literal(&self, value: &Option<Value>, _ir_type: &IRType) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            // SQL Server BIT prefers 0/1
            Some(Value::Bool(b)) => if *b { "1" } else { "0" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("N'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{s}'"),
            Some(Value::Json(j)) => format!("N'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "0x{}",
                    b.iter().map(|byte| format!("{byte:02X}")).collect::<String>()
                )
            }
        }
    }

    fn quote_char(&self) -> char {
        '['
    }

    fn quote_ident(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }

    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("IDENTITY(1,1)")
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
    fn sqlserver_bit_to_bool() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("active", "bit"), None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn sqlserver_int_types() {
        let a = SqlServerSyncAdapter;
        assert_eq!(a.column_to_ir(&col("a", "tinyint"), None).ir_type, IRType::Int8);
        assert_eq!(a.column_to_ir(&col("a", "smallint"), None).ir_type, IRType::Int16);
        assert_eq!(a.column_to_ir(&col("a", "int"), None).ir_type, IRType::Int32);
        assert_eq!(a.column_to_ir(&col("a", "bigint"), None).ir_type, IRType::Int64);
    }

    #[test]
    fn sqlserver_nvarchar_to_ir() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("name", "nvarchar(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn sqlserver_decimal_to_ir() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("price", "decimal(10,2)"), None);
        assert_eq!(ir.ir_type, IRType::Decimal { precision: 10, scale: 2 });
    }

    #[test]
    fn sqlserver_datetime_to_timestamp() {
        let a = SqlServerSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("t", "datetime2"), None).ir_type,
            IRType::Timestamp { with_timezone: false }
        );
        assert_eq!(
            a.column_to_ir(&col("t", "uniqueidentifier"), None).ir_type,
            IRType::Uuid
        );
        assert_eq!(a.column_to_ir(&col("x", "xml"), None).ir_type, IRType::Text);
    }

    #[test]
    fn sqlserver_quote_ident_escapes_brackets() {
        let q = SqlServerSyncAdapter.quote_ident("col]name");
        assert_eq!(q, "[col]]name]");
    }

    #[test]
    fn sqlserver_format_bool_literal() {
        let a = SqlServerSyncAdapter;
        assert_eq!(a.format_literal(&Some(Value::Bool(true)), &IRType::Bool), "1");
        assert_eq!(a.format_literal(&Some(Value::Bool(false)), &IRType::Bool), "0");
    }

    #[test]
    fn sqlserver_target_types() {
        let a = SqlServerSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "BIT");
        assert_eq!(a.ir_type_to_native(&IRType::Int32), "INT");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "UNIQUEIDENTIFIER");
        assert_eq!(a.auto_increment_keyword(), Some("IDENTITY(1,1)"));
    }
}
