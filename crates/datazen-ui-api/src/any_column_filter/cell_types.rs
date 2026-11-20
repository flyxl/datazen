//! Narrow cell-type projection used by the Any Column expansion.
//!
//! React `anyColumnFilter.ts` imports `resolveCellType`/`CellTypeHints` from
//! `lib/cellTypes.ts` and `cellTypesForScope`/`ValueSearchTypeGroup` from
//! `lib/valueSearch.ts`. Neither file is ported into this crate yet:
//! `cellTypes.ts` lives in `datazen-ui` (T078), and `datazen-ui` depends on
//! `datazen-ui-api` — the dependency arrow forbids reusing that port here.
//! This submodule therefore carries a faithful narrow projection of the
//! two functions the filter needs, with the same rule order and match
//! vocabulary. It is **not** a second authority: when `cellTypes.ts` /
//! `valueSearch.ts` land in this crate, `any_column_filter` should switch
//! its imports to them and this file should be deleted.
//!
//! Scope note: this projection is only used to decide "is this column
//! text-searchable under the chosen group set?" — the colour/editor/
//! zoned/read-only projections of the full type port are intentionally
//! absent.

use std::collections::BTreeSet;

/// React `CellType` (the `@datazen/extension-points` union).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellType {
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

impl CellType {
    /// The exact React literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Bool => "bool",
            Self::Date => "date",
            Self::Time => "time",
            Self::DateTime => "datetime",
            Self::Json => "json",
            Self::Bytes => "bytes",
            Self::Enum => "enum",
            Self::Uuid => "uuid",
            Self::Unknown => "unknown",
        }
    }
}

/// React `CellTypeMatch` — how a rule's pattern compares to a type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellTypeMatch {
    Exact,
    Prefix,
    Suffix,
    Substring,
    Token,
    TokenPrefix,
}

/// React `CellTypeRule`. The pattern is a `&'static str` because the shared
/// table and driver hint tables are static metadata; dynamic patterns are
/// not needed by any caller of the filter expansion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellTypeRule {
    pub match_kind: CellTypeMatch,
    pub pattern: &'static str,
    pub cell_type: CellType,
}

/// React `CellTypeHints` — the driver's dialect-specific declarations.
/// Every field defaults to "not declared" (an empty list).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellTypeHints {
    /// Evaluated before the shared table; the first hit wins.
    pub overrides: Vec<CellTypeRule>,
    /// Type names the driver reports as enumerations.
    pub enum_types: Vec<String>,
    /// Type names whose values carry a zone (kept for surface parity; the
    /// searchability decision does not use them).
    pub zoned_types: Vec<String>,
    /// Type names that cannot be edited in place (surface parity only).
    pub read_only_types: Vec<String>,
}

use CellType::{Bool, Bytes, Date, DateTime, Enum, Json, Number, Text, Time, Unknown, Uuid};
use CellTypeMatch::{Exact, Prefix, Substring, Suffix, Token, TokenPrefix};

/// React `DEFAULT_CELL_TYPE_RULES`, in evaluation order. The order is
/// load-bearing: `json` precedes the binary rules, and the explicit text
/// block (`point` contains `int`) precedes the numeric rules.
pub const DEFAULT_CELL_TYPE_RULES: &[CellTypeRule] = &[
    // Low-cardinality enumerations.
    CellTypeRule { match_kind: TokenPrefix, pattern: "enum", cell_type: Enum },
    CellTypeRule { match_kind: TokenPrefix, pattern: "set", cell_type: Enum },
    // Structured values.
    CellTypeRule { match_kind: Substring, pattern: "json", cell_type: Json },
    CellTypeRule { match_kind: Token, pattern: "array", cell_type: Json },
    CellTypeRule { match_kind: Suffix, pattern: "[]", cell_type: Json },
    // Binary.
    CellTypeRule { match_kind: TokenPrefix, pattern: "fixedstring", cell_type: Bytes },
    CellTypeRule { match_kind: Substring, pattern: "bytea", cell_type: Bytes },
    CellTypeRule { match_kind: Substring, pattern: "blob", cell_type: Bytes },
    CellTypeRule { match_kind: Substring, pattern: "binary", cell_type: Bytes },
    CellTypeRule { match_kind: Exact, pattern: "raw", cell_type: Bytes },
    CellTypeRule { match_kind: Exact, pattern: "image", cell_type: Bytes },
    // UUID.
    CellTypeRule { match_kind: TokenPrefix, pattern: "uuid", cell_type: Uuid },
    CellTypeRule { match_kind: Exact, pattern: "uniqueidentifier", cell_type: Uuid },
    // Booleans.
    CellTypeRule { match_kind: Substring, pattern: "bool", cell_type: Bool },
    // Date and time: timestamp/datetime first.
    CellTypeRule { match_kind: Substring, pattern: "timestamp", cell_type: DateTime },
    CellTypeRule { match_kind: Substring, pattern: "datetime", cell_type: DateTime },
    // PostgreSQL's `interval` is text on the wire.
    CellTypeRule { match_kind: Token, pattern: "interval", cell_type: Text },
    CellTypeRule { match_kind: Exact, pattern: "date", cell_type: Date },
    CellTypeRule { match_kind: TokenPrefix, pattern: "date", cell_type: Date },
    CellTypeRule { match_kind: Exact, pattern: "time", cell_type: Time },
    CellTypeRule { match_kind: TokenPrefix, pattern: "time", cell_type: Time },
    // Explicit text block, before the numeric rules (`point` contains `int`).
    CellTypeRule { match_kind: Token, pattern: "bit", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "varbit", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "xml", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "inet", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "cidr", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "macaddr", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "macaddr8", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "geometry", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "geography", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "point", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "polygon", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "linestring", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "hstore", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "year", cell_type: Text },
    // `money` is money-formatted text; parsing it would corrupt the value.
    CellTypeRule { match_kind: Token, pattern: "money", cell_type: Text },
    // Numbers: whole tokens only; unsigned integers get their own prefix.
    CellTypeRule { match_kind: TokenPrefix, pattern: "int", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "uint", cell_type: Number },
    CellTypeRule { match_kind: Token, pattern: "bigint", cell_type: Number },
    CellTypeRule { match_kind: Token, pattern: "smallint", cell_type: Number },
    CellTypeRule { match_kind: Token, pattern: "tinyint", cell_type: Number },
    CellTypeRule { match_kind: Token, pattern: "mediumint", cell_type: Number },
    CellTypeRule { match_kind: Substring, pattern: "serial", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "numeric", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "decimal", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "dec", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "float", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "double", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "real", cell_type: Number },
    CellTypeRule { match_kind: TokenPrefix, pattern: "number", cell_type: Number },
    // Character family.
    CellTypeRule { match_kind: TokenPrefix, pattern: "varchar", cell_type: Text },
    CellTypeRule { match_kind: TokenPrefix, pattern: "nvarchar", cell_type: Text },
    CellTypeRule { match_kind: TokenPrefix, pattern: "nchar", cell_type: Text },
    CellTypeRule { match_kind: TokenPrefix, pattern: "char", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "bpchar", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "character", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "varying", cell_type: Text },
    CellTypeRule { match_kind: TokenPrefix, pattern: "text", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "tinytext", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "mediumtext", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "longtext", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "clob", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "string", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "citext", cell_type: Text },
    CellTypeRule { match_kind: Token, pattern: "name", cell_type: Text },
];

/// React `BASE_SEARCHABLE_CELL_TYPES`: text/enum/unknown always match.
pub fn base_searchable_cell_types() -> BTreeSet<CellType> {
    [Text, Enum, Unknown].into_iter().collect()
}

/// React `ValueSearchTypeGroup` — the driver-declared extra groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueSearchTypeGroup {
    Number,
    Datetime,
}

impl ValueSearchTypeGroup {
    /// The exact React literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Datetime => "datetime",
        }
    }
}

/// React `ValueSearchTypeScope` — the four scopes the dialog offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSearchTypeScope {
    Text,
    TextNumber,
    TextDatetime,
    TextNumberDatetime,
}

/// Lower-case, trimmed type name (React `normalizeTypeName`).
fn normalize_type_name(data_type: Option<&str>) -> String {
    data_type.unwrap_or("").trim().to_lowercase()
}

/// A token is a run of letters, digits and underscores; everything else is a
/// separator. `int(11) unsigned` → `["int", "11", "unsigned"]`.
fn tokenize_type_name(type_name: &str) -> Vec<String> {
    normalize_type_name(Some(type_name))
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether one rule matches a type name (React `matchCellTypeRule`).
fn match_cell_type_rule(type_name: &str, rule: &CellTypeRule) -> bool {
    let normalized = normalize_type_name(Some(type_name));
    let pattern = rule.pattern.to_lowercase();
    match rule.match_kind {
        Exact => normalized == pattern,
        Prefix => normalized.starts_with(&pattern),
        Suffix => normalized.ends_with(&pattern),
        Substring => normalized.contains(&pattern),
        Token => tokenize_type_name(&normalized).iter().any(|token| token == &pattern),
        TokenPrefix => {
            tokenize_type_name(&normalized).iter().any(|token| token.starts_with(&pattern))
        }
    }
}

/// Driver hint names match on tokens, but may also name a whole type
/// (`user-defined`), which is not a token of it (React `matchesHintName`).
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

/// React `resolveCellType`: overrides → enum types → shared table → unknown.
pub fn resolve_cell_type(data_type: Option<&str>, hints: Option<&CellTypeHints>) -> CellType {
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

/// React `cellTypesForScope(scope)`: the base set plus the named groups'
/// cell types. `number` adds `number`; `datetime` adds `date`/`time`/
/// `datetime`. The scope string is parsed by substring in React; this
/// enum is the closed twin, so each variant names its exact contents.
pub fn cell_types_for_scope(scope: ValueSearchTypeScope) -> BTreeSet<CellType> {
    let mut types = base_searchable_cell_types();
    if matches!(scope, ValueSearchTypeScope::TextNumber | ValueSearchTypeScope::TextNumberDatetime)
    {
        types.insert(Number);
    }
    if matches!(scope, ValueSearchTypeScope::TextDatetime | ValueSearchTypeScope::TextNumberDatetime)
    {
        types.extend([Date, Time, DateTime]);
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_sets_are_the_exact_react_sets() {
        let base = base_searchable_cell_types();
        assert_eq!(base, [Text, Enum, Unknown].into_iter().collect());
        assert_eq!(cell_types_for_scope(ValueSearchTypeScope::Text), base);
        let number = cell_types_for_scope(ValueSearchTypeScope::TextNumber);
        assert_eq!(number, [Text, Enum, Unknown, Number].into_iter().collect());
        let datetime = cell_types_for_scope(ValueSearchTypeScope::TextDatetime);
        assert_eq!(
            datetime,
            [Text, Enum, Unknown, Date, Time, DateTime].into_iter().collect()
        );
        let both = cell_types_for_scope(ValueSearchTypeScope::TextNumberDatetime);
        assert_eq!(
            both,
            [Text, Enum, Unknown, Number, Date, Time, DateTime].into_iter().collect()
        );
    }

    #[test]
    fn group_spellings_are_the_react_literals() {
        assert_eq!(ValueSearchTypeGroup::Number.as_str(), "number");
        assert_eq!(ValueSearchTypeGroup::Datetime.as_str(), "datetime");
    }

    #[test]
    fn base_types_resolve_to_the_always_searchable_family() {
        assert_eq!(resolve_cell_type(Some("VARCHAR(32)"), None), Text);
        assert_eq!(resolve_cell_type(Some("text"), None), Text);
        assert_eq!(resolve_cell_type(Some("enum('a','b')"), None), Enum);
        assert_eq!(resolve_cell_type(Some("USER-DEFINED"), None), Unknown);
        // A name with no matching rule is unknown (still text-searchable).
        assert_eq!(resolve_cell_type(Some("mystery_type"), None), Unknown);
        assert_eq!(resolve_cell_type(None, None), Unknown);
        assert_eq!(resolve_cell_type(Some("  "), None), Unknown);
    }

    #[test]
    fn the_explicit_text_block_beats_the_numeric_rules() {
        // `point` contains `int`; `money` looks numeric but is text.
        for name in ["point", "polygon", "money", "inet", "geometry", "year", "bit"] {
            assert_eq!(resolve_cell_type(Some(name), None), Text, "{name}");
        }
        // A trailing `[]` makes an array JSON, not text.
        assert_eq!(resolve_cell_type(Some("text[]"), None), Json);
    }

    #[test]
    fn numeric_and_temporal_types_resolve_to_group_gated_families() {
        assert_eq!(resolve_cell_type(Some("int(11) unsigned"), None), Number);
        assert_eq!(resolve_cell_type(Some("uint64"), None), Number);
        assert_eq!(resolve_cell_type(Some("numeric(10,2)"), None), Number);
        assert_eq!(resolve_cell_type(Some("timestamp"), None), DateTime);
        assert_eq!(resolve_cell_type(Some("timestamptz"), None), DateTime);
        assert_eq!(resolve_cell_type(Some("date"), None), Date);
        assert_eq!(resolve_cell_type(Some("time"), None), Time);
        // timestamp/datetime come first, so the date/time rules do not steal.
        assert_eq!(resolve_cell_type(Some("datetime"), None), DateTime);
    }

    #[test]
    fn skipped_families_resolve_to_their_non_searchable_types() {
        assert_eq!(resolve_cell_type(Some("boolean"), None), Bool);
        assert_eq!(resolve_cell_type(Some("jsonb"), None), Json);
        assert_eq!(resolve_cell_type(Some("bytea"), None), Bytes);
        assert_eq!(resolve_cell_type(Some("blob"), None), Bytes);
        assert_eq!(resolve_cell_type(Some("uuid"), None), Uuid);
        assert_eq!(resolve_cell_type(Some("uniqueidentifier"), None), Uuid);
    }

    #[test]
    fn driver_overrides_win_then_enum_types_then_the_shared_table() {
        let hints = CellTypeHints {
            overrides: vec![CellTypeRule {
                match_kind: Token,
                pattern: "timestamp",
                cell_type: Bytes,
            }],
            enum_types: vec![String::from("user-defined")],
            ..CellTypeHints::default()
        };
        // SQL Server's `timestamp` is a rowversion, not a time.
        assert_eq!(resolve_cell_type(Some("timestamp"), Some(&hints)), Bytes);
        // A whole-name hint names an enum even though it is not a token.
        assert_eq!(resolve_cell_type(Some("USER-DEFINED"), Some(&hints)), Enum);
        // Without hints the same names take their shared-table answers.
        assert_eq!(resolve_cell_type(Some("timestamp"), None), DateTime);
        assert_eq!(resolve_cell_type(Some("USER-DEFINED"), None), Unknown);
    }

    #[test]
    fn as_str_pins_every_cell_type_literal() {
        for (cell_type, literal) in [
            (Text, "text"),
            (Number, "number"),
            (Bool, "bool"),
            (Date, "date"),
            (Time, "time"),
            (DateTime, "datetime"),
            (Json, "json"),
            (Bytes, "bytes"),
            (Enum, "enum"),
            (Uuid, "uuid"),
            (Unknown, "unknown"),
        ] {
            assert_eq!(cell_type.as_str(), literal);
        }
    }
}
