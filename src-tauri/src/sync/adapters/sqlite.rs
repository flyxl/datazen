//! SQLite sync adapter.

use crate::db::{ColumnSchema, Value};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ir::{IRColumn, IRDefault, IRType};

pub struct SqliteSyncAdapter;

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for SqliteSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        _native_full_type: Option<&str>,
    ) -> IRColumn {
        let upper = column.data_type.trim().to_uppercase();

        // SQLite type affinity rules (https://www.sqlite.org/datatype3.html)
        let ir_type = if upper.contains("INT") {
            IRType::Int64
        } else if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
            IRType::Text
        } else if upper.contains("BLOB") || upper.is_empty() {
            IRType::Blob
        } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
            IRType::Float64
        } else if upper.contains("BOOL") {
            IRType::Bool
        } else if upper.contains("DATE") || upper.contains("TIME") {
            IRType::Text
        } else if upper.contains("DECIMAL") || upper.contains("NUMERIC") {
            IRType::Decimal { precision: 0, scale: 0 }
        } else {
            // SQLite NUMERIC affinity as fallback
            IRType::Text
        };

        let default_expr = column.default_value.as_deref().and_then(|d| {
            let d = d.trim();
            if d.is_empty() {
                return None;
            }
            if d == "CURRENT_TIMESTAMP" || d == "current_timestamp" {
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

impl SyncTargetAdapter for SqliteSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "INTEGER".into(),
            IRType::Int8 | IRType::Int16 | IRType::Int32 | IRType::Int64 => "INTEGER".into(),
            IRType::Float32 | IRType::Float64 => "REAL".into(),
            IRType::Decimal { .. } => "REAL".into(),
            IRType::Char { .. }
            | IRType::Varchar { .. }
            | IRType::Text
            | IRType::Date
            | IRType::Time { .. }
            | IRType::Timestamp { .. }
            | IRType::Json
            | IRType::Uuid => "TEXT".into(),
            IRType::Binary { .. } | IRType::Blob => "BLOB".into(),
            IRType::Bit { .. } => "INTEGER".into(),
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

    fn auto_increment_keyword(&self) -> Option<&str> {
        Some("AUTOINCREMENT")
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
    fn sqlite_integer_affinity() {
        let ir = SqliteSyncAdapter.column_to_ir(&col("id", "INTEGER"), None);
        assert_eq!(ir.ir_type, IRType::Int64);
    }

    #[test]
    fn sqlite_text_affinity() {
        let ir = SqliteSyncAdapter.column_to_ir(&col("name", "TEXT"), None);
        assert_eq!(ir.ir_type, IRType::Text);
    }

    #[test]
    fn sqlite_blob_affinity() {
        let ir = SqliteSyncAdapter.column_to_ir(&col("data", "BLOB"), None);
        assert_eq!(ir.ir_type, IRType::Blob);
    }

    #[test]
    fn sqlite_target_maps_to_five_types() {
        let a = SqliteSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Int32), "INTEGER");
        assert_eq!(a.ir_type_to_native(&IRType::Float64), "REAL");
        assert_eq!(a.ir_type_to_native(&IRType::Text), "TEXT");
        assert_eq!(a.ir_type_to_native(&IRType::Blob), "BLOB");
        assert_eq!(a.ir_type_to_native(&IRType::Json), "TEXT");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "TEXT");
    }
}
