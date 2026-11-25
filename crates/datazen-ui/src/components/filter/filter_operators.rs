//! Filter operator metadata and value conversions, ported 1:1 from
//! `packages/frontend/src/lib/filterOperators.ts`.
//!
//! Everything the filter editor needs to know about an operator lives in one
//! table: which value control it needs, which column types it applies to,
//! whether it is a dialect operator, and whether its value is a LIKE pattern or
//! a literal. Keeping that as **data** is the point: adding an operator means
//! adding one row here, not touching three branches in the editor.
//!
//! The dispatch key is the column's **normalised cell type** — the host's
//! single type judgement (`crate::support::cell_types::resolve_cell_type`).
//! Dispatching on render families instead would fold `enum` and `uuid` back
//! into `text`, which offers a `LIKE` on an enumeration column: an entry the
//! database is guaranteed to reject.
//!
//! ## Home and removal stage
//!
//! The React file is `lib/filterOperators.ts`; its permanent Rune home is
//! `support/filter_operators.rs` (T513). It lives next to the editor until
//! then because T056 owns `components/filter/` and
//! [`super::filter_condition_editor::FilterConditionEditor`] is its only
//! consumer. T513 moves this file and deletes the copy.
//!
//! ## What is not ported
//!
//! `supportedOperatorsForMeta` / `likeCaseInsensitiveForMeta` read
//! `DatabaseTypeMeta.dataGrid`, which is driver metadata the filter panel
//! (T024) reads when it assembles the row; the editor receives the resulting
//! `supportedOperators` / `likeCaseInsensitive` as props, exactly as React
//! does.

// Dead-code lint is silenced per item below, each naming T513 (the permanent
// home of this lib/filterOperators.ts port) as its removal stage.

use super::filter_condition_chip::FilterValue;
use crate::support::cell_types::{resolve_cell_type, CellType, CellTypeHints};

/// Which control the value of a condition is edited with.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterValueInputKind {
    /// The operator carries no value (`isNull` / `isNotNull` / `isEmpty` /
    /// `isNotEmpty`); the editor renders no value field at all.
    None,
    /// One literal.
    Single,
    /// Two literals (`between` / `notBetween`).
    Pair,
    /// A list of literals (`in` / `notIn`).
    Tokens,
}

/// How the operator is grouped in the UI and in the documentation.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOperatorGroup {
    /// Value comparison.
    Compare,
    /// Text patterns.
    Pattern,
    /// Membership.
    Set,
    /// Null checks.
    Null,
    /// Empty-string checks.
    Empty,
    /// Only a driver can render these.
    Dialect,
}

/// One row of the operator table.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
#[derive(Clone, Copy, Debug)]
pub struct FilterOperatorMeta {
    /// The operator this entry describes.
    pub id: &'static str,
    /// Which value control the editor renders for it.
    pub value_input: FilterValueInputKind,
    /// Only a driver that declares the operator can render it.
    pub dialect: bool,
    /// UI grouping.
    pub group: FilterOperatorGroup,
    /// Whether the value is a LIKE **pattern** (the user writes the wildcards)
    /// or a **literal** (the back end escapes them).
    pub escapes_like_pattern: bool,
}

#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
impl FilterOperatorMeta {
    /// The cell types the operator applies to.
    ///
    /// React derives this field from the matrix, so the two can never drift
    /// apart; Rust keeps the same rule as a method rather than a stored copy
    /// (a `const` table cannot call a function).
    pub fn cell_types(&self) -> Vec<CellType> {
        cell_types_for_operator(self.id)
    }
}

const fn meta(
    id: &'static str,
    value_input: FilterValueInputKind,
    group: FilterOperatorGroup,
    escapes_like_pattern: bool,
) -> FilterOperatorMeta {
    FilterOperatorMeta {
        id,
        value_input,
        dialect: matches!(group, FilterOperatorGroup::Dialect),
        group,
        escapes_like_pattern,
    }
}

/// Every cell type, in the order the matrix lists them.
const ALL_CELL_TYPES: [CellType; 11] = [
    CellType::Text,
    CellType::Enum,
    CellType::Number,
    CellType::Bool,
    CellType::Date,
    CellType::Time,
    CellType::DateTime,
    CellType::Json,
    CellType::Bytes,
    CellType::Uuid,
    CellType::Unknown,
];

/// The operators each **cell type** offers, in display order.
///
/// The order of the operators that already shipped is frozen
/// (`eq ne gt lt gte lte like in isNull isNotNull`): every list keeps that
/// order inside itself, so adding an operator never reshuffles the ones a user
/// already knows.
///
/// `enum` and `uuid` have their own lists instead of sharing the text one: a
/// `LIKE` against an enumeration column needs an explicit cast in PostgreSQL,
/// so offering it is offering an entry the database rejects.
const OPERATORS_BY_CELL_TYPE: [&[&str]; 11] = [
    // text
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "like",
        "contains",
        "notContains",
        "startsWith",
        "endsWith",
        "in",
        "notIn",
        "isNull",
        "isNotNull",
        "isEmpty",
        "isNotEmpty",
    ],
    // enum
    &["eq", "ne", "in", "notIn", "isNull", "isNotNull"],
    // number
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "between",
        "notBetween",
        "in",
        "notIn",
        "isNull",
        "isNotNull",
    ],
    // bool
    &["eq", "ne", "isNull", "isNotNull"],
    // date
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "between",
        "notBetween",
        "isNull",
        "isNotNull",
    ],
    // time
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "between",
        "notBetween",
        "isNull",
        "isNotNull",
    ],
    // datetime
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "between",
        "notBetween",
        "isNull",
        "isNotNull",
    ],
    // json
    &["eq", "ne", "isNull", "isNotNull"],
    // bytes
    &["eq", "ne", "isNull", "isNotNull"],
    // uuid
    &["eq", "ne", "in", "notIn", "isNull", "isNotNull"],
    // unknown: handled as text, never as an error
    &[
        "eq",
        "ne",
        "gt",
        "lt",
        "gte",
        "lte",
        "like",
        "contains",
        "notContains",
        "startsWith",
        "endsWith",
        "in",
        "notIn",
        "isNull",
        "isNotNull",
        "isEmpty",
        "isNotEmpty",
    ],
];

/// The dialect operators a column type may use, in matrix order.
///
/// They are never part of a family's list: they only show up when the driver
/// declares them, and they are appended after the shared ones.
const DIALECT_OPERATORS_BY_CELL_TYPE: [&[&str]; 11] = [
    &["regex", "notRegex"], // text
    &[],                    // enum
    &[],                    // number
    &[],                    // bool
    &[],                    // date
    &[],                    // time
    &[],                    // datetime
    &["jsonContains"],      // json
    &[],                    // bytes
    &[],                    // uuid
    &["regex", "notRegex"], // unknown
];

/// The operator table: one entry for each of the 22 operators.
///
/// A missing entry would make the editor fall back to "a plain text box" for an
/// operator that needs two boxes or a chip list.
const FILTER_OPERATOR_META: [FilterOperatorMeta; 22] = [
    // ── comparison ──────────────────────────────────────────────────────────
    meta("eq", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("ne", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("gt", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("lt", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("gte", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("lte", FilterValueInputKind::Single, FilterOperatorGroup::Compare, false),
    meta("between", FilterValueInputKind::Pair, FilterOperatorGroup::Compare, false),
    meta("notBetween", FilterValueInputKind::Pair, FilterOperatorGroup::Compare, false),
    // ── patterns ───────────────────────────────────────────────────────────
    // `like` takes the wildcards from the user; the four below escape them.
    meta("like", FilterValueInputKind::Single, FilterOperatorGroup::Pattern, false),
    meta("contains", FilterValueInputKind::Single, FilterOperatorGroup::Pattern, true),
    meta("notContains", FilterValueInputKind::Single, FilterOperatorGroup::Pattern, true),
    meta("startsWith", FilterValueInputKind::Single, FilterOperatorGroup::Pattern, true),
    meta("endsWith", FilterValueInputKind::Single, FilterOperatorGroup::Pattern, true),
    // ── membership ─────────────────────────────────────────────────────────
    meta("in", FilterValueInputKind::Tokens, FilterOperatorGroup::Set, false),
    meta("notIn", FilterValueInputKind::Tokens, FilterOperatorGroup::Set, false),
    // ── null / empty ───────────────────────────────────────────────────────
    meta("isNull", FilterValueInputKind::None, FilterOperatorGroup::Null, false),
    meta("isNotNull", FilterValueInputKind::None, FilterOperatorGroup::Null, false),
    meta("isEmpty", FilterValueInputKind::None, FilterOperatorGroup::Empty, false),
    meta("isNotEmpty", FilterValueInputKind::None, FilterOperatorGroup::Empty, false),
    // ── dialect specific ───────────────────────────────────────────────────
    meta("regex", FilterValueInputKind::Single, FilterOperatorGroup::Dialect, false),
    meta("notRegex", FilterValueInputKind::Single, FilterOperatorGroup::Dialect, false),
    meta("jsonContains", FilterValueInputKind::Single, FilterOperatorGroup::Dialect, false),
];

/// The row of the table that describes `operator`.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn filter_operator_meta(operator: &str) -> Option<&'static FilterOperatorMeta> {
    FILTER_OPERATOR_META.iter().find(|entry| entry.id == operator)
}

/// The one static operator id that spells `name`.
///
/// Select choices arrive as `&str` borrowed from a temporary label, while a
/// [`super::filter_condition_chip::FilterCondition`] stores `&'static str`.
/// This is the bridge: the table is the closed set, so an unknown name can
/// only come from a caller-supplied operator that is already `&'static`.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn static_operator(name: &str) -> Option<&'static str> {
    FILTER_OPERATOR_META
        .iter()
        .map(|entry| entry.id)
        .find(|id| *id == name)
}

/// The matrix row of `cell`.
fn row_index(cell: CellType) -> usize {
    ALL_CELL_TYPES
        .iter()
        .position(|candidate| *candidate == cell)
        .unwrap_or(ALL_CELL_TYPES.len() - 1)
}

/// The operators `cell` offers, in display order (the shared matrix row).
fn operators_by_cell_type(cell: CellType) -> &'static [&'static str] {
    OPERATORS_BY_CELL_TYPE[row_index(cell)]
}

/// The dialect operators `cell` may use (the dialect matrix row).
fn dialect_operators_by_cell_type(cell: CellType) -> &'static [&'static str] {
    DIALECT_OPERATORS_BY_CELL_TYPE[row_index(cell)]
}

/// The cell types that offer an operator — the inverse of the matrix.
///
/// It is **derived**, never written a second time: the matrix is the only place
/// that says which operator belongs to which type.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn cell_types_for_operator(operator: &str) -> Vec<CellType> {
    ALL_CELL_TYPES
        .iter()
        .copied()
        .filter(|cell| {
            operators_by_cell_type(*cell).contains(&operator)
                || dialect_operators_by_cell_type(*cell).contains(&operator)
        })
        .collect()
}

/// The operators a column may be filtered with.
///
/// `data_type` is the driver's type string for the column; `supported` is the
/// driver's whitelist. The result is the intersection of "the column type can
/// take it" and "this database serves it", in a stable order.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn operators_for_column(
    data_type: Option<&str>,
    supported: &[&str],
    hints: Option<&CellTypeHints>,
) -> Vec<&'static str> {
    let cell_type = resolve_cell_type(data_type, hints);
    operators_by_cell_type(cell_type)
        .iter()
        .chain(dialect_operators_by_cell_type(cell_type).iter())
        .copied()
        .filter(|operator| supported.contains(operator))
        .collect()
}

/// Which value control an operator is edited with.
///
/// A condition that was persisted by another build may name an operator this
/// build does not know. The editor then degrades to a single value field
/// instead of throwing while it renders — an operator the user cannot change
/// anyway is better than a panel that fails to open.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn value_input_for(operator: &str) -> FilterValueInputKind {
    filter_operator_meta(operator)
        .map(|meta| meta.value_input)
        .unwrap_or(FilterValueInputKind::Single)
}

/// The operator a condition falls back to when its current one does not fit the
/// column any more (the user picked another column, or the driver dropped the
/// operator).
///
/// The value is kept: changing the column is usually "same value, other
/// column", and throwing the value away would lose the user's work.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn normalize_operator_for_column(
    current: &'static str,
    data_type: Option<&str>,
    supported: &[&str],
    hints: Option<&CellTypeHints>,
) -> &'static str {
    let allowed = operators_for_column(data_type, supported, hints);
    if allowed.contains(&current) {
        return current;
    }
    if allowed.contains(&"eq") {
        return "eq";
    }
    allowed.first().copied().unwrap_or(current)
}

/// The scalar form of a value; React's `asText` answers `''` for an array.
fn as_text(value: &FilterValue) -> String {
    match value {
        FilterValue::Single(text) => text.clone(),
        FilterValue::None | FilterValue::Pair(_, _) | FilterValue::Tokens(_) => String::new(),
    }
}

/// The list form of a value; React's `asTokens` splits a comma string.
fn as_tokens(value: &FilterValue) -> Vec<String> {
    match value {
        FilterValue::Tokens(tokens) => tokens.clone(),
        FilterValue::None => Vec::new(),
        FilterValue::Pair(lower, upper) => vec![lower.clone(), upper.clone()],
        FilterValue::Single(text) => split_tokens(text),
    }
}

/// Split a comma separated value the way React's `asTokens` does.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn split_tokens(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The pair form of a value; React's `asPair` keeps an unsplit scalar whole.
fn as_pair(value: &FilterValue) -> (String, String) {
    match value {
        FilterValue::Pair(lower, upper) => (lower.clone(), upper.clone()),
        FilterValue::Tokens(tokens) => (
            tokens.first().cloned().unwrap_or_default(),
            tokens.get(1).cloned().unwrap_or_default(),
        ),
        FilterValue::None => (String::new(), String::new()),
        FilterValue::Single(text) => match text.split_once(',') {
            None => (text.clone(), String::new()),
            Some((lower, upper)) => (lower.trim().to_owned(), upper.trim().to_owned()),
        },
    }
}

/// Convert a value when the operator changes and the value control changes with
/// it.
///
/// The conversions keep what the user typed as far as the new control can hold
/// it: a single value becomes the lower bound of a pair, a pair becomes the
/// lower bound of a single value, a token list becomes a comma separated text.
/// Switching to an operator that carries no value keeps the old value, so
/// switching back restores it.
#[allow(dead_code)] // T513 moves this port to support/filter_operators.rs.
pub fn convert_filter_value(
    from: FilterValueInputKind,
    to: FilterValueInputKind,
    value: &FilterValue,
) -> FilterValue {
    if from == to {
        return value.clone();
    }
    if to == FilterValueInputKind::None || from == FilterValueInputKind::None {
        return value.clone();
    }

    match to {
        FilterValueInputKind::Tokens => {
            let tokens = if from == FilterValueInputKind::Pair {
                let (lower, upper) = as_pair(value);
                [lower, upper]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect()
            } else {
                as_tokens(value)
            };
            FilterValue::Tokens(tokens)
        }
        FilterValueInputKind::Pair => {
            let (lower, upper) = if from == FilterValueInputKind::Tokens {
                let tokens = as_tokens(value);
                (
                    tokens.first().cloned().unwrap_or_default(),
                    tokens.get(1).cloned().unwrap_or_default(),
                )
            } else {
                (as_text(value), String::new())
            };
            FilterValue::Pair(lower, upper)
        }
        FilterValueInputKind::Single => {
            if from == FilterValueInputKind::Tokens {
                FilterValue::Single(as_tokens(value).join(", "))
            } else if from == FilterValueInputKind::Pair {
                FilterValue::Single(as_pair(value).0)
            } else {
                value.clone()
            }
        }
        FilterValueInputKind::None => value.clone(),
    }
}
