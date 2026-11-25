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
//! `produceSchemaCompletions`, `createSchemaCompletionSource`) — see the
//! parent module's slice map.

use std::collections::BTreeMap;

use crate::relation_metadata::identity::{
    EditorRelationKind, QualifiedRelationId, SqlIdentifierSegment, SqlRelationSourceKind,
};
use crate::relation_metadata::ordered_map::OrderedMap;
use crate::relation_metadata::schema::{
    ColumnSchema, EditorMetadataSnapshot, EditorRelationMetadata,
};
use crate::support::sql_dialects::dialect_profile::{
    SqlDialectProfile, fold_identifier, resolve_dialect_profile,
};
use crate::support::sql_dialects::identifier_quoting;

use super::super::metadata::find_relation_metadata;
use super::super::sql_editor::CompletionQuotePolicy;
use super::super::sql_namespace::SqlNamespace;
use super::semantic_model::{
    list_visible_relations, resolve_relation, RelationResolveResult, SqlCursorIntentKind,
    SqlRelationBinding, SqlSemanticModel,
};

/* -------------------------------------------------------------------------- */
/*  Token stream shapes (extension-points `sql-editor/tokens.ts`, carried)    */
/* -------------------------------------------------------------------------- */

/// Lexical token kinds produced by the unified SQL scanner
/// (`extension-points/src/sql-editor/tokens.ts`). Carried here because the
/// SELECT-column extractor reads token streams; the scanner itself is its
/// own ledger task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlTokenKind {
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
pub struct SqlToken {
    pub kind: SqlTokenKind,
    pub text: String,
    pub paren_depth: u32,
}

impl SqlToken {
    /// Shorthand for an `Other` token at depth 0.
    pub fn word(text: &str) -> Self {
        Self {
            kind: SqlTokenKind::Other,
            text: text.to_string(),
            paren_depth: 0,
        }
    }

    /// Shorthand for a `Whitespace` token at depth 0.
    pub fn space() -> Self {
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
pub enum SchemaCompletionKind {
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
pub struct SchemaCompletionItem {
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


/// The adapter surface the completion quotes and folds through
/// (`SqlDialectAdapter`'s completion-facing half, over a resolved
/// `SqlDialectProfile`). One value per dialect id; clone-cheap.
#[derive(Clone, Debug)]
pub struct SqlCompletionDialect {
    profile: SqlDialectProfile,
}

impl SqlCompletionDialect {
    /// `getDialectAdapter(dialectId)` — unknown ids resolve to `standard`,
    /// the React fallback.
    pub fn for_dialect_id(dialect_id: &str) -> Self {
        Self {
            profile: resolve_dialect_profile(dialect_id, None),
        }
    }

    /// `adapter.quoteIdentifier(name)` — wrap in the dialect's quote style,
    /// escaping the closer.
    pub fn quote_identifier(&self, name: &str) -> String {
        identifier_quoting::wrap_quoted(name, self.profile.quote_style)
    }

    /// `adapter.foldUnquotedIdentifier(name)` — fold the way the dialect's
    /// parser folds unquoted identifiers.
    pub fn fold_unquoted_identifier(&self, name: &str) -> String {
        fold_identifier(name, &self.profile)
    }

    /// `adapter.compareIdentifiers(a, b)` — equality after folding both
    /// sides.
    pub fn compare_identifiers(&self, a: &str, b: &str) -> bool {
        self.fold_unquoted_identifier(a) == self.fold_unquoted_identifier(b)
    }

    /// `adapter.shouldQuoteIdentifier(name)` — `needsQuoting` from
    /// `lib/sqlDialects/identifierQuoting.ts` (absorbed into the shared
    /// `identifier_quoting` module by T083): empty and non-plain names
    /// quote, reserved words quote (the dialect's extras included), and a
    /// fold case that would change the spelling quotes.
    pub fn should_quote_identifier(&self, name: &str) -> bool {
        identifier_quoting::needs_quoting(
            name,
            self.profile.quote_style,
            self.profile.fold_case,
            self.profile.reserved_keywords.as_deref(),
        )
    }
}

/* -------------------------------------------------------------------------- */
/*  Constants                                                                 */
/* -------------------------------------------------------------------------- */

/// Max completion items built by the all-columns fallback for a short
/// prefix. CodeMirror renders at most `maxRenderedOptions` (50); 300 gives
/// the fuzzy matcher headroom while bounding object construction and
/// quote/dialect work.
pub const SHORT_PREFIX_COLUMN_CAP: usize = 300;

/// Prefix length at/below which the cap applies (implicit typing only).
pub const SHORT_PREFIX_LEN: usize = 2;

/* -------------------------------------------------------------------------- */
/*  Pure helpers                                                              */
/* -------------------------------------------------------------------------- */

/// Whether a column name is a plausible match for a short typed prefix.
/// Cheap pre-filter (prefix/substring, case-insensitive) — the precise
/// fuzzy ranking still happens inside the completion popup on the
/// truncated set.
pub fn matches_short_prefix(col_name: &str, prefix: &str) -> bool {
    let lower = col_name.to_lowercase();
    lower.starts_with(prefix) || lower.contains(prefix)
}

/// `formatColumnDetail`: `dataType · PK · nullable · comment`, skipping
/// absent parts.
pub fn format_column_detail(col: &ColumnSchema) -> String {
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
pub struct RelatedTableBoost {
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
pub fn column_completions_from_relation(
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
pub fn relation_completions_from_snapshot(
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
pub fn relation_key_from_label(
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
pub fn all_columns_from_snapshot(
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
pub fn extract_select_columns(tokens: &[SqlToken]) -> Vec<String> {
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
pub fn filter_relations_by_columns<'a>(
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

/* -------------------------------------------------------------------------- */
/*  S2/S3: options and the schema-tree entry points                            */
/* -------------------------------------------------------------------------- */

/// `SchemaCompletionOptions` — the entry-point inputs. `model`/`snapshot`
/// are required like React's; every optional field maps one React
/// optional (adapter → `dialect_id`, default `standard`; `schema` → the
/// editor schema tree; `quotePolicy`/`includeTablePrefix` defaults
/// `'unquoted'`/`true`; `prefixHint`/`relatedTableBoosts` optional).
pub struct SchemaCompletionOptions<'a> {
    pub model: &'a SqlSemanticModel,
    pub snapshot: &'a EditorMetadataSnapshot,
    /// `adapter?: SqlDialectAdapter` — a dialect id resolved through the
    /// facade (`getDialectAdapter(dialectId)`, default `'standard'`).
    pub dialect_id: &'a str,
    /// `schema?: SQLNamespace` — the full editor schema tree
    /// (`buildEditorSchema()`'s output; its task is T340).
    pub schema: Option<&'a SqlNamespace>,
    pub quote_policy: CompletionQuotePolicy,
    pub include_table_prefix: bool,
    /// `prefixHint` — lowercased, unquoted; short (≤2 chars) hints cap
    /// the all-columns-from-schema fallback.
    pub prefix_hint: Option<&'a str>,
    pub related_table_boosts: Option<&'a BTreeMap<String, RelatedTableBoost>>,
}

impl<'a> SchemaCompletionOptions<'a> {
    /// React's field defaults.
    pub fn new(
        model: &'a SqlSemanticModel,
        snapshot: &'a EditorMetadataSnapshot,
    ) -> Self {
        Self {
            model,
            snapshot,
            dialect_id: "standard",
            schema: None,
            quote_policy: CompletionQuotePolicy::Unquoted,
            include_table_prefix: true,
            prefix_hint: None,
            related_table_boosts: None,
        }
    }
}

/// `matchRelation`: `findRelationMetadata` under the entry points' naming.
fn match_relation<'a>(
    binding: &SqlRelationBinding,
    metadata: &'a EditorMetadataSnapshot,
    dialect_id: &str,
    schema_tree: Option<&SqlNamespace>,
) -> Option<EditorRelationMetadata> {
    find_relation_metadata(
        Some(metadata),
        &binding.relation,
        dialect_id,
        schema_tree,
    )
}

/* -------------------------------------------------------------------------- */
/*  resolveSchemaDotCompletions                                               */
/* -------------------------------------------------------------------------- */

/// Resolve `schema.` / `db.schema.` prefixes → table completions from the
/// editor schema tree: a precise walk first, then (single-part
/// qualifiers only) a deep search below the current database hoist.
/// Returns `None` when the qualifier addresses no namespace node — the
/// caller keeps its previous behavior, no guessing.
pub fn resolve_schema_dot_completions(
    qualifier_parts: &[String],
    schema: Option<&SqlNamespace>,
    dialect: &SqlCompletionDialect,
) -> Option<Vec<SchemaCompletionItem>> {
    let schema = schema?;
    if qualifier_parts.is_empty() || qualifier_parts.len() > 2 {
        return None;
    }

    // 1. Precise walk: `buyer.` → top-level `buyer`; `db.schema.` → two
    // levels, preserving qualified-path semantics for nested trees.
    let mut node = schema;
    let mut precise = true;
    for part in qualifier_parts {
        let branch = as_branch_node(node)?;
        let key = branch
            .iter()
            .find(|(k, _)| dialect.compare_identifiers(k, part))
            .map(|(k, _)| k.clone());
        match key {
            Some(key) => {
                node = branch.get(&key)?;
            }
            None => {
                precise = false;
                break;
            }
        }
    }
    if precise {
        if let Some(branch) = as_branch_node(node) {
            if let Some(from_precise) = tables_from_namespace_node(branch, dialect) {
                return Some(from_precise);
            }
        }
    }

    // 2. Deep search: the namespace may sit below the current database
    // hoist (`{ winam: { buyer: { … } } }` while the qualifier is bare
    // `buyer`). Single-part qualifiers only — deeper paths already had
    // their precise chance above.
    if qualifier_parts.len() == 1 {
        if let Some(found) = find_namespace_branch(schema, &qualifier_parts[0], dialect) {
            if let Some(from_deep) = tables_from_namespace_node(found, dialect) {
                return Some(from_deep);
            }
        }
    }
    None
}

/// `asBranchNode`: narrow a node to its branch map (`None` for leaves).
fn as_branch_node(node: &SqlNamespace) -> Option<&OrderedMap<String, SqlNamespace>> {
    match node {
        SqlNamespace::Branch(branch) => Some(branch),
        SqlNamespace::Table(_) => None,
    }
}

/// `tablesFromNamespaceNode`: table-leaf completions under a namespace
/// node, deduplicated by folded name (`None` when it holds no tables).
///
/// `apply` carries ONLY the table name: the completion `from` covers just
/// the fragment after the dot, so accepting `au` in `buyer.au` yields
/// `buyer."autoscale_configs"` — prefixing the qualifier here would
/// duplicate it.
fn tables_from_namespace_node(
    node: &OrderedMap<String, SqlNamespace>,
    dialect: &SqlCompletionDialect,
) -> Option<Vec<SchemaCompletionItem>> {
    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (tbl_key, child) in node.iter() {
        if !matches!(child, SqlNamespace::Table(_)) {
            continue; // nested namespace, not a table leaf
        }
        let folded = dialect.fold_unquoted_identifier(tbl_key);
        if !seen.insert(folded) {
            continue;
        }
        let quoted = dialect.quote_identifier(tbl_key);
        results.push(SchemaCompletionItem {
            label: quoted.clone(),
            filter_text: Some(tbl_key.clone()),
            kind: SchemaCompletionKind::Type,
            detail: Some("table".to_string()),
            apply: Some(quoted),
            boost: Some(10),
        });
    }
    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// `findNamespaceBranch`: depth-first search for the first branch node
/// keyed by `name` (table leaves are skipped — a leaf keyed like the
/// namespace is not the namespace).
fn find_namespace_branch<'a>(
    node: &'a SqlNamespace,
    name: &str,
    dialect: &SqlCompletionDialect,
) -> Option<&'a OrderedMap<String, SqlNamespace>> {
    let branch = as_branch_node(node)?;
    for (key, child) in branch.iter() {
        if matches!(child, SqlNamespace::Table(_)) {
            continue;
        }
        if dialect.compare_identifiers(key, name) {
            return as_branch_node(child);
        }
        if let Some(deeper) = find_namespace_branch(child, name, dialect) {
            return Some(deeper);
        }
    }
    None
}

/* -------------------------------------------------------------------------- */
/*  resolveAliasDotCompletions                                                */
/* -------------------------------------------------------------------------- */

/// Resolve `alias.` → column completions for the uniquely-bound relation.
///
/// Returns `None` when the prefix is not a single qualifier, the alias is
/// ambiguous or unresolved, or the resolved relation is a CTE/subquery or
/// carries no metadata (an empty hit must NOT short-circuit the caller's
/// schema-tree fallback — it means "no columns known", not "resolved").
pub fn resolve_alias_dot_completions(
    qualifier_parts: &[String],
    model: &SqlSemanticModel,
    snapshot: &EditorMetadataSnapshot,
    dialect: &SqlCompletionDialect,
    dialect_id: &str,
    schema_tree: Option<&SqlNamespace>,
    quote_policy: CompletionQuotePolicy,
) -> Option<Vec<SchemaCompletionItem>> {
    if qualifier_parts.len() != 1 {
        return None;
    }

    let qualifier = &qualifier_parts[0];
    let result = resolve_relation(qualifier, model, None, dialect);

    let binding = match result {
        RelationResolveResult::Unique(binding) => binding,
        _ => return None,
    };
    // No column completions for CTEs/subqueries — only for real relations
    // whose metadata is available in the snapshot.
    if !matches!(
        binding.source_kind,
        SqlRelationSourceKind::Table | SqlRelationSourceKind::View
    ) {
        return None;
    }

    let rel_meta = match_relation(&binding, snapshot, dialect_id, schema_tree)?;
    if rel_meta.columns.is_empty() {
        return None;
    }

    Some(column_completions_from_relation(&rel_meta, "", dialect, quote_policy))
}

/* -------------------------------------------------------------------------- */
/*  Editor-schema fallbacks                                                   */
/* -------------------------------------------------------------------------- */

/// `relationCompletionsFromEditorSchema`: relation completions from the
/// editor schema tree, used when the snapshot is empty or incomplete.
/// With qualifier parts, the tables under the matching top-level
/// namespace; otherwise every table leaf plus one entry per schema node
/// (deduplicated by folded name).
fn relation_completions_from_editor_schema(
    schema: &SqlNamespace,
    dialect: &SqlCompletionDialect,
    qualifier_parts: &[String],
) -> Vec<SchemaCompletionItem> {
    let mut results = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    fn add_table(
        results: &mut Vec<SchemaCompletionItem>,
        seen: &mut std::collections::BTreeSet<String>,
        name: &str,
        dialect: &SqlCompletionDialect,
    ) {
        let quoted = dialect.quote_identifier(name);
        let key = dialect.fold_unquoted_identifier(name);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        results.push(SchemaCompletionItem {
            label: quoted.clone(),
            filter_text: None,
            kind: SchemaCompletionKind::Type,
            detail: Some("table".to_string()),
            apply: Some(quoted),
            boost: Some(10),
        });
    }

    fn add_schema(
        results: &mut Vec<SchemaCompletionItem>,
        seen: &mut std::collections::BTreeSet<String>,
        name: &str,
        dialect: &SqlCompletionDialect,
    ) {
        let quoted = dialect.quote_identifier(name);
        let key = dialect.fold_unquoted_identifier(name);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        results.push(SchemaCompletionItem {
            label: quoted.clone(),
            filter_text: None,
            kind: SchemaCompletionKind::Type,
            detail: Some("schema".to_string()),
            apply: Some(quoted),
            boost: Some(8),
        });
    }

    let Some(root) = as_branch_node(schema) else {
        return results;
    };

    if !qualifier_parts.is_empty() {
        let last_qualifier = &qualifier_parts[qualifier_parts.len() - 1];
        for (key, child) in root.iter() {
            if !matches!(child, SqlNamespace::Table(_))
                && dialect.compare_identifiers(key, last_qualifier)
            {
                if let Some(child_branch) = as_branch_node(child) {
                    for (tbl_key, _) in child_branch.iter() {
                        add_table(&mut results, &mut seen, tbl_key, dialect);
                    }
                }
            }
        }
        return results;
    }

    for (key, child) in root.iter() {
        if matches!(child, SqlNamespace::Table(_)) {
            add_table(&mut results, &mut seen, key, dialect);
        } else {
            add_schema(&mut results, &mut seen, key, dialect);
            if let Some(child_branch) = as_branch_node(child) {
                for (tbl_key, _) in child_branch.iter() {
                    add_table(&mut results, &mut seen, tbl_key, dialect);
                }
            }
        }
    }

    results
}

/// `allColumnsFromEditorSchema`: column completions for ALL columns from
/// the full editor schema tree — the projection fallback when there is no
/// FROM clause AND the metadata snapshot is empty. A short (≤2 chars)
/// `prefix_hint` pre-filters and truncates so deleting down to a short
/// prefix does not build items for the whole database.
pub fn all_columns_from_editor_schema(
    schema: &SqlNamespace,
    include_table_prefix: bool,
    dialect: &SqlCompletionDialect,
    quote_policy: CompletionQuotePolicy,
    prefix_hint: Option<&str>,
) -> Vec<SchemaCompletionItem> {
    let mut results = Vec::new();
    let prefix = match prefix_hint {
        Some(hint) if !hint.is_empty() && hint.len() <= SHORT_PREFIX_LEN => {
            Some(hint.to_lowercase())
        }
        _ => None,
    };

    fn walk(
        node: &SqlNamespace,
        table_name: Option<&str>,
        results: &mut Vec<SchemaCompletionItem>,
        prefix: &Option<String>,
        include_table_prefix: bool,
        dialect: &SqlCompletionDialect,
        quote_policy: CompletionQuotePolicy,
    ) {
        match node {
            // Leaf: the node is the column-name list of `table_name`.
            SqlNamespace::Table(cols) => {
                let Some(table_name) = table_name else { return };
                if cols.is_empty() {
                    return;
                }
                let quoted_table = dialect.quote_identifier(table_name);
                for col_name in cols {
                    if let Some(prefix) = prefix {
                        if !matches_short_prefix(col_name, prefix) {
                            continue;
                        }
                        if results.len() >= SHORT_PREFIX_COLUMN_CAP {
                            return;
                        }
                    }
                    let quoted_label = dialect.quote_identifier(col_name);
                    let unquoted_label = col_name.clone();
                    let must_quote = dialect.should_quote_identifier(col_name);

                    let apply = |label: &str| -> String {
                        if include_table_prefix {
                            format!("{quoted_table}.{label}")
                        } else {
                            label.to_string()
                        }
                    };

                    let push = |label: &str, boost: i32,
                                results: &mut Vec<SchemaCompletionItem>| {
                        results.push(SchemaCompletionItem {
                            label: label.to_string(),
                            filter_text: Some(col_name.clone()),
                            kind: SchemaCompletionKind::Property,
                            detail: Some(table_name.to_string()),
                            apply: Some(apply(label)),
                            boost: Some(boost),
                        });
                    };

                    match quote_policy {
                        CompletionQuotePolicy::Always => push(&quoted_label, 10, results),
                        CompletionQuotePolicy::Both => {
                            if must_quote {
                                push(&quoted_label, 12, results);
                            } else {
                                push(&unquoted_label, 12, results);
                                push(&quoted_label, 10, results);
                            }
                        }
                        CompletionQuotePolicy::Unquoted => {
                            if must_quote {
                                push(&quoted_label, 12, results);
                            } else {
                                push(&unquoted_label, 12, results);
                            }
                        }
                    }
                }
            }
            // Branch: each key is a table (leaf child) or a nested
            // namespace — recurse without changing the pending table name
            // so table leaves inside the namespace still resolve.
            SqlNamespace::Branch(branch) => {
                for (key, child) in branch.iter() {
                    match child {
                        SqlNamespace::Table(_) => walk(
                            child,
                            Some(key),
                            results,
                            prefix,
                            include_table_prefix,
                            dialect,
                            quote_policy,
                        ),
                        SqlNamespace::Branch(_) => walk(
                            child,
                            table_name,
                            results,
                            prefix,
                            include_table_prefix,
                            dialect,
                            quote_policy,
                        ),
                    }
                }
            }
        }
    }

    walk(
        schema,
        None,
        &mut results,
        &prefix,
        include_table_prefix,
        dialect,
        quote_policy,
    );
    results
}

/* -------------------------------------------------------------------------- */
/*  produceSchemaCompletions                                                  */
/* -------------------------------------------------------------------------- */

/// Produce completions for the current cursor intent:
///
/// 1. `qualified_column` with a single qualifier → resolve the alias,
///    then the schema-namespace fallback; ambiguity resolves to nothing.
/// 2. `relation` / `join_target` → table completions from the snapshot
///    (FROM-column matches first when the SELECT lists columns), merged
///    with the editor schema tree.
/// 3. `projection` → aliases plus the visible relations' columns, falling
///    back to all columns from the snapshot, then from the schema tree.
/// 4. Anything else → relation completions from the snapshot.
pub fn produce_schema_completions(options: &SchemaCompletionOptions<'_>) -> Vec<SchemaCompletionItem> {
    let dialect = SqlCompletionDialect::for_dialect_id(options.dialect_id);
    let dialect_id = options.dialect_id;
    let intent = &options.model.cursor_intent;
    let include_table_prefix = options.include_table_prefix;
    let quote_policy = options.quote_policy;

    match intent.kind {
        SqlCursorIntentKind::QualifiedColumn => {
            if let Some(completions) = resolve_alias_dot_completions(
                &intent.qualifier_parts,
                options.model,
                options.snapshot,
                &dialect,
                dialect_id,
                options.schema,
                quote_policy,
            ) {
                if !completions.is_empty() {
                    return completions;
                }
            }
            // Schema-namespace fallback: `buyer.` where `buyer` is a
            // schema/database (not a FROM binding, or a binding with zero
            // known columns) offers its tables from the schema tree. An
            // empty alias hit must NOT short-circuit here — it means "no
            // columns known", not "resolved".
            if let Some(schema) = options.schema {
                if let Some(schema_tables) =
                    resolve_schema_dot_completions(&intent.qualifier_parts, Some(schema), &dialect)
                {
                    return schema_tables;
                }
            }
            // Ambiguous or unresolved qualifier: nothing — don't guess.
            Vec::new()
        }

        SqlCursorIntentKind::Relation | SqlCursorIntentKind::JoinTarget => {
            // FROM hint: if SELECT has column names, suggest tables that
            // contain ALL those columns first.
            let select_columns = extract_select_columns(&options.model.tokens);
            if !select_columns.is_empty() {
                let matching_tables =
                    filter_relations_by_columns(options.snapshot, &select_columns, &dialect);
                if !matching_tables.is_empty() {
                    let mut completions: Vec<SchemaCompletionItem> = matching_tables
                        .iter()
                        .map(|rel_meta| {
                            let name = dialect.quote_identifier(&rel_meta.identity.name.name);
                            SchemaCompletionItem {
                                label: name.clone(),
                                filter_text: None,
                                kind: SchemaCompletionKind::Type,
                                detail: Some(format!(
                                    "{} (matched columns)",
                                    relation_kind_text(rel_meta.kind)
                                )),
                                apply: Some(name),
                                boost: Some(15),
                            }
                        })
                        .collect();
                    // Remaining tables follow as secondary suggestions.
                    let filtered_keys: std::collections::BTreeSet<String> = matching_tables
                        .iter()
                        .map(|r| r.key.clone())
                        .collect();
                    let remaining = relation_completions_from_snapshot(
                        options.snapshot,
                        &dialect,
                        &intent.qualifier_parts,
                        options.related_table_boosts,
                    )
                    .into_iter()
                    .filter(|c| {
                        let k = relation_key_from_label(&c.label, &dialect, options.snapshot);
                        k.is_none() || !filtered_keys.contains(&k.unwrap())
                    });
                    completions.extend(remaining);
                    return completions;
                }
            }

            let mut from_snapshot = relation_completions_from_snapshot(
                options.snapshot,
                &dialect,
                &intent.qualifier_parts,
                options.related_table_boosts,
            );
            if !from_snapshot.is_empty() {
                // Merge extra tables from the schema that the snapshot
                // does not know yet.
                if let Some(schema) = options.schema {
                    let from_schema = relation_completions_from_editor_schema(
                        schema,
                        &dialect,
                        &intent.qualifier_parts,
                    );
                    let mut seen_labels: std::collections::BTreeSet<String> =
                        from_snapshot.iter().map(|c| c.label.clone()).collect();
                    for item in from_schema {
                        if seen_labels.insert(item.label.clone()) {
                            from_snapshot.push(item);
                        }
                    }
                }
                return from_snapshot;
            }

            // Snapshot empty: fall back to the schema tree.
            if let Some(schema) = options.schema {
                return relation_completions_from_editor_schema(
                    schema,
                    &dialect,
                    &intent.qualifier_parts,
                );
            }

            Vec::new()
        }

        SqlCursorIntentKind::Projection => {
            let visible = list_visible_relations(options.model, None, &dialect);
            if !visible.is_empty() {
                let mut results: Vec<SchemaCompletionItem> = Vec::new();
                let mut seen_qualifiers = std::collections::BTreeSet::new();

                // 1. Alias completions (typing "ON e" suggests "eo").
                for binding in &visible {
                    if let Some(alias) = &binding.alias {
                        let table_name = &binding.relation.name.name;
                        results.push(SchemaCompletionItem {
                            label: alias.clone(),
                            filter_text: None,
                            kind: SchemaCompletionKind::Variable,
                            detail: Some(format!("alias for {table_name}")),
                            apply: None,
                            boost: Some(8),
                        });
                    }
                }

                // 2. Column completions from each visible relation. The
                // schema tree is passed on purpose: without it the
                // metadata fallback has nothing to read while the snapshot
                // is still filling in (async in React), and a bare
                // `WHERE ` completes nothing at all.
                for binding in &visible {
                    let Some(rel_meta) = match_relation(
                        binding,
                        options.snapshot,
                        dialect_id,
                        options.schema,
                    ) else {
                        continue;
                    };

                    let qualifier = binding.alias.clone().unwrap_or_else(|| {
                        fold_name_segment(&binding.relation.name, &dialect)
                    });
                    if !seen_qualifiers
                        .insert(dialect.fold_unquoted_identifier(&qualifier))
                    {
                        continue;
                    }

                    results.extend(column_completions_from_relation(
                        &rel_meta,
                        if include_table_prefix { &qualifier } else { "" },
                        &dialect,
                        quote_policy,
                    ));
                }

                // Deduplicate by label. Returning unconditionally (the old
                // behavior) made the fallbacks below unreachable for every
                // statement with a FROM clause, including those whose
                // bindings resolved to no columns at all — an empty result
                // falls through instead of ending the search.
                let mut seen_labels = std::collections::BTreeSet::new();
                results.retain(|item| seen_labels.insert(item.label.clone()));
                if !results.is_empty() {
                    return results;
                }
            }

            // Fallback: ALL columns — first from the snapshot (tables the
            // SQL references), then from the full editor schema tree
            // (typing "SELECT ord" with no FROM clause).
            let from_snapshot = all_columns_from_snapshot(
                options.snapshot,
                include_table_prefix,
                &dialect,
                quote_policy,
            );
            if !from_snapshot.is_empty() {
                return from_snapshot;
            }

            if let Some(schema) = options.schema {
                return all_columns_from_editor_schema(
                    schema,
                    include_table_prefix,
                    &dialect,
                    quote_policy,
                    options.prefix_hint,
                );
            }

            Vec::new()
        }

        _ => relation_completions_from_snapshot(
            options.snapshot,
            &dialect,
            &intent.qualifier_parts,
            options.related_table_boosts,
        ),
    }
}

/// `foldSegment(binding.relation.name, adapter)` — quoted name segments
/// pass through, unquoted ones fold (quoteHelper.ts, the one segment the
/// projection branch needs).
fn fold_name_segment(
    segment: &crate::relation_metadata::identity::SqlIdentifierSegment,
    dialect: &SqlCompletionDialect,
) -> String {
    if segment.quoted {
        segment.name.clone()
    } else {
        dialect.fold_unquoted_identifier(&segment.name)
    }
}

/* -------------------------------------------------------------------------- */
/*  createSchemaCompletionSource                                              */
/* -------------------------------------------------------------------------- */

/// The CodeMirror `CompletionResult` the source returns: the replacement
/// start, the options, and `validFor` — the pattern a further typed
/// character must match to keep the popup alive
/// (`/^[A-Za-z_$"][\w$"']*$/`).
#[derive(Clone, Debug)]
pub struct SchemaCompletionResult {
    pub from: usize,
    pub options: Vec<SchemaCompletionItem>,
}

/// `validFor` — hand-rolled, the regex's exact semantics: the first
/// character is a letter, `_`, `$` or `"`; every later character
/// additionally allows digits and `'`.
pub fn completion_valid_for(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let first_ok = first.is_ascii_alphabetic() || first == '_' || first == '$' || first == '"';
    if !first_ok {
        return false;
    }
    chars.all(|c| {
        c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '"' || c == '\''
    })
}

/// `createSchemaCompletionSource`: wrap `produceSchemaCompletions` as a
/// completion source — one `()`-callable producing the result, `None`
/// when there is nothing to offer (CodeMirror's "no popup").
pub fn create_schema_completion_source(
    options: &SchemaCompletionOptions<'_>,
) -> Option<SchemaCompletionResult> {
    let completions = produce_schema_completions(options);
    if completions.is_empty() {
        return None;
    }
    Some(SchemaCompletionResult {
        from: options.model.cursor_intent.replacement_range.from,
        options: completions,
    })
}
