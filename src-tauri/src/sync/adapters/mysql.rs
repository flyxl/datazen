//! MySQL / MariaDB sync adapter.

use crate::db::{ColumnSchema, Value};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ir::{IRColumn, IRDefault, IRType};

pub struct MysqlSyncAdapter {
    pub is_mariadb: bool,
}

// ── helpers ────────────────────────────────────────────────────────

fn strip_modifiers(s: &str) -> String {
    s.to_lowercase()
        .replace(" unsigned", "")
        .replace(" zerofill", "")
}

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

fn parse_mysql_default(raw: &str) -> Option<IRDefault> {
    let d = raw.trim();
    if d.is_empty() {
        return None;
    }
    if d == "CURRENT_TIMESTAMP" || d == "current_timestamp()" {
        return Some(IRDefault::CurrentTimestamp);
    }
    Some(IRDefault::Literal(d.to_string()))
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for MysqlSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        _native_full_type: Option<&str>,
    ) -> IRColumn {
        let base = strip_modifiers(&column.data_type);

        let ir_type = if base.starts_with("tinyint(1)") {
            IRType::Bool
        } else if base.starts_with("tinyint") {
            IRType::Int8
        } else if base.starts_with("smallint") {
            IRType::Int16
        } else if base.starts_with("mediumint") {
            IRType::Int32
        } else if base.starts_with("bigint") {
            IRType::Int64
        } else if base.starts_with("int(") || base == "int" || base == "integer" {
            IRType::Int32
        } else if base.starts_with("varchar") {
            let len = parse_length(&base, "varchar");
            IRType::Varchar { length: len }
        } else if base.starts_with("char(") {
            let len = parse_length(&base, "char").unwrap_or(1);
            IRType::Char { length: len }
        } else if base.starts_with("decimal") {
            let (p, s) = parse_precision(&base, "decimal");
            IRType::Decimal { precision: p, scale: s }
        } else if base.starts_with("enum(") || base.starts_with("set(") || base == "enum" || base == "set" {
            IRType::Text
        } else if base.starts_with("varbinary") {
            let len = parse_length(&base, "varbinary");
            IRType::Binary { length: len }
        } else if base.starts_with("binary") {
            let len = parse_length(&base, "binary");
            IRType::Binary { length: len }
        } else if base.starts_with("bit(") || base == "bit" {
            if base == "bit(1)" || base == "bit" {
                IRType::Bool
            } else {
                let len = parse_length(&base, "bit").unwrap_or(1);
                IRType::Bit { length: len }
            }
        } else {
            match base.as_str() {
                "float" => IRType::Float32,
                "double" => IRType::Float64,
                "datetime" | "timestamp" => IRType::Timestamp { with_timezone: false },
                "date" => IRType::Date,
                "time" => IRType::Time { with_timezone: false },
                "year" => IRType::Int16,
                "text" | "longtext" | "mediumtext" | "tinytext" => IRType::Text,
                "blob" | "longblob" | "mediumblob" | "tinyblob" => IRType::Blob,
                "json" => IRType::Json,
                _ => IRType::Other(column.data_type.clone()),
            }
        };

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable,
            default_expr: column
                .default_value
                .as_deref()
                .and_then(parse_mysql_default),
            is_primary_key: column.is_primary_key,
            is_auto_increment: column.is_auto_increment,
            comment: column.comment.clone(),
        }
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for MysqlSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "TINYINT(1)".into(),
            IRType::Int8 => "TINYINT".into(),
            IRType::Int16 => "SMALLINT".into(),
            IRType::Int32 => "INT".into(),
            IRType::Int64 => "BIGINT".into(),
            IRType::Float32 => "FLOAT".into(),
            IRType::Float64 => "DOUBLE".into(),
            IRType::Decimal { precision: 0, .. } => "DECIMAL(65,30)".into(),
            IRType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            IRType::Char { length } => format!("CHAR({length})"),
            IRType::Varchar { length: Some(n) } => format!("VARCHAR({n})"),
            IRType::Varchar { length: None } => "VARCHAR(255)".into(),
            IRType::Text => "TEXT".into(),
            IRType::Binary { length: Some(n) } => format!("VARBINARY({n})"),
            IRType::Binary { length: None } | IRType::Blob => "LONGBLOB".into(),
            IRType::Date => "DATE".into(),
            IRType::Time { .. } => "TIME".into(),
            IRType::Timestamp { .. } => "DATETIME".into(),
            IRType::Json => "JSON".into(),
            IRType::Uuid => "CHAR(36)".into(),
            IRType::Bit { length } => format!("BIT({length})"),
            IRType::Other(_) => "TEXT".into(),
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
            Some(Value::Bool(b)) => if *b { "1" } else { "0" }.into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!(
                    "X'{}'",
                    b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
                )
            }
        }
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("AUTO_INCREMENT")
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

    fn adapter() -> MysqlSyncAdapter {
        MysqlSyncAdapter { is_mariadb: false }
    }

    #[test]
    fn mysql_tinyint1_is_bool() {
        let ir = adapter().column_to_ir(&col("active", "tinyint(1)"), None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn mysql_varchar_to_ir() {
        let ir = adapter().column_to_ir(&col("name", "varchar(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn mysql_unsigned_int() {
        let ir = adapter().column_to_ir(&col("age", "int unsigned"), None);
        assert_eq!(ir.ir_type, IRType::Int32);
    }

    #[test]
    fn mysql_enum_to_text() {
        let ir = adapter().column_to_ir(&col("status", "enum('a','b','c')"), None);
        assert_eq!(ir.ir_type, IRType::Text);
    }

    #[test]
    fn mysql_json() {
        let ir = adapter().column_to_ir(&col("data", "json"), None);
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn mysql_target_types() {
        let a = adapter();
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "TINYINT(1)");
        assert_eq!(a.ir_type_to_native(&IRType::Int32), "INT");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "CHAR(36)");
        assert_eq!(a.ir_type_to_native(&IRType::Json), "JSON");
        assert_eq!(a.ir_type_to_native(&IRType::Blob), "LONGBLOB");
        assert_eq!(
            a.ir_type_to_native(&IRType::Varchar { length: Some(255) }),
            "VARCHAR(255)"
        );
    }

    #[test]
    fn mysql_format_bool_literal() {
        let a = adapter();
        assert_eq!(
            a.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "1"
        );
        assert_eq!(
            a.format_literal(&Some(Value::Bool(false)), &IRType::Bool),
            "0"
        );
    }
}
