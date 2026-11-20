//! Port of `packages/frontend/src/lib/sqlDialects/identifierQuoting.ts`.
//!
//! Identifier quoting and folding — pure functions over a resolved dialect
//! profile. They live below both the SQL editor and the Visual Query
//! Builder so that anything deriving a relation key folds and quotes
//! identifiers identically: the metadata cache is keyed by the folded
//! identity, so a second implementation would silently split one physical
//! relation into two cache cells (the doc note the React file opens with).
//!
//! ## What maps where
//!
//! - React's `Set<string>` keyword table is a sorted `&[&str]` searched
//!   with `binary_search` (the crate's `i18n.rs` pattern); membership is
//!   what matters, not order.
//! - `needsQuoting`'s `/^[A-Za-z_][A-Za-z0-9_$]*$/` test is a manual
//!   scan over ASCII (no regex engine in this crate).
//! - `foldIdentifier` takes the fold case directly (the primitive);
//!   `dialect_profile::fold_identifier` resolves the profile and calls
//!   here, so the fold exists exactly once.
//! - `quoteIdentifierForDialect` honours `quoteStyle: 'none'` — the
//!   passthrough — which is why [`QuoteStyle`] carries the `None`
//!   variant even though no built-in profile uses it (a driver may
//!   declare it).
//!
//! Known deltas: none behavioural — every keyword, rule and escaping
//! string is the React file's verbatim.

use super::dialect_profile::{FoldCase, QuoteStyle, SqlDialectProfile};

/// React `SQL_RESERVED_IDENTIFIER_KEYWORDS` — the reserved words that
/// force quoting, in the React file's (alphabetical) order so the
/// `binary_search` membership test stays honest (a test pins sortedness).
pub(crate) const SQL_RESERVED_IDENTIFIER_KEYWORDS: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "both",
    "case",
    "cast",
    "check",
    "collate",
    "collation",
    "column",
    "constraint",
    "create",
    "cross",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "delete",
    "desc",
    "describe",
    "distinct",
    "do",
    "drop",
    "else",
    "end",
    "except",
    "false",
    "fetch",
    "for",
    "foreign",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "index",
    "initially",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "isnull",
    "join",
    "key",
    "keys",
    "lateral",
    "leading",
    "left",
    "like",
    "limit",
    "localtime",
    "localtimestamp",
    "lock",
    "natural",
    "not",
    "notnull",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "outer",
    "overlaps",
    "placing",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "session_user",
    "set",
    "similar",
    "some",
    "symmetric",
    "system_user",
    "table",
    "tablesample",
    "then",
    "to",
    "trailing",
    "true",
    "union",
    "unique",
    "update",
    "user",
    "using",
    "values",
    "variadic",
    "verbose",
    "when",
    "where",
    "window",
    "with",
];

/// Membership in the reserved-keyword table (`Set.has`).
pub(crate) fn is_reserved_keyword(word: &str) -> bool {
    SQL_RESERVED_IDENTIFIER_KEYWORDS.binary_search(&word).is_ok()
}

/// React `foldIdentifier`: fold an unquoted identifier the way the
/// dialect does (`lower` / `upper` / preserve).
pub(crate) fn fold_identifier(value: &str, fold_case: FoldCase) -> String {
    match fold_case {
        FoldCase::Lower => value.to_lowercase(),
        FoldCase::Upper => value.to_uppercase(),
        FoldCase::Preserve => value.to_owned(),
    }
}

/// `/^[A-Za-z_][A-Za-z0-9_$]*$/` — a plain SQL identifier spelling.
fn is_plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// React `needsQuoting`: whether the identifier must be quoted for the
/// dialect — empty, non-plain spellings, reserved words (the shared table
/// plus the dialect's extras), and case that folding would change.
pub(crate) fn needs_quoting(
    value: &str,
    quote_style: QuoteStyle,
    fold_case: FoldCase,
    extra_reserved_keywords: Option<&[String]>,
) -> bool {
    if value.is_empty() {
        return true;
    }
    if quote_style == QuoteStyle::None {
        return false;
    }
    if !is_plain_identifier(value) {
        return true;
    }
    let lower = value.to_lowercase();
    if is_reserved_keyword(&lower) {
        return true;
    }
    if let Some(extra) = extra_reserved_keywords {
        if extra.iter().any(|kw| kw == &lower) {
            return true;
        }
    }
    if fold_case == FoldCase::Lower && value.chars().any(|c| c.is_ascii_uppercase()) {
        return true;
    }
    if fold_case == FoldCase::Upper && value.chars().any(|c| c.is_ascii_lowercase()) {
        return true;
    }
    false
}

/// React `escapeForQuote`: the doubling each quote style applies inside
/// the wrapper (backtick and `none` style have no escape).
fn escape_for_quote(value: &str, style: QuoteStyle) -> String {
    match style {
        QuoteStyle::Double => value.replace('"', "\"\""),
        QuoteStyle::Bracket => value.replace(']', "]]"),
        QuoteStyle::Backtick | QuoteStyle::None => value.to_owned(),
    }
}

/// React `wrapQuoted`: the identifier wrapped in the style's delimiters.
pub(crate) fn wrap_quoted(value: &str, style: QuoteStyle) -> String {
    let escaped = escape_for_quote(value, style);
    match style {
        QuoteStyle::Double => format!("\"{escaped}\""),
        QuoteStyle::Backtick => format!("`{escaped}`"),
        QuoteStyle::Bracket => format!("[{escaped}]"),
        QuoteStyle::None => value.to_owned(),
    }
}

/// React `quoteIdentifierForDialect`: quote an identifier for a dialect,
/// honouring `quoteStyle: 'none'` (the passthrough).
pub(crate) fn quote_identifier_for_dialect(value: &str, profile: &SqlDialectProfile) -> String {
    if profile.quote_style == QuoteStyle::None {
        return value.to_owned();
    }
    wrap_quoted(value, profile.quote_style)
}

pub(crate) mod tests {
    use super::*;

    #[test]
    fn keyword_table_is_sorted_and_complete() {
        // binary_search only answers membership on a sorted slice; pin it
        // so a future keyword insertion cannot silently break lookups.
        let sorted = SQL_RESERVED_IDENTIFIER_KEYWORDS.windows(2).all(|w| w[0] < w[1]);
        assert!(sorted, "the reserved-keyword table must stay sorted");
        assert_eq!(SQL_RESERVED_IDENTIFIER_KEYWORDS.len(), 108);
        // Spot rows from both halves plus a near-miss.
        assert!(is_reserved_keyword("select"));
        assert!(is_reserved_keyword("with"));
        assert!(is_reserved_keyword("current_timestamp"));
        assert!(!is_reserved_keyword("selection"));
        assert!(!is_reserved_keyword("Select"), "lookup is lowercase-exact");
    }

    #[test]
    fn folding_follows_the_case() {
        assert_eq!(fold_identifier("Users", FoldCase::Lower), "users");
        assert_eq!(fold_identifier("users", FoldCase::Upper), "USERS");
        assert_eq!(fold_identifier("Users", FoldCase::Preserve), "Users");
    }

    #[test]
    fn needs_quoting_covers_every_rule() {
        // Empty always quotes; 'none' style never quotes.
        assert!(needs_quoting("", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(!needs_quoting("order", QuoteStyle::None, FoldCase::Lower, None));
        // Non-plain spellings quote.
        assert!(needs_quoting("user table", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(needs_quoting("1users", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(needs_quoting("user-id", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(!needs_quoting("user_1", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(!needs_quoting("a$b", QuoteStyle::Double, FoldCase::Lower, None));
        // Reserved words quote, whatever their casing (checked lowercased).
        assert!(needs_quoting("SELECT", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(needs_quoting("select", QuoteStyle::Backtick, FoldCase::Lower, None));
        // Extra reserved words quote only when provided.
        assert!(!needs_quoting("snapshot", QuoteStyle::Double, FoldCase::Lower, None));
        let extras = vec!["snapshot".to_string()];
        assert!(needs_quoting("snapshot", QuoteStyle::Double, FoldCase::Lower, Some(&extras)));
        assert!(needs_quoting("SNAPSHOT", QuoteStyle::Double, FoldCase::Lower, Some(&extras)));
        // Case that folding would change quotes.
        assert!(needs_quoting("Users", QuoteStyle::Double, FoldCase::Lower, None));
        assert!(!needs_quoting("USERS", QuoteStyle::Double, FoldCase::Upper, None));
        assert!(needs_quoting("users", QuoteStyle::Double, FoldCase::Upper, None));
        // Preserve folds nothing, so plain names never quote.
        assert!(!needs_quoting("Users", QuoteStyle::Bracket, FoldCase::Preserve, None));
    }

    #[test]
    fn wrap_quoted_doubles_the_inner_delimiter() {
        assert_eq!(wrap_quoted("us\"er", QuoteStyle::Double), "\"us\"\"er\"");
        assert_eq!(wrap_quoted("us]er", QuoteStyle::Bracket), "[us]]er]");
        assert_eq!(wrap_quoted("us`er", QuoteStyle::Backtick), "`us`er`");
        assert_eq!(wrap_quoted("plain", QuoteStyle::Double), "\"plain\"");
        // 'none' returns the value untouched.
        assert_eq!(wrap_quoted("plain", QuoteStyle::None), "plain");
    }

    #[test]
    fn quote_for_dialect_honours_none() {
        let mut profile = SqlDialectProfile {
            quote_style: QuoteStyle::None,
            ..super::super::dialect_profile::STANDARD_DIALECT_PROFILE.clone()
        };
        assert_eq!(quote_identifier_for_dialect("Users", &profile), "Users");
        profile.quote_style = QuoteStyle::Backtick;
        assert_eq!(quote_identifier_for_dialect("Users", &profile), "`Users`");
    }
}
