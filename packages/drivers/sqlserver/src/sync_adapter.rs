//! SQL Server sync adapter.

use datazen_driver_api::sync::contract_from_column;
use datazen_driver_api::{
    BoxedSyncAdapter, ColumnSchema, IRColumn, IRDefault, IRForeignKey, IRIndex, IRTableObjects,
    IRType, SyncAdapterFactory, SyncKeyContract, SyncKeyKind, SyncSourceAdapter, SyncTargetAdapter,
    TableSchema, Value,
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

/// Map SQL Server's catalog index-type vocabulary onto the dialect-neutral
/// index model used by the transfer IR.
///
/// `CLUSTERED` / `NONCLUSTERED` — and the `UNIQUE_CONSTRAINT:` prefix the
/// catalog parser adds for constraint-backed unique indexes — describe
/// physical layout and constraint backing rather than a different index kind:
/// an ordinary `CREATE [UNIQUE] INDEX` reproduces the indexed columns and
/// uniqueness, which is exactly what the IR can carry. Anything else (gin, rum,
/// hash, …) is returned untouched so the shared renderer keeps rejecting index
/// methods it cannot express.
fn portable_index_type(raw: &str) -> String {
    match raw.trim().to_ascii_uppercase().as_str() {
        ""
        | "CLUSTERED"
        | "NONCLUSTERED"
        | "UNIQUE_CONSTRAINT:CLUSTERED"
        | "UNIQUE_CONSTRAINT:NONCLUSTERED" => String::new(),
        _ => raw.to_string(),
    }
}

fn native_type_name(raw: &str) -> &str {
    raw.trim()
        .split(|character: char| character == '(' || character.is_whitespace())
        .next()
        .unwrap_or_default()
}

fn is_safe_native_only_type(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    match native_type_name(&normalized) {
        "tinyint" | "xml" => normalized == native_type_name(&normalized),
        "binary" => {
            parse_length(&normalized, "binary").is_some_and(|length| (1..=8_000).contains(&length))
        }
        _ => false,
    }
}

fn normalize_native_type(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

// ── SyncSourceAdapter ──────────────────────────────────────────────

impl SyncSourceAdapter for SqlServerSyncAdapter {
    fn sync_key_contract(&self, column: &ColumnSchema) -> Result<SyncKeyContract, String> {
        let lower = column.data_type.trim().to_ascii_lowercase();
        let base = lower.split(['(', ' ', ',']).next().unwrap_or_default();
        match base {
            // SQL Server string comparisons can ignore trailing spaces and
            // depend on a database collation. The shared bytewise contract
            // cannot represent those semantics, so refuse string keys until a
            // driver-owned ordering/equality contract can prove parity.
            "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext" => Err(format!(
                "SQL Server text key type '{}' has collation and trailing-space semantics that Data Sync cannot verify safely",
                column.data_type
            )),
            // SQL Server's UNIQUEIDENTIFIER ordering differs from the generic
            // lexical UUID order, so it must not enter the merge/keyset path.
            "uniqueidentifier" => Err(
                "SQL Server UNIQUEIDENTIFIER keys do not have a verified Data Sync ordering contract".into(),
            ),
            // SQL Server exposes rowversion through the legacy `timestamp`
            // type name; it is a generated version token, never a row key.
            "timestamp" | "rowversion" => Err(
                "SQL Server rowversion columns cannot be used as Data Sync keys".into(),
            ),
            "datetime2" | "datetimeoffset" => {
                let precision = lower
                    .split_once('(')
                    .and_then(|(_, rest)| rest.strip_suffix(')'))
                    .and_then(|value| value.parse::<u8>().ok())
                    .unwrap_or(7);
                Ok(SyncKeyContract::reject_nulls(SyncKeyKind::Timestamp {
                    with_timezone: base == "datetimeoffset",
                    precision,
                }))
            }
            _ => contract_from_column(column),
        }
    }

    fn validate_transfer_source_column(&self, column: &ColumnSchema) -> Result<(), String> {
        let native = native_type_name(&column.data_type).to_ascii_lowercase();
        if matches!(native.as_str(), "timestamp" | "rowversion") {
            return Err(format!(
                "SQL Server {native} is a generated row version, not a timestamp value; materialize it into a regular binary column before transfer"
            ));
        }
        Ok(())
    }

    fn transfer_source_type_is_native_only(
        &self,
        _column: &ColumnSchema,
        source_ir: &IRColumn,
    ) -> bool {
        matches!(&source_ir.ir_type, IRType::Other(_))
    }

    fn transfer_source_requires_collation_preservation(&self, column: &ColumnSchema) -> bool {
        matches!(
            native_type_name(&column.data_type)
                .to_ascii_lowercase()
                .as_str(),
            "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext"
        )
    }

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
                      WHEN t.name = 'float' THEN \
                        '(' + CAST(c.precision AS varchar(10)) + ')' \
                      WHEN t.name IN ('datetime2','datetimeoffset','time') THEN \
                        '(' + CAST(c.scale AS varchar(10)) + ')' \
                      ELSE '' END AS full_type \
             FROM sys.columns c \
             JOIN sys.types t ON t.user_type_id = c.user_type_id \
             WHERE c.object_id = OBJECT_ID('{escaped}') \
             ORDER BY c.column_id"
        ))
    }

    /// Mirror of the shared mapping, except that SQL Server's catalog index
    /// types are translated into the dialect-neutral vocabulary the transfer
    /// renderers understand (see [`portable_index_type`]).
    ///
    /// The shared `render_index_ddl` only accepts an ordinary B-tree index, so
    /// leaving SQL Server's `CLUSTERED` / `NONCLUSTERED` vocabulary in place
    /// would fail the whole structure plan for any secondary index. Doing the
    /// translation here — at the point where the catalog vocabulary enters the
    /// IR — keeps every target adapter working, not just SQL Server itself.
    fn table_objects_to_ir(&self, schema: &TableSchema) -> IRTableObjects {
        IRTableObjects {
            indexes: schema
                .indexes
                .iter()
                .map(|index| IRIndex {
                    name: index.name.clone(),
                    columns: index.columns.clone(),
                    is_unique: index.is_unique,
                    is_primary: index.is_primary,
                    index_type: portable_index_type(&index.index_type),
                })
                .collect(),
            foreign_keys: schema
                .foreign_keys
                .iter()
                .map(|foreign_key| IRForeignKey {
                    name: foreign_key.name.clone(),
                    columns: foreign_key.columns.clone(),
                    referenced_table: foreign_key.referenced_table.clone(),
                    referenced_columns: foreign_key.referenced_columns.clone(),
                    on_update: foreign_key.on_update.clone(),
                    on_delete: foreign_key.on_delete.clone(),
                })
                .collect(),
        }
    }

    fn unsupported_transfer_structure_query(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        // Refuse only table-level objects the transfer plan genuinely cannot
        // recreate, so this gate keeps the same granularity as the PostgreSQL
        // adapter: reject what is inexpressible, let through what the IR and
        // the DDL emitters already carry.
        //
        // Secondary indexes and foreign keys are deliberately NOT listed. The
        // host derives `IRTableObjects` from these same catalog rows and emits
        // them through `render_index_ddl` / `render_foreign_key_ddl` after all
        // CREATE TABLE statements, so rejecting them here would refuse objects
        // Data Transfer already recreates on the target. Anything the catalog
        // reader cannot model into `IndexInfo` / `ForeignKeyInfo` (filtered,
        // INCLUDE or descending indexes; disabled or NOT FOR REPLICATION
        // foreign keys) is already refused there, as is any remaining
        // referential action the shared renderer cannot express (SET DEFAULT).
        //
        // What remains below is inexpressible by the IR model or by the SQL
        // Server emitters: the model has no computed-column concept, neither
        // emission path renders CHECK constraints, and
        // `auto_increment_keyword()` hardcodes `IDENTITY(1,1)`, so a
        // non-default seed or increment cannot be recreated.
        //
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
        let native = native_type_name(&lower);

        let ir_type = if native == "nvarchar" {
            let len = parse_length(&lower, "nvarchar");
            IRType::Varchar { length: len }
        } else if native == "varchar" {
            let len = parse_length(&lower, "varchar");
            IRType::Varchar { length: len }
        } else if native == "nchar" {
            let len = parse_length(&lower, "nchar").unwrap_or(1);
            IRType::Char { length: len }
        } else if native == "char" {
            let len = parse_length(&lower, "char").unwrap_or(1);
            IRType::Char { length: len }
        } else if native == "decimal" {
            let (p, s) = parse_precision(&lower, "decimal");
            IRType::Decimal {
                precision: p,
                scale: s,
            }
        } else if native == "numeric" {
            let (p, s) = parse_precision(&lower, "numeric");
            IRType::Decimal {
                precision: p,
                scale: s,
            }
        } else if native == "money" {
            IRType::Decimal {
                precision: 19,
                scale: 4,
            }
        } else if native == "smallmoney" {
            IRType::Decimal {
                precision: 10,
                scale: 4,
            }
        } else if native == "varbinary" {
            let len = parse_length(&lower, "varbinary");
            IRType::Binary { length: len }
        } else if native == "binary" || native == "xml" || native == "tinyint" {
            IRType::Other(lower.clone())
        } else if native == "bit" {
            IRType::Bool
        } else if native == "smallint" {
            IRType::Int16
        } else if native == "int" || native == "integer" {
            IRType::Int32
        } else if native == "bigint" {
            IRType::Int64
        } else if native == "real" {
            IRType::Float32
        } else if native == "float" {
            match parse_length(&lower, "float") {
                Some(precision) if precision <= 24 => IRType::Float32,
                _ => IRType::Float64,
            }
        } else if native == "text" || native == "ntext" {
            IRType::Text
        } else if native == "image" {
            IRType::Blob
        } else if native == "date" {
            IRType::Date
        } else if native == "time" {
            IRType::Time {
                with_timezone: false,
            }
        } else if native == "datetimeoffset" {
            IRType::Timestamp {
                with_timezone: true,
            }
        } else if matches!(native, "datetime" | "datetime2" | "smalldatetime") {
            IRType::Timestamp {
                with_timezone: false,
            }
        } else if native == "uniqueidentifier" {
            IRType::Uuid
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
            IRType::Timestamp {
                with_timezone: true,
            } => "DATETIMEOFFSET".into(),
            IRType::Timestamp {
                with_timezone: false,
            } => "DATETIME2".into(),
            IRType::Json => "NVARCHAR(MAX)".into(),
            IRType::Uuid => "UNIQUEIDENTIFIER".into(),
            IRType::Bit { .. } => "BIT".into(),
            IRType::Other(native) if is_safe_native_only_type(native) => native.clone(),
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

    fn supports_explicit_identity_values(&self) -> bool {
        true
    }

    /// SQL Server identifies an index by `(object_id, index_id)`, not by a
    /// schema-level naming object, so two different tables may reuse the same
    /// index name. Reporting the wider namespace here stops the host's
    /// `ensure_unique_object_name` from rejecting a plan that SQL Server would
    /// accept.
    fn index_names_are_table_scoped(&self) -> bool {
        true
    }

    fn validate_transfer_column_type(
        &self,
        source_column: &ColumnSchema,
        source_ir: &IRColumn,
        _source_text_limit_bytes: Option<u64>,
        source_requires_collation_preservation: bool,
        source_type_is_native_only: bool,
        target_native_type: Option<&str>,
        _creating_target: bool,
    ) -> Result<(), String> {
        if source_requires_collation_preservation {
            return Err(format!(
                "source column '{}' uses SQL Server collation semantics that cannot be proven equivalent; choose the target default collation explicitly or review a matching target collation",
                source_column.name
            ));
        }
        if matches!(&source_ir.ir_type, IRType::Decimal { precision: 0, .. }) {
            return Err(
                "unbounded decimal source values cannot be proven to fit SQL Server's maximum precision 38; select a bounded reviewed target type".into(),
            );
        }
        if source_type_is_native_only {
            let IRType::Other(source_native) = &source_ir.ir_type else {
                return Err("native-only source type marker is inconsistent".into());
            };
            if !is_safe_native_only_type(source_native) {
                return Err(format!(
                    "SQL Server cannot safely recreate native-only source type '{source_native}'"
                ));
            }
            let target_native = target_native_type.ok_or_else(|| {
                "target native type is unavailable for native-only source data".to_string()
            })?;
            if normalize_native_type(source_native) != normalize_native_type(target_native) {
                return Err(format!(
                    "native-only source type '{source_native}' requires the same target type; found '{target_native}'"
                ));
            }
        }
        Ok(())
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
        assert!(sql.contains("WHEN t.name = 'float'"));

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
            IRType::Other("tinyint".into())
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
    fn sqlserver_sync_key_contract_fails_closed_for_collated_and_guid_keys() {
        let adapter = SqlServerSyncAdapter;
        for ty in [
            "varchar(32)",
            "nvarchar(64)",
            "uniqueidentifier",
            "rowversion",
        ] {
            let error = adapter
                .sync_key_contract(&col("key", ty))
                .expect_err("key ordering must be verified before paging");
            assert!(!error.is_empty(), "{ty}");
        }
    }

    #[test]
    fn sqlserver_sync_key_contract_preserves_decimal_and_datetime_semantics() {
        let adapter = SqlServerSyncAdapter;
        assert!(matches!(
            adapter.sync_key_contract(&col("key", "decimal(38, 18)")),
            Ok(SyncKeyContract {
                kind: SyncKeyKind::Decimal { scale: Some(18) },
                null_policy: datazen_driver_api::SyncKeyNullPolicy::Reject,
            })
        ));
        assert!(matches!(
            adapter.sync_key_contract(&col("key", "datetimeoffset(7)")),
            Ok(SyncKeyContract {
                kind: SyncKeyKind::Timestamp {
                    with_timezone: true,
                    precision: 7,
                },
                null_policy: datazen_driver_api::SyncKeyNullPolicy::Reject,
            })
        ));
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
        assert_eq!(
            a.column_to_ir(&col("x", "xml"), None).ir_type,
            IRType::Other("xml".into())
        );
    }

    #[test]
    fn sqlserver_transfer_preserves_exact_and_native_only_types() {
        let adapter = SqlServerSyncAdapter;
        assert!(adapter.supports_explicit_identity_values());
        assert_eq!(
            adapter.column_to_ir(&col("amount", "money"), None).ir_type,
            IRType::Decimal {
                precision: 19,
                scale: 4
            }
        );
        assert_eq!(
            adapter
                .column_to_ir(&col("small_amount", "smallmoney"), None)
                .ir_type,
            IRType::Decimal {
                precision: 10,
                scale: 4
            }
        );
        assert_eq!(
            adapter
                .column_to_ir(&col("at", "datetimeoffset(7)"), None)
                .ir_type,
            IRType::Timestamp {
                with_timezone: true
            }
        );
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Timestamp {
                with_timezone: true
            }),
            "DATETIMEOFFSET"
        );
        assert_eq!(
            adapter
                .column_to_ir(&col("raw", "binary(16)"), None)
                .ir_type,
            IRType::Other("binary(16)".into())
        );
        assert_eq!(
            adapter.ir_type_to_native(&IRType::Other("binary(16)".into())),
            "binary(16)"
        );
        assert_eq!(
            adapter.column_to_ir(&col("f", "float(24)"), None).ir_type,
            IRType::Float32
        );
        assert_eq!(
            adapter.column_to_ir(&col("f", "float(53)"), None).ir_type,
            IRType::Float64
        );
    }

    #[test]
    fn sqlserver_transfer_rejects_generated_rowversion_and_unsafe_native_types() {
        let adapter = SqlServerSyncAdapter;
        assert!(adapter
            .validate_transfer_source_column(&col("version", "rowversion"))
            .is_err());
        assert!(adapter
            .validate_transfer_source_column(&col("version", "timestamp"))
            .is_err());
        assert!(!is_safe_native_only_type("custom_type"));
        assert!(!is_safe_native_only_type("binary(MAX)"));
        assert!(is_safe_native_only_type("binary(8000)"));
    }

    #[test]
    fn sqlserver_transfer_native_only_mapping_requires_exact_target_type() {
        let adapter = SqlServerSyncAdapter;
        let source_column = col("raw", "tinyint");
        let source_ir = adapter.column_to_ir(&source_column, None);
        assert!(adapter
            .validate_transfer_column_type(
                &source_column,
                &source_ir,
                None,
                false,
                true,
                Some("TINYINT"),
                false,
            )
            .is_ok());
        assert!(adapter
            .validate_transfer_column_type(
                &source_column,
                &source_ir,
                None,
                false,
                true,
                Some("SMALLINT"),
                false,
            )
            .is_err());

        let unbounded_decimal = IRColumn {
            name: "amount".into(),
            ir_type: IRType::Decimal {
                precision: 0,
                scale: 0,
            },
            nullable: true,
            default_expr: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: None,
        };
        assert!(adapter
            .validate_transfer_column_type(
                &col("amount", "numeric"),
                &unbounded_decimal,
                None,
                false,
                false,
                Some("DECIMAL(38,18)"),
                true,
            )
            .is_err());
    }

    #[test]
    fn sqlserver_transfer_requires_explicit_collation_decision() {
        let adapter = SqlServerSyncAdapter;
        let source_column = col("name", "nvarchar(20)");
        assert!(adapter.transfer_source_requires_collation_preservation(&source_column));
        let source_ir = adapter.column_to_ir(&source_column, None);
        assert!(adapter
            .validate_transfer_column_type(
                &source_column,
                &source_ir,
                None,
                true,
                false,
                Some("NVARCHAR(20)"),
                true,
            )
            .is_err());
        assert!(adapter
            .validate_transfer_column_type(
                &source_column,
                &source_ir,
                None,
                false,
                false,
                Some("NVARCHAR(20)"),
                true,
            )
            .is_ok());
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
    fn sqlserver_source_preflight_rejects_only_unrepresentable_structure_objects() {
        let query = SqlServerSyncAdapter
            .unsupported_transfer_structure_query("db]name", Some("sales' data"), "people's")
            .expect("SQL Server source preflight query");
        // Escaping invariants: catalog as a quoted identifier, schema/table as
        // quoted literals.
        assert!(query.contains("[db]]name].sys.computed_columns"));
        assert!(query.contains("s.name = N'sales'' data'"));
        assert!(query.contains("t.name = N'people''s'"));
        // Still rejected: no IR concept, no emission path, hardcoded IDENTITY.
        assert!(query.contains("computed column "));
        assert!(query.contains("CHECK constraint "));
        assert!(query.contains("non-default seed/increment"));
        // Not rejected: the host emits both from the same catalog rows through
        // `render_index_ddl` / `render_foreign_key_ddl`.
        assert!(!query.contains("secondary index "));
        assert!(!query.contains("foreign key "));
        assert!(!query.contains("sys.indexes"));
        assert!(!query.contains("sys.foreign_keys"));
    }

    #[test]
    fn sqlserver_portable_index_type_maps_own_catalog_vocabulary_only() {
        assert_eq!(portable_index_type("NONCLUSTERED"), "");
        assert_eq!(portable_index_type("CLUSTERED"), "");
        assert_eq!(portable_index_type("UNIQUE_CONSTRAINT:NONCLUSTERED"), "");
        assert_eq!(portable_index_type("UNIQUE_CONSTRAINT:CLUSTERED"), "");
        assert_eq!(portable_index_type("  "), "");
        // Other engines' index methods stay visible so the shared renderer
        // keeps rejecting them.
        assert_eq!(portable_index_type("btree"), "btree");
        assert_eq!(portable_index_type("GIN"), "GIN");
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
