//! Column type resolution, ported from `packages/frontend/src/lib/cellTypes.ts`.
//!
//! The shared table is the host's single answer to "what kind of value does
//! this column hold"; the cell colour, the editor control and the filter
//! operators are all projections of it, so a column cannot be painted as text
//! and edited as a number.
//!
//! Order is load-bearing: `json` precedes the binary rules (a composite name
//! like `binary json` is JSON) and the explicit text block precedes the
//! numeric rules (`point` contains `int` but is a geometry type).
#![allow(dead_code)] // consumers: CellRenderer/GridCell/DetailPanel (T004/T015/T009)

use crate::support::data_type_colors::DataTypeFamily;

/// The normalised cell type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellType {
    Text,
    Number,
    Bool,
    Date,
    Time,
    DateTime,
    Json,
    Bytes,
    Enum,
    Uuid,
    Unknown,
}

/// How a rule pattern is compared with a type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellTypeMatch {
    Exact,
    Prefix,
    Suffix,
    Substring,
    Token,
    TokenPrefix,
}

/// One rule in the ordered type table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellTypeRule {
    pub match_kind: CellTypeMatch,
    pub pattern: &'static str,
    pub cell_type: CellType,
}

/// Driver-supplied hints. Every field is optional in the React version; here
/// an empty vector means "not declared".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellTypeHints {
    pub overrides: Vec<CellTypeRule>,
    pub enum_types: Vec<String>,
    pub zoned_types: Vec<String>,
    pub read_only_types: Vec<String>,
}

use CellType::{Bool, Bytes, Date, DateTime, Enum, Json, Number, Text, Time, Unknown, Uuid};
use CellTypeMatch::{Exact, Prefix, Substring, Suffix, Token, TokenPrefix};

/// The shared default table, in evaluation order.
pub(crate) const DEFAULT_CELL_TYPE_RULES: &[CellTypeRule] = &[
    // Low-cardinality enumerations.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "enum",
        cell_type: Enum,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "set",
        cell_type: Enum,
    },
    // Structured values.
    CellTypeRule {
        match_kind: Substring,
        pattern: "json",
        cell_type: Json,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "array",
        cell_type: Json,
    },
    CellTypeRule {
        match_kind: Suffix,
        pattern: "[]",
        cell_type: Json,
    },
    // Binary.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "fixedstring",
        cell_type: Bytes,
    },
    CellTypeRule {
        match_kind: Substring,
        pattern: "bytea",
        cell_type: Bytes,
    },
    CellTypeRule {
        match_kind: Substring,
        pattern: "blob",
        cell_type: Bytes,
    },
    CellTypeRule {
        match_kind: Substring,
        pattern: "binary",
        cell_type: Bytes,
    },
    CellTypeRule {
        match_kind: Exact,
        pattern: "raw",
        cell_type: Bytes,
    },
    CellTypeRule {
        match_kind: Exact,
        pattern: "image",
        cell_type: Bytes,
    },
    // UUID.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "uuid",
        cell_type: Uuid,
    },
    CellTypeRule {
        match_kind: Exact,
        pattern: "uniqueidentifier",
        cell_type: Uuid,
    },
    // Booleans.
    CellTypeRule {
        match_kind: Substring,
        pattern: "bool",
        cell_type: Bool,
    },
    // Date and time: timestamp/datetime first so `datetime` is not stolen by
    // `date`, nor `timestamp` by `time`.
    CellTypeRule {
        match_kind: Substring,
        pattern: "timestamp",
        cell_type: DateTime,
    },
    CellTypeRule {
        match_kind: Substring,
        pattern: "datetime",
        cell_type: DateTime,
    },
    // PostgreSQL `interval` is text on the wire.
    CellTypeRule {
        match_kind: Token,
        pattern: "interval",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Exact,
        pattern: "date",
        cell_type: Date,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "date",
        cell_type: Date,
    },
    CellTypeRule {
        match_kind: Exact,
        pattern: "time",
        cell_type: Time,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "time",
        cell_type: Time,
    },
    // Explicit text block: text on the wire, whatever they look like. Before
    // the numeric rules on purpose (`point` contains `int`).
    CellTypeRule {
        match_kind: Token,
        pattern: "bit",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "varbit",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "xml",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "inet",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "cidr",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "macaddr",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "macaddr8",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "geometry",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "geography",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "point",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "polygon",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "linestring",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "hstore",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "year",
        cell_type: Text,
    },
    // `money` is money-formatted text on the wire; parsing it as a number
    // would corrupt it.
    CellTypeRule {
        match_kind: Token,
        pattern: "money",
        cell_type: Text,
    },
    // Numbers.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "int",
        cell_type: Number,
    },
    // Unsigned integers: `uint8` does not start with `int`.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "uint",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "bigint",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "smallint",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "tinyint",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "mediumint",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: Substring,
        pattern: "serial",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "numeric",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "decimal",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "dec",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "float",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "double",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "real",
        cell_type: Number,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "number",
        cell_type: Number,
    },
    // The character family.
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "varchar",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "nvarchar",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "nchar",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "char",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "bpchar",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "character",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "varying",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: TokenPrefix,
        pattern: "text",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "tinytext",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "mediumtext",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "longtext",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "clob",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "string",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "citext",
        cell_type: Text,
    },
    CellTypeRule {
        match_kind: Token,
        pattern: "name",
        cell_type: Text,
    },
];

/// Type names that carry a zone: standard SQL plus the de-facto spellings.
const DEFAULT_ZONED_TYPE_NAMES: &[&str] = &[
    "timestamp with time zone",
    "datetime with time zone",
    "timestamptz",
    "timetz",
    "datetimeoffset",
];

/// Type names that denote an integer column (no fraction).
const INTEGER_TYPE_TOKENS: &[&str] = &[
    "int",
    "int2",
    "int4",
    "int8",
    "int16",
    "int32",
    "int64",
    "int128",
    "int256",
    "integer",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uint128",
    "uint256",
    "bigint",
    "smallint",
    "tinyint",
    "mediumint",
    "serial",
    "serial2",
    "serial4",
    "serial8",
    "bigserial",
    "smallserial",
];

/// Lower-case, trimmed type name.
pub(crate) fn normalize_type_name(data_type: Option<&str>) -> String {
    data_type.unwrap_or("").trim().to_lowercase()
}

/// A token is a run of letters, digits and underscores; everything else is a
/// separator. `int(11) unsigned` → `["int", "11", "unsigned"]`.
pub(crate) fn tokenize_type_name(type_name: &str) -> Vec<String> {
    normalize_type_name(Some(type_name))
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether one rule matches a type name.
pub(crate) fn match_cell_type_rule(type_name: &str, rule: &CellTypeRule) -> bool {
    let normalized = normalize_type_name(Some(type_name));
    let pattern = rule.pattern.to_ascii_lowercase();
    match rule.match_kind {
        Exact => normalized == pattern,
        Prefix => normalized.starts_with(&pattern),
        Suffix => normalized.ends_with(&pattern),
        Substring => normalized.contains(&pattern),
        Token => tokenize_type_name(&normalized)
            .iter()
            .any(|token| token == &pattern),
        TokenPrefix => tokenize_type_name(&normalized)
            .iter()
            .any(|token| token.starts_with(&pattern)),
    }
}

/// Hints match on tokens, but may also name a **whole** type
/// (`user-defined`), which is not a token of it.
fn matches_hint_name(type_name: &str, name: &str) -> bool {
    let pattern = name.trim().to_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if tokenize_type_name(type_name)
        .iter()
        .any(|token| token == &pattern || token.starts_with(&pattern))
    {
        return true;
    }
    normalize_type_name(Some(type_name)).contains(&pattern)
}

/// The normalised type of a column.
///
/// Evaluation order: driver `overrides`, driver `enum_types`, the shared
/// table, then `unknown`.
pub(crate) fn resolve_cell_type(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
) -> CellType {
    let type_name = normalize_type_name(data_type);
    if type_name.is_empty() {
        return Unknown;
    }
    if let Some(hints) = hints {
        for rule in &hints.overrides {
            if match_cell_type_rule(&type_name, rule) {
                return rule.cell_type;
            }
        }
        for name in &hints.enum_types {
            if matches_hint_name(&type_name, name) {
                return Enum;
            }
        }
    }
    for rule in DEFAULT_CELL_TYPE_RULES {
        if match_cell_type_rule(&type_name, rule) {
            return rule.cell_type;
        }
    }
    Unknown
}

/// The render family of a cell type. `null` is a property of the value, not
/// of the type, so it is unreachable here.
pub(crate) fn cell_type_to_data_type_family(cell_type: CellType) -> DataTypeFamily {
    match cell_type {
        Text => DataTypeFamily::Text,
        Number => DataTypeFamily::Number,
        Bool => DataTypeFamily::Bool,
        Date | Time | DateTime => DataTypeFamily::Datetime,
        Json => DataTypeFamily::Json,
        Bytes => DataTypeFamily::Binary,
        Enum | Uuid | Unknown => DataTypeFamily::Text,
    }
}

/// Whether the column's values carry a zone. Zoned values are edited as plain
/// text: a date picker cannot carry an offset.
pub(crate) fn is_zoned_cell_type(data_type: &str) -> bool {
    let type_name = normalize_type_name(Some(data_type));
    if type_name.is_empty() {
        return false;
    }
    DEFAULT_ZONED_TYPE_NAMES
        .iter()
        .any(|name| type_name.contains(name))
}

/// Hint-aware variant of [`is_zoned_cell_type`].
pub(crate) fn is_zoned_cell_type_with_hints(
    data_type: &str,
    hints: Option<&CellTypeHints>,
) -> bool {
    if is_zoned_cell_type(data_type) {
        return true;
    }
    let type_name = normalize_type_name(Some(data_type));
    hints
        .map(|hints| {
            hints
                .zoned_types
                .iter()
                .any(|name| matches_hint_name(&type_name, name))
        })
        .unwrap_or(false)
}

/// Whether the driver declared this column type as not writable in place.
/// Only the driver can answer this, so there is no shared default.
pub(crate) fn is_read_only_cell_type(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
) -> bool {
    let type_name = normalize_type_name(data_type);
    if type_name.is_empty() {
        return false;
    }
    hints
        .map(|hints| {
            hints
                .read_only_types
                .iter()
                .any(|name| matches_hint_name(&type_name, name))
        })
        .unwrap_or(false)
}

/// Whether the column holds whole numbers. The answer comes from the type
/// name because `CellType` folds integers and decimals into one `Number`.
pub(crate) fn is_integer_cell_type(data_type: Option<&str>, hints: Option<&CellTypeHints>) -> bool {
    let type_name = normalize_type_name(data_type);
    if type_name.is_empty() {
        return false;
    }
    if resolve_cell_type(Some(&type_name), hints) != Number {
        return false;
    }
    tokenize_type_name(&type_name)
        .iter()
        .any(|token| INTEGER_TYPE_TOKENS.contains(&token.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_missing_types_are_unknown() {
        assert_eq!(resolve_cell_type(None, None), Unknown);
        assert_eq!(resolve_cell_type(Some("   "), None), Unknown);
    }

    #[test]
    fn resolves_the_common_sql_families() {
        assert_eq!(resolve_cell_type(Some("text"), None), Text);
        assert_eq!(resolve_cell_type(Some("varchar(255)"), None), Text);
        assert_eq!(resolve_cell_type(Some("int(11) unsigned"), None), Number);
        assert_eq!(resolve_cell_type(Some("bigint"), None), Number);
        assert_eq!(resolve_cell_type(Some("numeric(10,2)"), None), Number);
        assert_eq!(resolve_cell_type(Some("bool"), None), Bool);
        assert_eq!(resolve_cell_type(Some("boolean"), None), Bool);
        assert_eq!(resolve_cell_type(Some("date"), None), Date);
        assert_eq!(resolve_cell_type(Some("time"), None), Time);
        assert_eq!(resolve_cell_type(Some("timestamp"), None), DateTime);
        assert_eq!(resolve_cell_type(Some("datetime"), None), DateTime);
        assert_eq!(resolve_cell_type(Some("jsonb"), None), Json);
        assert_eq!(resolve_cell_type(Some("bytea"), None), Bytes);
        assert_eq!(resolve_cell_type(Some("enum('a','b')"), None), Enum);
        assert_eq!(resolve_cell_type(Some("uuid"), None), Uuid);
    }

    #[test]
    fn geometry_and_network_types_stay_text() {
        for name in [
            "point",
            "polygon",
            "geometry",
            "geography",
            "inet",
            "cidr",
            "macaddr",
            "hstore",
            "money",
            "interval",
        ] {
            assert_eq!(resolve_cell_type(Some(name), None), Text, "{name}");
        }
    }

    #[test]
    fn json_precedes_binary_rules() {
        // A composite name like `binary json` must not be stolen by `binary`.
        assert_eq!(resolve_cell_type(Some("binary json"), None), Json);
        assert_eq!(resolve_cell_type(Some("text[]"), None), Json);
        assert_eq!(resolve_cell_type(Some("integer array"), None), Json);
    }

    #[test]
    fn driver_overrides_win_over_the_shared_table() {
        let hints = CellTypeHints {
            overrides: vec![CellTypeRule {
                match_kind: Exact,
                pattern: "timestamp",
                cell_type: Bytes,
            }],
            ..Default::default()
        };
        assert_eq!(resolve_cell_type(Some("timestamp"), Some(&hints)), Bytes);
    }

    #[test]
    fn driver_enum_hints_answer_enum_even_for_opaque_names() {
        let hints = CellTypeHints {
            enum_types: vec!["user-defined".to_owned()],
            ..Default::default()
        };
        assert_eq!(resolve_cell_type(Some("USER-DEFINED"), Some(&hints)), Enum);
    }

    #[test]
    fn tokenize_splits_on_non_identifier_characters() {
        assert_eq!(
            tokenize_type_name("Nullable(Int32)"),
            vec!["nullable", "int32"]
        );
        assert_eq!(
            tokenize_type_name("int(11) unsigned"),
            vec!["int", "11", "unsigned"]
        );
        assert_eq!(tokenize_type_name(""), Vec::<String>::new());
    }

    #[test]
    fn unsigned_integers_are_numbers() {
        for name in ["uint8", "uint64", "uint256"] {
            assert_eq!(resolve_cell_type(Some(name), None), Number, "{name}");
        }
    }

    #[test]
    fn zoned_types_are_detected() {
        assert!(is_zoned_cell_type("timestamptz"));
        assert!(is_zoned_cell_type("timestamp with time zone"));
        assert!(is_zoned_cell_type("datetimeoffset"));
        assert!(!is_zoned_cell_type("timestamp"));
        assert!(!is_zoned_cell_type(""));
        let hints = CellTypeHints {
            zoned_types: vec!["my-zoned".to_owned()],
            ..Default::default()
        };
        assert!(is_zoned_cell_type_with_hints("my-zoned", Some(&hints)));
    }

    #[test]
    fn read_only_types_only_come_from_hints() {
        assert!(!is_read_only_cell_type(Some("rowversion"), None));
        let hints = CellTypeHints {
            read_only_types: vec!["rowversion".to_owned()],
            ..Default::default()
        };
        assert!(is_read_only_cell_type(Some("rowversion"), Some(&hints)));
    }

    #[test]
    fn integers_are_distinguished_from_decimals() {
        assert!(is_integer_cell_type(Some("int8"), None));
        assert!(is_integer_cell_type(Some("serial"), None));
        assert!(is_integer_cell_type(Some("uint64"), None));
        assert!(!is_integer_cell_type(Some("numeric(10,2)"), None));
        assert!(!is_integer_cell_type(Some("text"), None));
        assert!(!is_integer_cell_type(None, None));
    }

    #[test]
    fn families_project_every_cell_type() {
        assert_eq!(cell_type_to_data_type_family(Text), DataTypeFamily::Text);
        assert_eq!(
            cell_type_to_data_type_family(Number),
            DataTypeFamily::Number
        );
        assert_eq!(cell_type_to_data_type_family(Bool), DataTypeFamily::Bool);
        assert_eq!(
            cell_type_to_data_type_family(DateTime),
            DataTypeFamily::Datetime
        );
        assert_eq!(
            cell_type_to_data_type_family(Date),
            DataTypeFamily::Datetime
        );
        assert_eq!(cell_type_to_data_type_family(Json), DataTypeFamily::Json);
        assert_eq!(cell_type_to_data_type_family(Bytes), DataTypeFamily::Binary);
        assert_eq!(cell_type_to_data_type_family(Enum), DataTypeFamily::Text);
        assert_eq!(cell_type_to_data_type_family(Uuid), DataTypeFamily::Text);
        assert_eq!(cell_type_to_data_type_family(Unknown), DataTypeFamily::Text);
    }
}
