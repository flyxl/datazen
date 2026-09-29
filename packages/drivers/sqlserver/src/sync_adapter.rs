//! SQL Server sync adapter.

use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRType, SyncAdapterFactory,
    SyncSourceAdapter, SyncTargetAdapter, Value,
};

pub struct SqlServerSyncAdapter;

fn create() -> BoxedSyncAdapter {
    BoxedSyncAdapter::both(SqlServerSyncAdapter)
}

datazen_driver_api::inventory::submit! {
    SyncAdapterFactory {
        db_types: &["sqlserver"],
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
            let s = parts
                .get(1)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
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
    let unprefixed_string = u.strip_prefix("N'").or_else(|| u.strip_prefix("n'"));
    let hex_literal = u
        .strip_prefix("0x")
        .or_else(|| u.strip_prefix("0X"))
        .is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_hexdigit())
        });
    let literal = u.parse::<i128>().is_ok()
        || u.parse::<f64>().is_ok()
        || matches!(u.to_ascii_lowercase().as_str(), "true" | "false" | "null")
        || hex_literal
        || (u.starts_with('\'') && u.ends_with('\''))
        || unprefixed_string.is_some_and(|value| value.ends_with('\''));
    if literal {
        return Some(IRDefault::Literal(
            unprefixed_string
                .map(|value| format!("'{value}"))
                .unwrap_or_else(|| u.to_string()),
        ));
    }
    Some(IRDefault::RawExpression(u.to_string()))
}

fn base_type(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    // Drop length/precision suffix for match, keep full string for parsers.
    lower
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for SqlServerSyncAdapter {
    /// Rebuild declared type dimensions that INFORMATION_SCHEMA omits, so a
    /// transfer preserves bounded strings and decimal precision/scale.
    fn full_column_types_query(&self, table: &str) -> Option<String> {
        let escaped = table.replace('\'', "''");
        Some(format!(
            "SELECT c.name AS col_name, \
                    t.name + CASE \
                      WHEN t.name IN ('nvarchar','nchar') THEN \
                        '(' + CASE WHEN c.max_length = -1 THEN 'max' \
                                   ELSE CAST(c.max_length / 2 AS varchar(10)) END + ')' \
                      WHEN t.name IN ('varchar','char','varbinary','binary') THEN \
                        '(' + CASE WHEN c.max_length = -1 THEN 'max' \
                                   ELSE CAST(c.max_length AS varchar(10)) END + ')' \
                      WHEN t.name IN ('decimal','numeric') THEN \
                        '(' + CAST(c.precision AS varchar(10)) + ',' + CAST(c.scale AS varchar(10)) + ')' \
                      WHEN t.name IN ('datetime2','datetimeoffset','time') THEN \
                        '(' + CAST(c.scale AS varchar(10)) + ')' \
                      ELSE '' END AS full_type \
             FROM sys.columns c \
             JOIN sys.types t ON t.user_type_id = c.user_type_id \
             WHERE c.object_id = OBJECT_ID('{escaped}') \
             ORDER BY c.column_id"
        ))
    }

    fn unsupported_transfer_structure_query(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        // The current transfer plan preserves columns and PKs only, so refuse
        // table-level objects that would otherwise disappear from the export.
        // Keep catalog qualifiers and user names safely quoted as identifiers
        // or literals, including `]` and apostrophes.
        let catalog = if database.trim().is_empty() {
            String::new()
        } else {
            format!("[{}].", database.replace(']', "]]"))
        };
        let schema = schema.unwrap_or("dbo").replace('\'', "''");
        let table = table.replace('\'', "''");
        Some(format!(
            "SELECT CONCAT('computed column ', c.name) AS unsupported_object \
             FROM {catalog}sys.computed_columns c \
             JOIN {catalog}sys.tables t ON t.object_id = c.object_id \
             JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
             WHERE s.name = N'{schema}' AND t.name = N'{table}' \
             UNION ALL \
             SELECT CONCAT('secondary index ', i.name) \
             FROM {catalog}sys.indexes i \
             JOIN {catalog}sys.tables t ON t.object_id = i.object_id \
             JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
             WHERE s.name = N'{schema}' AND t.name = N'{table}' \
               AND i.index_id > 0 AND i.is_primary_key = 0 \
             UNION ALL \
             SELECT CONCAT('foreign key ', fk.name) \
             FROM {catalog}sys.foreign_keys fk \
             JOIN {catalog}sys.tables t ON t.object_id = fk.parent_object_id \
             JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
             WHERE s.name = N'{schema}' AND t.name = N'{table}' \
             UNION ALL \
             SELECT CONCAT('CHECK constraint ', cc.name) \
             FROM {catalog}sys.check_constraints cc \
             JOIN {catalog}sys.tables t ON t.object_id = cc.parent_object_id \
             JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
             WHERE s.name = N'{schema}' AND t.name = N'{table}' \
             UNION ALL \
             SELECT CONCAT('identity column ', ic.name, ' (non-default seed/increment)') \
             FROM {catalog}sys.identity_columns ic \
             JOIN {catalog}sys.tables t ON t.object_id = ic.object_id \
             JOIN {catalog}sys.schemas s ON s.schema_id = t.schema_id \
             WHERE s.name = N'{schema}' AND t.name = N'{table}' \
               AND (TRY_CONVERT(decimal(38,0), ic.seed_value) <> 1 \
                    OR TRY_CONVERT(decimal(38,0), ic.increment_value) <> 1)"
        ))
    }

    fn column_to_ir(&self, column: &ColumnSchema, native_full_type: Option<&str>) -> IRColumn {
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
            IRType::Decimal {
                precision: p,
                scale: s,
            }
        } else if lower.starts_with("numeric") {
            let (p, s) = parse_precision(&lower, "numeric");
            IRType::Decimal {
                precision: p,
                scale: s,
            }
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
            IRType::Time {
                with_timezone: false,
            }
        } else if lower == "datetime"
            || lower == "datetime2"
            || lower.starts_with("datetime2(")
            || lower == "smalldatetime"
        {
            IRType::Timestamp {
                with_timezone: false,
            }
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
                    b.iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<String>()
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

    fn qualify_relation(&self, database: &str, schema: Option<&str>, table: &str) -> String {
        let quoted_table = self.quote_ident(table);
        let quoted_schema = schema
            .filter(|value| !value.trim().is_empty())
            .map(|value| self.quote_ident(value));
        let quoted_database = (!database.trim().is_empty()).then(|| self.quote_ident(database));
        match (quoted_database, quoted_schema) {
            (Some(database), Some(schema)) => format!("{database}.{schema}.{quoted_table}"),
            // SQL Server parses a two-part relation as schema.object, so a
            // database-only scope must still include the driver's effective
            // default schema. Data Transfer resolves this to dbo unless the
            // endpoint/config supplies another schema.
            (Some(database), None) => format!("{database}.[dbo].{quoted_table}"),
            (None, Some(schema)) => format!("{schema}.{quoted_table}"),
            (None, None) => quoted_table,
        }
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
    fn full_column_types_query_preserves_declared_dimensions_and_escapes_names() {
        let sql = SqlServerSyncAdapter
            .full_column_types_query("dbo.users")
            .expect("SQL Server reports complete column types");
        assert!(sql.contains("sys.columns"));
        assert!(sql.contains("c.max_length / 2"));
        assert!(sql.contains("c.precision"));

        let quoted = SqlServerSyncAdapter
            .full_column_types_query("dbo.o'brien")
            .expect("table name query");
        assert!(quoted.contains("OBJECT_ID('dbo.o''brien')"));
    }

    #[test]
    fn sqlserver_bit_to_bool() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("active", "bit"), None);
        assert_eq!(ir.ir_type, IRType::Bool);
    }

    #[test]
    fn sqlserver_int_types() {
        let a = SqlServerSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("a", "tinyint"), None).ir_type,
            IRType::Int8
        );
        assert_eq!(
            a.column_to_ir(&col("a", "smallint"), None).ir_type,
            IRType::Int16
        );
        assert_eq!(
            a.column_to_ir(&col("a", "int"), None).ir_type,
            IRType::Int32
        );
        assert_eq!(
            a.column_to_ir(&col("a", "bigint"), None).ir_type,
            IRType::Int64
        );
    }

    #[test]
    fn sqlserver_nvarchar_to_ir() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("name", "nvarchar(100)"), None);
        assert_eq!(ir.ir_type, IRType::Varchar { length: Some(100) });
    }

    #[test]
    fn sqlserver_decimal_to_ir() {
        let ir = SqlServerSyncAdapter.column_to_ir(&col("price", "decimal(10,2)"), None);
        assert_eq!(
            ir.ir_type,
            IRType::Decimal {
                precision: 10,
                scale: 2
            }
        );
    }

    #[test]
    fn sqlserver_datetime_to_timestamp() {
        let a = SqlServerSyncAdapter;
        assert_eq!(
            a.column_to_ir(&col("t", "datetime2"), None).ir_type,
            IRType::Timestamp {
                with_timezone: false
            }
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
        assert_eq!(
            a.format_literal(&Some(Value::Bool(true)), &IRType::Bool),
            "1"
        );
        assert_eq!(
            a.format_literal(&Some(Value::Bool(false)), &IRType::Bool),
            "0"
        );
    }

    #[test]
    fn sqlserver_target_types() {
        let a = SqlServerSyncAdapter;
        assert_eq!(a.ir_type_to_native(&IRType::Bool), "BIT");
        assert_eq!(a.ir_type_to_native(&IRType::Int32), "INT");
        assert_eq!(a.ir_type_to_native(&IRType::Uuid), "UNIQUEIDENTIFIER");
        assert_eq!(a.auto_increment_keyword(), Some("IDENTITY(1,1)"));
    }

    #[test]
    fn sqlserver_transfer_relation_keeps_database_and_schema_scope() {
        assert_eq!(
            SqlServerSyncAdapter.qualify_relation("archive", Some("dbo"), "users"),
            "[archive].[dbo].[users]"
        );
        assert_eq!(
            SqlServerSyncAdapter.qualify_relation("archive", None, "users"),
            "[archive].[dbo].[users]"
        );
        assert_eq!(
            SqlServerSyncAdapter.qualify_relation("", Some("sales"), "users"),
            "[sales].[users]"
        );
    }

    #[test]
    fn sqlserver_source_preflight_rejects_unmodeled_structure_objects() {
        let query = SqlServerSyncAdapter
            .unsupported_transfer_structure_query("db]name", Some("sales' data"), "people's")
            .expect("SQL Server source preflight query");
        assert!(query.contains("[db]]name].sys.computed_columns"));
        assert!(query.contains("s.name = N'sales'' data'"));
        assert!(query.contains("t.name = N'people''s'"));
        assert!(query.contains("secondary index "));
        assert!(query.contains("foreign key "));
        assert!(query.contains("CHECK constraint "));
        assert!(query.contains("non-default seed/increment"));
    }

    #[test]
    fn sqlserver_default_parser_fails_closed_for_expressions() {
        assert_eq!(
            parse_sqlserver_default("((N'hello'))"),
            Some(IRDefault::Literal("'hello'".into()))
        );
        assert!(matches!(
            parse_sqlserver_default("(NEXT VALUE FOR dbo.sequence)"),
            Some(IRDefault::RawExpression(_))
        ));
    }
}
