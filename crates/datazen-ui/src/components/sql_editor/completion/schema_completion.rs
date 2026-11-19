//! Alias-aware schema completion, slice 1 — the pure core over the
//! metadata snapshot and the token stream.
//!
//! Port of `components/sql-editor/completion/schemaCompletion.ts` (T062,
//! split). Everything here is what the React module builds completions
//! FROM, before the semantic model and the editor schema tree get a say:
//!
//! * the completion item shape (`SchemaCompletionItem` — CodeMirror's
//!   `Completion` plus `filterText`),
//! * the dialect facade the completion quotes and folds through,
//! * `columnCompletionsFromRelation` (one relation's columns, three
//!   quoting policies),
//! * `relationCompletionsFromSnapshot` (table completions with
//!   schema/database qualifier filtering and related-table boosts),
//! * `relationKeyFromLabel` (label → snapshot key, for dedup),
//! * `allColumnsFromSnapshot` (the no-FROM fallback and its dedup rules),
//! * `extractSelectColumns` (bare column names out of the SELECT clause
//!   token stream),
//! * `filterRelationsByColumns` (tables containing ALL the selected
//!   columns).
//!
//! The entry points that need the semantic model / schema tree
//! (`resolveAliasDotCompletions`, `resolveSchemaDotCompletions`,
//! `produceSchemaCompletions`, `createSchemaCompletionSource`) land with
//! S2/S3 — see the parent module's docs.

use std::collections::BTreeMap;

use crate::relation_metadata::identity::{
    EditorRelationKind, QualifiedRelationId, SqlIdentifierSegment,
};
use crate::relation_metadata::schema::{
    ColumnSchema, EditorMetadataSnapshot, EditorRelationMetadata,
};
use crate::support::sql_dialects::dialect_profile::{
    FoldCase, QuoteStyle, SqlDialectProfile, fold_identifier, resolve_dialect_profile,
};

use super::super::sql_editor::CompletionQuotePolicy;

/* -------------------------------------------------------------------------- */
/*  Token stream shapes (extension-points `sql-editor/tokens.ts`, carried)    */
/* -------------------------------------------------------------------------- */

/// Lexical token kinds produced by the unified SQL scanner
/// (`extension-points/src/sql-editor/tokens.ts`). Carried here because the
/// SELECT-column extractor reads token streams; the scanner itself is its
/// own ledger task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SqlTokenKind {
    Whitespace,
    Semicolon,
    LineComment,
    BlockComment,
    SingleQuoted,
    DoubleQuoted,
    BacktickQuoted,
    BracketQuoted,
    DollarQuoted,
    OpenParen,
    CloseParen,
    /// Any other contiguous run (identifiers, operators, numbers, etc.).
    Other,
}

/// One scanner token. `paren_depth` is the nesting depth the token sits at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SqlToken {
    pub kind: SqlTokenKind,
    pub text: String,
    pub paren_depth: u32,
}

impl SqlToken {
    /// Shorthand for an `Other` token at depth 0.
    pub(crate) fn word(text: &str) -> Self {
        Self {
            kind: SqlTokenKind::Other,
            text: text.to_string(),
            paren_depth: 0,
        }
    }

    /// Shorthand for a `Whitespace` token at depth 0.
    pub(crate) fn space() -> Self {
        Self {
            kind: SqlTokenKind::Whitespace,
            text: " ".to_string(),
            paren_depth: 0,
        }
    }
}

/* -------------------------------------------------------------------------- */
/*  Completion item shape (CodeMirror `Completion`, as the sources build it)  */
/* -------------------------------------------------------------------------- */

/// The `Completion.type` values the schema sources produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaCompletionKind {
    /// Columns (`'property'`).
    Property,
    /// Tables and schemas (`'type'`).
    Type,
    /// Aliases (`'variable'`).
    Variable,
}

/// One completion item: CodeMirror's `Completion` plus the `filterText`
/// extension the schema sources rely on. `apply` carries the full
/// replacement text (qualifier-prefixed when the policy asks); `boost`
/// ranks without excluding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SchemaCompletionItem {
    pub label: String,
    pub filter_text: Option<String>,
    pub kind: SchemaCompletionKind,
    pub detail: Option<String>,
    pub apply: Option<String>,
    pub boost: Option<i32>,
}

/* -------------------------------------------------------------------------- */
/*  Dialect facade (adapter surface over a resolved profile)                  */
/* -------------------------------------------------------------------------- */

/// Reserved words that force quoting
/// (`lib/sqlDialects/identifierQuoting.ts`'s
/// `SQL_RESERVED_IDENTIFIER_KEYWORDS`, carried until T084 absorbs it).
const SQL_RESERVED_IDENTIFIER_KEYWORDS: &[&str] = &[
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

/// The adapter surface the completion quotes and folds through
/// (`SqlDialectAdapter`'s completion-facing half, over a resolved
/// `SqlDialectProfile`). One value per dialect id; clone-cheap.
#[derive(Clone, Debug)]
pub(crate) struct SqlCompletionDialect {
    profile: SqlDialectProfile,
}

impl SqlCompletionDialect {
    /// `getDialectAdapter(dialectId)` — unknown ids resolve to `standard`,
    /// the React fallback.
    pub(crate) fn for_dialect_id(dialect_id: &str) -> Self {
        Self {
            profile: resolve_dialect_profile(dialect_id, None),
        }
    }

    /// `adapter.quoteIdentifier(name)` — wrap in the dialect's quote style,
    /// escaping the closer.
    pub(crate) fn quote_identifier(&self, name: &str) -> String {
        match self.profile.quote_style {
            QuoteStyle::Double => format!("\"{}\"", name.replace('"', "\"\"")),
            QuoteStyle::Backtick => format!("`{name}`"),
            QuoteStyle::Bracket => format!("[{}]", name.replace(']', "]]")),
        }
    }

    /// `adapter.foldUnquotedIdentifier(name)` — fold the way the dialect's
    /// parser folds unquoted identifiers.
    pub(crate) fn fold_unquoted_identifier(&self, name: &str) -> String {
        fold_identifier(name, &self.profile)
    }

    /// `adapter.compareIdentifiers(a, b)` — equality after folding both
    /// sides.
    pub(crate) fn compare_identifiers(&self, a: &str, b: &str) -> bool {
        self.fold_unquoted_identifier(a) == self.fold_unquoted_identifier(b)
    }

    /// `adapter.shouldQuoteIdentifier(name)` — `needsQuoting` from
    /// `lib/sqlDialects/identifierQuoting.ts` (T084's file; carried here
    /// until that task lands and absorbs it): empty and non-plain names
    /// quote, reserved words quote (the dialect's extras included), and a
    /// fold case that would change the spelling quotes.
    pub(crate) fn should_quote_identifier(&self, name: &str) -> bool {
        if name.is_empty() {
            return true;
        }
        let bytes = name.as_bytes();
        let first = bytes[0];
        let plain = (first.is_ascii_alphabetic() || first == b'_')
            && bytes[1..]
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$');
        if !plain {
            return true;
        }
        let lower = name.to_lowercase();
        if SQL_RESERVED_IDENTIFIER_KEYWORDS.contains(&lower.as_str()) {
            return true;
        }
        if self
            .profile
            .reserved_keywords
            .as_ref()
            .is_some_and(|extras| extras.iter().any(|k| *k == lower))
        {
            return true;
        }
        match self.profile.fold_case {
            FoldCase::Lower => name.bytes().any(|b| b.is_ascii_uppercase()),
            FoldCase::Upper => name.bytes().any(|b| b.is_ascii_lowercase()),
            FoldCase::Preserve => false,
        }
    }
}

/* -------------------------------------------------------------------------- */
/*  Constants                                                                 */
/* -------------------------------------------------------------------------- */

/// Max completion items built by the all-columns fallback for a short
/// prefix. CodeMirror renders at most `maxRenderedOptions` (50); 300 gives
/// the fuzzy matcher headroom while bounding object construction and
/// quote/dialect work.
pub(crate) const SHORT_PREFIX_COLUMN_CAP: usize = 300;

/// Prefix length at/below which the cap applies (implicit typing only).
pub(crate) const SHORT_PREFIX_LEN: usize = 2;

/* -------------------------------------------------------------------------- */
/*  Pure helpers                                                              */
/* -------------------------------------------------------------------------- */

/// Whether a column name is a plausible match for a short typed prefix.
/// Cheap pre-filter (prefix/substring, case-insensitive) — the precise
/// fuzzy ranking still happens inside the completion popup on the
/// truncated set.
pub(crate) fn matches_short_prefix(col_name: &str, prefix: &str) -> bool {
    let lower = col_name.to_lowercase();
    lower.starts_with(prefix) || lower.contains(prefix)
}

/// `formatColumnDetail`: `dataType · PK · nullable · comment`, skipping
/// absent parts.
pub(crate) fn format_column_detail(col: &ColumnSchema) -> String {
    let mut parts = vec![col.data_type.clone()];
    if col.is_primary_key {
        parts.push("PK".to_string());
    }
    if col.nullable {
        parts.push("nullable".to_string());
    }
    if let Some(comment) = &col.comment {
        parts.push(comment.clone());
    }
    parts.join(" · ")
}

/// One related-table boost entry: how much to boost a table the statement
/// already mentions a relative of, via which relation, from which origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelatedTableBoost {
    pub boost: i32,
    pub via: String,
    pub origin: String,
}

/// `rel.kind` as the detail string React stringifies it to.
fn relation_kind_text(kind: EditorRelationKind) -> &'static str {
    match kind {
        EditorRelationKind::Table => "table",
        EditorRelationKind::View => "view",
    }
}

/* -------------------------------------------------------------------------- */
/*  Column completions from one relation                                      */
/* -------------------------------------------------------------------------- */

/// Build column completions from a single relation's metadata.
///
/// Quoting policies:
/// - `Unquoted` (default): normal identifiers unquoted; reserved words and
///   special names quoted.
/// - `Both`: unquoted + quoted for safe identifiers, quoted for
///   reserved/special.
/// - `Always`: every column quoted.
pub(crate) fn column_completions_from_relation(
    rel: &EditorRelationMetadata,
    qualifier_prefix: &str,
    dialect: &SqlCompletionDialect,
    quote_policy: CompletionQuotePolicy,
) -> Vec<SchemaCompletionItem> {
    let mut results = Vec::new();
    for col in &rel.columns {
        let quoted_label = dialect.quote_identifier(&col.name);
        let unquoted_label = col.name.clone();
        let detail = format_column_detail(col);
        let must_quote = dialect.should_quote_identifier(&col.name);

        let apply = |label: &str| -> String {
            if qualifier_prefix.is_empty() {
                label.to_string()
            } else {
                format!("{qualifier_prefix}.{label}")
            }
        };

        match quote_policy {
            CompletionQuotePolicy::Always => results.push(SchemaCompletionItem {
                label: quoted_label.clone(),
                filter_text: Some(col.name.clone()),
                kind: SchemaCompletionKind::Property,
                detail: Some(detail.clone()),
                apply: Some(apply(&quoted_label)),
                boost: Some(10),
            }),
            CompletionQuotePolicy::Both => {
                if must_quote {
                    results.push(SchemaCompletionItem {
                        label: quoted_label.clone(),
                        filter_text: Some(col.name.clone()),
                        kind: SchemaCompletionKind::Property,
                        detail: Some(detail.clone()),
                        apply: Some(apply(&quoted_label)),
                        boost: Some(12),
                    });
                } else {
                    results.push(SchemaCompletionItem {
                        label: unquoted_label.clone(),
                        filter_text: Some(col.name.clone()),
                        kind: SchemaCompletionKind::Property,
                        detail: Some(detail.clone()),
                        apply: Some(apply(&unquoted_label)),
                        boost: Some(12),
                    });
                    results.push(SchemaCompletionItem {
                        label: quoted_label.clone(),
                        filter_text: Some(col.name.clone()),
                        kind: SchemaCompletionKind::Property,
                        detail: Some(detail.clone()),
                        apply: Some(apply(&quoted_label)),
                        boost: Some(10),
                    });
                }
            }
            CompletionQuotePolicy::Unquoted => {
                if must_quote {
                    results.push(SchemaCompletionItem {
                        label: quoted_label.clone(),
                        filter_text: Some(col.name.clone()),
                        kind: SchemaCompletionKind::Property,
                        detail: Some(detail.clone()),
                        apply: Some(apply(&quoted_label)),
                        boost: Some(12),
                    });
                } else {
                    results.push(SchemaCompletionItem {
                        label: unquoted_label.clone(),
                        filter_text: Some(col.name.clone()),
                        kind: SchemaCompletionKind::Property,
                        detail: Some(detail.clone()),
                        apply: Some(apply(&unquoted_label)),
                        boost: Some(12),
                    });
                }
            }
        }
    }
    results
}

/* -------------------------------------------------------------------------- */
/*  Relation completions from the snapshot                                    */
/* -------------------------------------------------------------------------- */

/// Build table/relation completions from the metadata snapshot, filtered
/// to the current scope's database/schema context: with a single qualifier
/// part, a relation matches when its last (schema) or second-to-last
/// (database) namespace segment compares equal. A related table outranks
/// an unrelated one without excluding it — the ranking never removes a
/// valid choice.
pub(crate) fn relation_completions_from_snapshot(
    metadata: &EditorMetadataSnapshot,
    dialect: &SqlCompletionDialect,
    qualifier_parts: &[String],
    related_table_boosts: Option<&BTreeMap<String, RelatedTableBoost>>,
) -> Vec<SchemaCompletionItem> {
    let mut results = Vec::new();
    for rel in metadata.relations().values() {
        if !qualifier_parts.is_empty() {
            let last_qualifier = &qualifier_parts[qualifier_parts.len() - 1];
            let namespace_names: Vec<&str> = rel
                .identity
                .namespace_path
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            if qualifier_parts.len() == 1 {
                let matches_schema = !namespace_names.is_empty()
                    && dialect.compare_identifiers(
                        namespace_names[namespace_names.len() - 1],
                        last_qualifier,
                    );
                let matches_database = namespace_names.len() >= 2
                    && dialect.compare_identifiers(
                        namespace_names[namespace_names.len() - 2],
                        last_qualifier,
                    );
                if !matches_schema && !matches_database {
                    continue;
                }
            }
        }

        let name = dialect.quote_identifier(&rel.identity.name.name);
        let related = related_table_boosts
            .and_then(|boosts| boosts.get(&rel.identity.name.name.to_lowercase()));
        results.push(SchemaCompletionItem {
            detail: Some(match related {
                Some(related) => format!("{} · via {}", relation_kind_text(rel.kind), related.via),
                None => relation_kind_text(rel.kind).to_string(),
            }),
            apply: Some(name.clone()),
            label: name,
            filter_text: None,
            kind: SchemaCompletionKind::Type,
            boost: Some(10 + related.map(|r| r.boost).unwrap_or(0)),
        });
    }
    results
}

/// Find the snapshot key for a completion label by matching the quoted
/// relation name. Used to deduplicate filtered vs. remaining relation
/// completions.
pub(crate) fn relation_key_from_label(
    label: &str,
    dialect: &SqlCompletionDialect,
    snapshot: &EditorMetadataSnapshot,
) -> Option<String> {
    for (key, rel) in snapshot.relations() {
        let quoted_name = dialect.quote_identifier(&rel.identity.name.name);
        if quoted_name == label {
            return Some(key.clone());
        }
    }
    None
}

/* -------------------------------------------------------------------------- */
/*  All-columns fallback (snapshot)                                           */
/* -------------------------------------------------------------------------- */

/// Produce column completions for ALL columns across ALL relations in the
/// snapshot — the fallback when no FROM clause is present (projection
/// context without visible relations).
///
/// Deduplicates by (folded label, folded qualifier, quotedness) so columns
/// appearing in multiple tables are listed once per table (the qualifier
/// differs) while identical columns within one table never duplicate.
pub(crate) fn all_columns_from_snapshot(
    snapshot: &EditorMetadataSnapshot,
    include_table_prefix: bool,
    dialect: &SqlCompletionDialect,
    quote_policy: CompletionQuotePolicy,
) -> Vec<SchemaCompletionItem> {
    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for rel_meta in snapshot.relations().values() {
        if rel_meta.columns.is_empty() {
            continue;
        }

        let table_name = dialect.quote_identifier(&rel_meta.identity.name.name);
        let qualifier = rel_meta.identity.name.name.clone();

        for col in &rel_meta.columns {
            let quoted_label = dialect.quote_identifier(&col.name);
            let unquoted_label = col.name.clone();
            let detail = format!("{} · {}", format_column_detail(col), table_name);
            let must_quote = dialect.should_quote_identifier(&col.name);

            // (label, boost, quotedness) per the policy — `both` with a
            // safe name is the only two-entry case.
            let chosen: Vec<(&String, i32, bool)> = match quote_policy {
                CompletionQuotePolicy::Always => vec![(&quoted_label, 10, true)],
                CompletionQuotePolicy::Both if must_quote => vec![(&quoted_label, 12, true)],
                CompletionQuotePolicy::Both => {
                    vec![(&unquoted_label, 12, false), (&quoted_label, 10, true)]
                }
                CompletionQuotePolicy::Unquoted if must_quote => vec![(&quoted_label, 12, true)],
                CompletionQuotePolicy::Unquoted => vec![(&unquoted_label, 12, false)],
            };

            for (label, boost, quoted) in chosen {
                let key = format!(
                    "{}|{}|{}",
                    dialect.fold_unquoted_identifier(label),
                    dialect.fold_unquoted_identifier(&qualifier),
                    if quoted { "quoted" } else { "unquoted" }
                );
                if !seen.insert(key) {
                    continue;
                }
                let apply = if include_table_prefix {
                    format!("{table_name}.{label}")
                } else {
                    label.clone()
                };
                results.push(SchemaCompletionItem {
                    label: label.clone(),
                    filter_text: Some(col.name.clone()),
                    kind: SchemaCompletionKind::Property,
                    detail: Some(detail.clone()),
                    apply: Some(apply),
                    boost: Some(boost),
                });
            }
        }
    }

    results
}

/* -------------------------------------------------------------------------- */
/*  SELECT-column extraction                                                  */
/* -------------------------------------------------------------------------- */

/// Extract unqualified column names from the SELECT clause of a token
/// stream. Looks for identifiers between the SELECT keyword and the next
/// clause terminator (FROM/WHERE/GROUP/ORDER/LIMIT/HAVING/UNION/…),
/// skipping functions (identifier followed by `(`), `AS` aliases and
/// expression keywords. Returns bare column names, unquoted and
/// fold-cased to lowercase for case-insensitive comparison.
pub(crate) fn extract_select_columns(tokens: &[SqlToken]) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    // Find the SELECT keyword token.
    let mut select_idx = None;
    for (i, t) in tokens.iter().enumerate() {
        if t.kind == SqlTokenKind::Other
            && t.text.eq_ignore_ascii_case("SELECT")
            && t.paren_depth == 0
        {
            select_idx = Some(i);
            break;
        }
    }
    let select_idx = match select_idx {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    const TERMINATORS: &[&str] = &[
        "FROM",
        "WHERE",
        "GROUP",
        "ORDER",
        "LIMIT",
        "HAVING",
        "UNION",
        "INTERSECT",
        "EXCEPT",
        "MINUS",
        "INTO",
        "ON",
        "SET",
        "VALUES",
        "RETURNING",
        "WINDOW",
        "FETCH",
        "FOR",
        "LOCK",
    ];

    let mut end_idx = tokens.len();
    for (i, t) in tokens.iter().enumerate().skip(select_idx + 1) {
        if t.kind == SqlTokenKind::Other
            && t.paren_depth == 0
            && TERMINATORS.contains(&t.text.to_ascii_uppercase().as_str())
        {
            end_idx = i;
            break;
        }
    }

    const SKIP_KEYWORDS: &[&str] = &[
        "DISTINCT", "ALL", "BETWEEN", "IN", "NOT", "LIKE", "ILIKE", "ESCAPE",
    ];

    let mut columns: Vec<String> = Vec::new();
    let mut skip_next_ident = false; // after seeing AS, skip the next identifier (alias)

    let mut i = select_idx + 1;
    while i < end_idx {
        let t = &tokens[i];
        // Skip commas, parentheses.
        if t.text == "," || t.text == "(" || t.text == ")" {
            i += 1;
            continue;
        }
        if t.kind == SqlTokenKind::Whitespace {
            i += 1;
            continue;
        }

        let upper = t.text.to_ascii_uppercase();
        if t.kind == SqlTokenKind::Other && SKIP_KEYWORDS.contains(&upper.as_str()) {
            i += 1;
            continue;
        }

        // After the AS keyword, skip the next identifier (bare or
        // double-quoted) — it's an alias.
        if t.kind == SqlTokenKind::Other && upper == "AS" {
            skip_next_ident = true;
            i += 1;
            continue;
        }
        if skip_next_ident {
            if t.kind == SqlTokenKind::Other || t.kind == SqlTokenKind::DoubleQuoted {
                skip_next_ident = false;
                i += 1;
                continue;
            }
            skip_next_ident = false;
        }

        // Collect identifiers (unquoted or double-quoted).
        let ident_text: Option<String> = match t.kind {
            SqlTokenKind::Other => {
                // Bare identifier: must look like an identifier
                // (starts with a letter or underscore).
                let first = t.text.as_bytes().first().copied();
                let is_ident_start =
                    matches!(first, Some(b'A'..=b'Z') | Some(b'a'..=b'z') | Some(b'_'));
                if is_ident_start {
                    Some(t.text.to_lowercase())
                } else {
                    None
                }
            }
            SqlTokenKind::DoubleQuoted => {
                let inner = t
                    .text
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(&t.text)
                    .replace("\"\"", "\"");
                Some(inner.to_lowercase())
            }
            _ => None,
        };

        if let Some(ident) = ident_text {
            // Check the next non-whitespace token — if it's `(` this is a
            // function call; skip it (and the caller keeps walking the
            // paren group's tokens).
            let mut next_non_ws: Option<&SqlToken> = None;
            for t2 in tokens.iter().take(end_idx).skip(i + 1) {
                if t2.kind != SqlTokenKind::Whitespace {
                    next_non_ws = Some(t2);
                    break;
                }
            }
            if next_non_ws.map(|t2| t2.text.as_str()) == Some("(") {
                i += 1;
                continue;
            }

            columns.push(ident);
        }
        i += 1;
    }

    columns
}

/* -------------------------------------------------------------------------- */
/*  Relations-by-columns filter                                               */
/* -------------------------------------------------------------------------- */

/// Filter snapshot relations to only those containing ALL of the given
/// column names (case-insensitive). Empty when the column list is empty or
/// no table matches every column.
pub(crate) fn filter_relations_by_columns<'a>(
    snapshot: &'a EditorMetadataSnapshot,
    select_columns: &[String],
    dialect: &SqlCompletionDialect,
) -> Vec<&'a EditorRelationMetadata> {
    if select_columns.is_empty() {
        return Vec::new();
    }

    // React lowercases the requested names (not dialect folding) and
    // compares against the dialect-folded column set.
    let required: std::collections::BTreeSet<String> =
        select_columns.iter().map(|c| c.to_lowercase()).collect();

    snapshot
        .relations()
        .values()
        .filter(|rel_meta| {
            let col_names: std::collections::BTreeSet<String> = rel_meta
                .columns
                .iter()
                .map(|c| dialect.fold_unquoted_identifier(&c.name))
                .collect();
            required.iter().all(|req| col_names.contains(req))
        })
        .collect()
}
