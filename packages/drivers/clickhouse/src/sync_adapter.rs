//! ClickHouse sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRTable, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct ClickHouseSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(ClickHouseSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["clickhouse"],
        create,
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn unwrap_wrappers(mut s: &str) -> &str {
    loop {
        if let Some(inner) = s.strip_prefix("Nullable(").and_then(|r| r.strip_suffix(')')) {
            s = inner;
            continue;
        }
        if let Some(inner) = s
            .strip_prefix("LowCardinality(")
            .and_then(|r| r.strip_suffix(')'))
        {
            s = inner;
            continue;
        }
        break;
    }
    s
}

fn parse_paren_args(s: &str, prefix: &str) -> Option<(u8, u8)> {
    let rest = s.strip_prefix(prefix)?.trim();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    let p = parts.first()?.trim().parse().ok()?;
    let scale = parts.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    Some((p, scale))
}

fn parse_ch_type(raw: &str) -> (IRType, bool) {
    let trimmed = raw.trim();
    let mut nullable = false;
    let mut s = trimmed;
    if let Some(inner) = s.strip_prefix("Nullable(").and_then(|r| r.strip_suffix(')')) {
        nullable = true;
        s = inner;
    }
    s = unwrap_wrappers(s);

    let lower = s.to_lowercase();
    let ir_type = if lower.starts_with("fixedstring") {
        let len = s
            .strip_prefix("FixedString")
            .or_else(|| s.strip_prefix("fixedstring"))
            .and_then(|r| r.trim().strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(1);
        IRType::Char { length: len }
    } else if lower.starts_with("decimal") {
        let (p, scale) = parse_paren_args(s, "Decimal")
            .or_else(|| parse_paren_args(s, "decimal"))
            .unwrap_or((0, 0));
        IRType::Decimal {
            precision: p,
            scale,
        }
    } else if lower.starts_with("datetime64") {
        IRType::Timestamp { with_timezone: false }
    } else if lower.starts_with("array(") || lower == "array" {
        IRType::Json
    } else if lower.starts_with("map(") || lower == "map" {
        IRType::Json
    } else if lower.starts_with("tuple(") || lower == "tuple" {
        IRType::Json
    } else {
        match lower.as_str() {
            "uint8" => IRType::Int8,
            "uint16" => IRType::Int16,
            "uint32" => IRType::Int32,
            "uint64" => IRType::Int64,
            "int8" => IRType::Int8,
            "int16" => IRType::Int16,
            "int32" => IRType::Int32,
            "int64" => IRType::Int64,
            "float32" => IRType::Float32,
            "float64" => IRType::Float64,
            "string" => IRType::Text,
            "date" => IRType::Date,
            "datetime" => IRType::Timestamp { with_timezone: false },
            "uuid" => IRType::Uuid,
            "bool" | "boolean" => IRType::Bool,
            _ => IRType::Other(s.to_string()),
        }
    };

    (ir_type, nullable)
}

fn order_by_clause(ir_table: &IRTable) -> String {
    let q = |name: &str| ClickHouseSyncAdapter.quote_ident(name);
    if !ir_table.primary_keys.is_empty() {
        ir_table
            .primary_keys
            .iter()
            .map(|k| q(k))
            .collect::<Vec<_>>()
            .join(", ")
    } else if let Some(first) = ir_table.columns.first() {
        q(&first.name)
    } else {
        "tuple()".into()
    }
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for ClickHouseSyncAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn {
        let raw = native_full_type.unwrap_or(&column.data_type);
        let (ir_type, type_nullable) = parse_ch_type(raw);

        IRColumn {
            name: column.name.clone(),
            ir_type,
            nullable: column.nullable || type_nullable,
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

    fn table_options_query(&self, table: &str) -> Option<String> {
        let escaped = table.replace('\'', "''");
        Some(format!(
            "SELECT if(empty(engine_full), \
               concat('ENGINE = ', engine, if(empty(sorting_key), '', concat('\\nORDER BY (', sorting_key, ')'))), \
               concat('ENGINE = ', engine_full)) \
             FROM system.tables \
             WHERE database = currentDatabase() AND name = '{escaped}' AND is_temporary = 0 \
             LIMIT 1"
        ))
    }
}

// ── SyncTargetAdapter ──────────────────────────────────────────────

impl SyncTargetAdapter for ClickHouseSyncAdapter {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        match ir_type {
            IRType::Bool => "Bool".into(),
            IRType::Int8 => "Int8".into(),
            IRType::Int16 => "Int16".into(),
            IRType::Int32 => "Int32".into(),
            IRType::Int64 => "Int64".into(),
            IRType::Float32 => "Float32".into(),
            IRType::Float64 => "Float64".into(),
            IRType::Decimal { precision: 0, .. } => "Decimal(18, 4)".into(),
            IRType::Decimal { precision, scale } => format!("Decimal({precision},{scale})"),
            IRType::Char { length } => format!("FixedString({length})"),
            IRType::Varchar { length: Some(n) } => format!("FixedString({n})"),
            IRType::Varchar { length: None } | IRType::Text => "String".into(),
            IRType::Binary { .. } | IRType::Blob => "String".into(),
            IRType::Date => "Date".into(),
            IRType::Time { .. } => "String".into(),
            IRType::Timestamp { .. } => "DateTime64(3)".into(),
            IRType::Json => "String".into(),
            IRType::Uuid => "UUID".into(),
            IRType::Bit { .. } => "UInt8".into(),
            IRType::Other(_) => "String".into(),
        }
    }

    fn format_default(&self, default: &IRDefault) -> Option<String> {
        match default {
            IRDefault::CurrentTimestamp => Some("now64()".into()),
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
            Some(Value::Bytes(b)) => format!(
                "'{}'",
                b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()
            ),
        }
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn supports_primary_key(&self) -> bool {
        false
    }

    fn create_table_suffix(&self, ir_table: &IRTable) -> Option<String> {
        if let Some(opts) = ir_table.table_options.clone() {
            return Some(opts);
        }
        let order = order_by_clause(ir_table);
        Some(format!("ENGINE = MergeTree\nORDER BY ({order})"))
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
    fn ch_uint_and_int_types() {
        let adapter = ClickHouseSyncAdapter;
        assert_eq!(
            adapter.column_to_ir(&col("a", "UInt32"), None).ir_type,
            IRType::Int32
        );
        assert_eq!(
            adapter.column_to_ir(&col("b", "Int64"), None).ir_type,
            IRType::Int64
        );
    }

    #[test]
    fn ch_nullable_unwraps_inner_type() {
        let adapter = ClickHouseSyncAdapter;
        let ir = adapter.column_to_ir(&col("name", "Nullable(String)"), None);
        assert_eq!(ir.ir_type, IRType::Text);
        assert!(ir.nullable);
    }

    #[test]
    fn ch_array_maps_to_json() {
        let adapter = ClickHouseSyncAdapter;
        let ir = adapter.column_to_ir(&col("tags", "Array(String)"), None);
        assert_eq!(ir.ir_type, IRType::Json);
    }

    #[test]
    fn ch_target_roundtrip_types() {
        let adapter = ClickHouseSyncAdapter;
        assert_eq!(adapter.ir_type_to_native(&IRType::Bool), "Bool");
        assert_eq!(adapter.ir_type_to_native(&IRType::Uuid), "UUID");
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Char { length: 16 }),
            "FixedString(16)"
        );
    }

    #[test]
    fn ch_create_table_suffix_default_mergetree() {
        let adapter = ClickHouseSyncAdapter;
        let table = IRTable {
            name: "events".into(),
            columns: vec![IRColumn {
                name: "ts".into(),
                ir_type: IRType::Timestamp { with_timezone: false },
                nullable: false,
                default_expr: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: None,
        };
        let suffix = adapter.create_table_suffix(&table).unwrap();
        assert!(suffix.contains("ENGINE = MergeTree"));
        assert!(suffix.contains("ORDER BY (`ts`)"));
    }

    #[test]
    fn ch_create_table_suffix_uses_table_options() {
        let adapter = ClickHouseSyncAdapter;
        let table = IRTable {
            name: "t".into(),
            columns: vec![],
            primary_keys: vec![],
            table_options: Some("ENGINE = Log".into()),
        };
        assert_eq!(
            adapter.create_table_suffix(&table),
            Some("ENGINE = Log".into())
        );
    }

    #[test]
    fn ch_format_literal() {
        let adapter = ClickHouseSyncAdapter;
        assert_eq!(adapter.format_literal(&None, &IRType::Text), "NULL");
        assert_eq!(
            adapter.format_literal(&Some(Value::String("a'b".into())), &IRType::Text),
            "'a''b'"
        );
    }
}
