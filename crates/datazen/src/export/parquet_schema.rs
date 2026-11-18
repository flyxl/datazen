//! Parquet schema inference (PRD `docs/archive/development/parquet-export-prd.md`
//! §4 F3) — pure functions, decoupled from the writer so the mapping tables
//! are testable without an `ArrowWriter`.
//!
//! The only inputs are `ColumnInfo { name, data_type: String, nullable: bool }`
//! (see `crates/datazen-driver-api/src/types.rs`): each driver passes its
//! database's own type name through verbatim (PG protocol names like `int4` /
//! `varchar`, sqlx MySQL `type_info().name()`, SQLite's declared `INTEGER`),
//! there is no unified type enum. Every column therefore goes through three
//! steps, in order:
//!
//! 1. **Dialect mapping** — look up the driver-reported type in the dialect
//!    table selected by `ExportTablesRequest.database_type` (wire values
//!    `"postgresql" | "mysql" | "sqlite" | "sqlserver"`, matched
//!    case-insensitively after trimming). A `None` dialect — or an unmapped
//!    one (`questdb`, `cloudberry`, `mariadb`, ...) — skips this step.
//! 2. **First-batch value inference** — a column the table misses looks at
//!    its non-null `Value` variants in the first `rows()` batch: all
//!    `Integer` → `Int64`, all `Float` → `Float64`, all `Bool` → `Boolean`;
//!    any `String` / `Timestamp` / `Json` / `Bytes` value settles the column
//!    as `Utf8` on sight, and a column mixing variants — or all `NULL`, or
//!    with no rows at all — is `Utf8`.
//! 3. **Utf8 fallback** — a column that cannot be pinned down is `Utf8`;
//!    one undecided column never fails the whole export.
//!
//! Timestamps stay `Utf8` in v1 (`Value::Timestamp` is already a string; no
//! logical time types and no `Decimal128` are in scope — PRD §3 non-goals),
//! and `numeric` / `decimal` map to `Utf8` too, to avoid precision loss.
//! Nullability is carried verbatim from `ColumnInfo.nullable`.

use arrow::datatypes::{DataType, Field, Schema};
use datazen_driver_api::{ColumnInfo, Value};

/// The wire `databaseType` values that have a v1 mapping table (PRD §4 F3
/// minimal set). Dialects outside this list skip the mapping step and fall
/// through to value inference, which is always safe.
const DIALECT_POSTGRESQL: &str = "postgresql";
const DIALECT_MYSQL: &str = "mysql";
const DIALECT_SQLITE: &str = "sqlite";
const DIALECT_SQLSERVER: &str = "sqlserver";

/// Normalize a driver-reported type string: trim, lowercase, and drop a
/// parenthesized size/precision suffix (`tinyint(1)` → `tinyint`,
/// `varchar(255)` → `varchar`, SQLite's declared `INTEGER` → `integer`).
fn normalize_type_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let base = match trimmed.find('(') {
        Some(idx) => &trimmed[..idx],
        None => trimmed,
    };
    base.trim().to_ascii_lowercase()
}

/// Step 1: the per-dialect mapping table. `None` means "not in the v1 minimal
/// set" — the column falls through to value inference instead of guessing.
///
/// `Utf8` entries are deliberate decisions, not misses: a mapped `varchar`
/// column whose values look numeric still exports as `Utf8`, so the schema is
/// stable whether or not rows have arrived yet.
fn dialect_type(dialect: &str, raw_type: &str) -> Option<DataType> {
    let normalized = normalize_type_name(raw_type);
    let t = normalized.as_str();
    match dialect {
        DIALECT_POSTGRESQL => match t {
            "int2" => Some(DataType::Int16),
            "int4" => Some(DataType::Int32),
            "int8" => Some(DataType::Int64),
            "float4" => Some(DataType::Float32),
            "float8" => Some(DataType::Float64),
            "bool" => Some(DataType::Boolean),
            "bytea" => Some(DataType::Binary),
            // Strings stay Utf8 even when the values look numeric; timestamps
            // / uuid / json are Utf8 in v1 (no logical types, PRD §3);
            // numeric/decimal avoid Decimal128 precision loss.
            "varchar" | "text" | "char" => Some(DataType::Utf8),
            "timestamp" | "timestamptz" | "date" | "time" => Some(DataType::Utf8),
            "uuid" | "json" | "jsonb" => Some(DataType::Utf8),
            "numeric" | "decimal" => Some(DataType::Utf8),
            _ => None,
        },
        DIALECT_MYSQL => match t {
            "int" => Some(DataType::Int32),
            "bigint" => Some(DataType::Int64),
            // tinyint(1) normalizes to tinyint → Int8; treating it as Boolean
            // is PRD §8 Q2, deferred and documented, not silent.
            "tinyint" => Some(DataType::Int8),
            "float" => Some(DataType::Float32),
            "double" => Some(DataType::Float64),
            "decimal" => Some(DataType::Utf8),
            "varchar" | "text" => Some(DataType::Utf8),
            "datetime" | "timestamp" | "date" => Some(DataType::Utf8),
            "blob" => Some(DataType::Binary),
            "json" => Some(DataType::Utf8),
            _ => None,
        },
        DIALECT_SQLITE => match t {
            // SQLite type affinity is loose; anything the declared name does
            // not pin down is left to value inference, which matters more here
            // than the mapping does.
            "integer" => Some(DataType::Int64),
            "real" => Some(DataType::Float64),
            "text" => Some(DataType::Utf8),
            "blob" => Some(DataType::Binary),
            "numeric" => Some(DataType::Utf8),
            _ => None,
        },
        DIALECT_SQLSERVER => match t {
            "int" => Some(DataType::Int32),
            "bigint" => Some(DataType::Int64),
            "smallint" => Some(DataType::Int16),
            "tinyint" => Some(DataType::UInt8),
            "float" => Some(DataType::Float64),
            "real" => Some(DataType::Float32),
            "bit" => Some(DataType::Boolean),
            "varchar" | "nvarchar" | "text" => Some(DataType::Utf8),
            "datetime" | "datetime2" | "date" => Some(DataType::Utf8),
            "varbinary" => Some(DataType::Binary),
            "decimal" => Some(DataType::Utf8),
            _ => None,
        },
        _ => None,
    }
}

/// Step 2: infer one column's type from its non-null values in the first
/// batch. Returns `Utf8` whenever the values give no unanimous answer.
///
/// A row shorter than the column list contributes no signal for its missing
/// tail — the same silence as a NULL.
fn first_batch_type(column_index: usize, first_batch: &[Vec<Option<Value>>]) -> DataType {
    let mut saw_integer = false;
    let mut saw_float = false;
    let mut saw_bool = false;
    for row in first_batch {
        match row.get(column_index).and_then(|value| value.as_ref()) {
            Some(Value::Integer(_)) => saw_integer = true,
            Some(Value::Float(_)) => saw_float = true,
            Some(Value::Bool(_)) => saw_bool = true,
            // String / Timestamp / Json / Bytes are Utf8 on sight (PRD: 其余 →
            // Utf8), so one such value settles the column immediately.
            Some(_) => return DataType::Utf8,
            None => continue,
        }
    }
    match (saw_integer, saw_float, saw_bool) {
        (true, false, false) => DataType::Int64,
        (false, true, false) => DataType::Float64,
        (false, false, true) => DataType::Boolean,
        // Nothing seen at all (empty table / all-NULL column), or a mix of
        // variants — Utf8 either way.
        _ => DataType::Utf8,
    }
}

/// Infer the Arrow schema for one Parquet export (PRD §4 F3): per column, the
/// dialect mapping table first, then first-batch value inference, then the
/// `Utf8` fallback. A column that cannot be decided never fails the export.
///
/// `first_batch` is the first `rows()` batch (≤ 500 rows by construction of
/// the streaming loop); it is empty for an empty table, in which case every
/// column takes the dialect mapping or the `Utf8` fallback — the same
/// decision `finish()` relies on for zero-row exports.
pub fn infer_schema(
    columns: &[ColumnInfo],
    dialect: Option<&str>,
    first_batch: &[Vec<Option<Value>>],
) -> Schema {
    let dialect = dialect.map(|d| d.trim().to_ascii_lowercase());
    Schema::new(
        columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let data_type = dialect
                    .as_deref()
                    .and_then(|d| dialect_type(d, &column.data_type))
                    .unwrap_or_else(|| first_batch_type(index, first_batch));
                Field::new(column.name.as_str(), data_type, column.nullable)
            })
            .collect::<Vec<Field>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::DataType;

    fn col(name: &str, data_type: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
        }
    }

    /// Schema of one column, over a table of rows.
    fn single_column_type(
        data_type: &str,
        dialect: Option<&str>,
        rows: &[Vec<Option<Value>>],
    ) -> DataType {
        let schema = infer_schema(&[col("c", data_type, true)], dialect, rows);
        schema.field(0).data_type().clone()
    }

    fn int_cell(v: i64) -> Option<Value> {
        Some(Value::Integer(v))
    }

    #[test]
    fn postgres_dialect_mapping_table() {
        let cases: &[(&str, DataType)] = &[
            ("int2", DataType::Int16),
            ("int4", DataType::Int32),
            ("int8", DataType::Int64),
            ("float4", DataType::Float32),
            ("float8", DataType::Float64),
            ("bool", DataType::Boolean),
            ("bytea", DataType::Binary),
            ("varchar", DataType::Utf8),
            ("text", DataType::Utf8),
            ("char", DataType::Utf8),
            ("timestamp", DataType::Utf8),
            ("timestamptz", DataType::Utf8),
            ("date", DataType::Utf8),
            ("time", DataType::Utf8),
            ("uuid", DataType::Utf8),
            ("json", DataType::Utf8),
            ("jsonb", DataType::Utf8),
            ("numeric", DataType::Utf8),
            ("decimal", DataType::Utf8),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                single_column_type(raw, Some("postgresql"), &[]),
                *expected,
                "postgres {raw}"
            );
        }
    }

    #[test]
    fn mysql_dialect_mapping_table() {
        let cases: &[(&str, DataType)] = &[
            ("int", DataType::Int32),
            ("bigint", DataType::Int64),
            // tinyint(1) normalizes to tinyint -> Int8; the Boolean reading of
            // tinyint(1) is PRD §8 Q2, deliberately deferred, not silent.
            ("tinyint", DataType::Int8),
            ("tinyint(1)", DataType::Int8),
            ("float", DataType::Float32),
            ("double", DataType::Float64),
            ("decimal", DataType::Utf8),
            ("decimal(10,2)", DataType::Utf8),
            ("varchar", DataType::Utf8),
            ("varchar(255)", DataType::Utf8),
            ("text", DataType::Utf8),
            ("datetime", DataType::Utf8),
            ("timestamp", DataType::Utf8),
            ("date", DataType::Utf8),
            ("blob", DataType::Binary),
            ("json", DataType::Utf8),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                single_column_type(raw, Some("mysql"), &[]),
                *expected,
                "mysql {raw}"
            );
        }
    }

    #[test]
    fn sqlite_dialect_mapping_table() {
        // SQLite reports declared types in their original case; normalization
        // is part of the mapping's contract.
        let cases: &[(&str, DataType)] = &[
            ("integer", DataType::Int64),
            ("INTEGER", DataType::Int64),
            ("real", DataType::Float64),
            ("REAL", DataType::Float64),
            ("text", DataType::Utf8),
            ("TEXT", DataType::Utf8),
            ("blob", DataType::Binary),
            ("numeric", DataType::Utf8),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                single_column_type(raw, Some("sqlite"), &[]),
                *expected,
                "sqlite {raw}"
            );
        }
    }

    #[test]
    fn sqlserver_dialect_mapping_table() {
        let cases: &[(&str, DataType)] = &[
            ("int", DataType::Int32),
            ("bigint", DataType::Int64),
            ("smallint", DataType::Int16),
            ("tinyint", DataType::UInt8),
            ("float", DataType::Float64),
            ("real", DataType::Float32),
            ("bit", DataType::Boolean),
            ("varchar", DataType::Utf8),
            ("nvarchar", DataType::Utf8),
            ("text", DataType::Utf8),
            ("datetime", DataType::Utf8),
            ("datetime2", DataType::Utf8),
            ("date", DataType::Utf8),
            ("varbinary", DataType::Binary),
            ("decimal", DataType::Utf8),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                single_column_type(raw, Some("sqlserver"), &[]),
                *expected,
                "sqlserver {raw}"
            );
        }
    }

    #[test]
    fn dialect_matching_is_trimmed_and_case_insensitive() {
        for dialect in ["PostgreSQL", " POSTGRESQL ", "postgresql"] {
            assert_eq!(
                single_column_type("int4", Some(dialect), &[]),
                DataType::Int32,
                "dialect {dialect:?}"
            );
        }
    }

    #[test]
    fn unmapped_type_in_known_dialect_falls_through_to_value_inference() {
        // "money" is not in the v1 PG table; the first batch decides.
        assert_eq!(
            single_column_type("money", Some("postgresql"), &[vec![int_cell(7)]]),
            DataType::Int64
        );
        assert_eq!(
            single_column_type(
                "money",
                Some("postgresql"),
                &[vec![Some(Value::Float(1.5))]]
            ),
            DataType::Float64
        );
        assert_eq!(
            single_column_type(
                "money",
                Some("postgresql"),
                &[vec![Some(Value::Bool(true))]]
            ),
            DataType::Boolean
        );
        // The same fall-through holds in every dialect that has a table.
        for dialect in ["mysql", "sqlite", "sqlserver"] {
            assert_eq!(
                single_column_type("money", Some(dialect), &[vec![int_cell(7)]]),
                DataType::Int64,
                "dialect {dialect}"
            );
        }
    }

    #[test]
    fn unmapped_dialect_skips_the_table_and_uses_values() {
        // questdb / cloudberry / mariadb / oracle ... have no v1 table; value
        // inference is always safe.
        assert_eq!(
            single_column_type("whatever", Some("oracle"), &[vec![int_cell(9)]]),
            DataType::Int64
        );
        assert_eq!(
            single_column_type("whatever", None, &[vec![int_cell(9)]]),
            DataType::Int64
        );
        assert_eq!(
            single_column_type("whatever", None, &[vec![Some(Value::Float(2.5))]]),
            DataType::Float64
        );
        assert_eq!(
            single_column_type("whatever", None, &[vec![Some(Value::Bool(false))]]),
            DataType::Boolean
        );
    }

    #[test]
    fn string_bytes_timestamp_and_json_values_settle_utf8() {
        let values: &[Value] = &[
            Value::String("x".into()),
            Value::Bytes(vec![1, 2, 3]),
            Value::Timestamp("2026-11-12 00:00:00".into()),
            Value::Json(serde_json::Value::String("{}".into())),
        ];
        for value in values {
            assert_eq!(
                single_column_type("unknown", None, &[vec![Some(value.clone())]]),
                DataType::Utf8,
                "value {value:?}"
            );
        }
    }

    #[test]
    fn all_null_column_falls_back_to_utf8() {
        // All NULL — including a completely empty table — gives no signal.
        assert_eq!(
            single_column_type("unknown", None, &[vec![None], vec![None]]),
            DataType::Utf8
        );
        assert_eq!(single_column_type("unknown", None, &[]), DataType::Utf8);
        // Mapped types still apply to an empty table: the empty-export schema
        // is the dialect mapping plus the Utf8 fallback, with no values.
        assert_eq!(
            single_column_type("int8", Some("postgresql"), &[]),
            DataType::Int64
        );
    }

    #[test]
    fn mixed_variants_fall_back_to_utf8() {
        assert_eq!(
            single_column_type(
                "unknown",
                None,
                &[vec![int_cell(1)], vec![Some(Value::Float(1.5))]]
            ),
            DataType::Utf8
        );
        assert_eq!(
            single_column_type(
                "unknown",
                None,
                &[vec![int_cell(1)], vec![Some(Value::Bool(true))]]
            ),
            DataType::Utf8
        );
        // NULLs do not count as a variant: Integers plus NULLs stay Int64.
        assert_eq!(
            single_column_type(
                "unknown",
                None,
                &[vec![None], vec![int_cell(3)], vec![None]]
            ),
            DataType::Int64
        );
    }

    #[test]
    fn dialect_mapping_takes_precedence_over_value_inference() {
        // A mapped varchar holding integer-looking strings stays Utf8 — the
        // mapping table decides before any values are consulted.
        assert_eq!(
            single_column_type(
                "varchar",
                Some("postgresql"),
                &[vec![int_cell(1)], vec![int_cell(2)]]
            ),
            DataType::Utf8
        );
    }

    #[test]
    fn names_and_nullability_come_from_column_info() {
        let schema = infer_schema(
            &[col("id", "int8", false), col("note", "text", true)],
            Some("postgresql"),
            &[],
        );
        assert_eq!(schema.field(0).name().as_str(), "id");
        assert!(!schema.field(0).is_nullable());
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).name().as_str(), "note");
        assert!(schema.field(1).is_nullable());
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
    }

    #[test]
    fn empty_column_set_yields_empty_schema() {
        let schema = infer_schema(&[], None, &[]);
        assert!(schema.fields().is_empty());
        // Even with stray rows — nothing to infer, no panic.
        let schema = infer_schema(&[], Some("sqlite"), &[vec![int_cell(1)]]);
        assert!(schema.fields().is_empty());
    }

    #[test]
    fn short_rows_are_silent_for_their_missing_tail() {
        // Rows narrower than the column list contribute nothing for their
        // missing tail — same silence as a NULL, not an error.
        let schema = infer_schema(
            &[col("a", "unknown", true), col("b", "unknown", true)],
            None,
            &[vec![int_cell(1)], vec![int_cell(2)]],
        );
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
    }
}
