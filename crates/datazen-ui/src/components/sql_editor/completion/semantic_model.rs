//! SQL semantic model — the **resolver half** of React's
//! `@datazen/extension-points` `sql-editor/scopeModel.ts` +
//! `relationResolver.ts` + `semanticTypes.ts`, carried as a dependency
//! slice for the schema-completion entry points (T062 S2/S3).
//!
//! The extension-points package is not part of the 647-task React parity
//! ledger, so its shapes land where consumed, the `lib_helpers`
//! precedent. What is carried here:
//!
//! * the **shapes** the completion entry points read: scopes (with their
//!   relation and CTE bindings), the cursor intent, and the semantic
//!   model they hang off;
//! * the **resolver functions** that operate on those shapes:
//!   `findScopeAtCursor`, `getScopeChain` (scopeModel.ts),
//!   `resolveRelation`, `listVisibleRelations` (relationResolver.ts) —
//!   including the CTE-shadowing and innermost-wins rules.
//!
//! What is deliberately NOT carried (and documented so it is not
//! re-derived): the **builder half** — `scanSql` (scanner.ts),
//! `buildStatementRanges` (statementRanges.ts), `buildScopesForStatement`
//! (scope/builder.ts + scope/clauseParsers.ts) and `detectCursorIntent`
//! (scope/cursorIntent.ts). Nothing in the completion entry points calls
//! them: they produce the model this module consumes. A model arrives
//! fully built from the host (tests construct it by hand exactly like
//! React's own unit tests do).
//!
//! Faithfulness notes:
//!
//! * `SqlSemanticModel` here is the **resolver-visible subset** — tokens
//!   (read by the SELECT-column extractor), scopes and the cursor
//!   intent. React's `statement`/`references`/`diagnostics` fields are
//!   unread by every ported entry point and stay with the builder half.
//! * Tokens reuse this crate's carried [`SqlToken`][super::schema_completion::SqlToken]
//!   (kind/text/paren-depth, no offsets): the resolver reads token
//!   positions only through scope ranges and the intent's replacement
//!   range, which are separate fields here.
//! * Shadowing and ambiguity are order-sensitive: React iterates arrays
//!   and `Map`s in construction order, so the Rust side keeps `Vec`s and
//!   returns candidates in the same order the scopes list them.

use crate::relation_metadata::identity::{
    QualifiedRelationId, SqlRelationSourceKind, SqlIdentifierSegment,
};

use super::schema_completion::{SqlCompletionDialect, SqlToken};

/* -------------------------------------------------------------------------- */
/*  Shapes (semanticTypes.ts)                                                 */
/* -------------------------------------------------------------------------- */

/// Inclusive-exclusive document offset range (`SqlTextRange`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlTextRange {
    pub from: usize,
    pub to: usize,
}

impl SqlTextRange {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }
}

/// The cursor intents the completion entry points branch on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlCursorIntentKind {
    /// `relation`
    Relation,
    /// `qualified_column`
    QualifiedColumn,
    /// `projection`
    Projection,
    /// `join_target`
    JoinTarget,
    /// `function_call`
    FunctionCall,
    /// `insert_values`
    InsertValues,
    /// `insert_columns`
    InsertColumns,
    /// `unknown`
    Unknown,
}

/// `SqlCursorIntent`: what the user is typing at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlCursorIntent {
    pub kind: SqlCursorIntentKind,
    /// Source text from the statement start (or 128 chars back) to the
    /// cursor — the prefix the intent was detected from.
    pub prefix: String,
    pub replacement_range: SqlTextRange,
    pub qualifier_parts: Vec<String>,
    pub scope_id: Option<String>,
}

/// `SqlRelationBinding`: one FROM/JOIN binding (relation + alias).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRelationBinding {
    pub relation: QualifiedRelationId,
    pub alias: Option<String>,
    pub source_range: SqlTextRange,
    pub alias_range: Option<SqlTextRange>,
    pub source_kind: SqlRelationSourceKind,
}

impl SqlRelationBinding {
    /// A table binding with no alias — the test/builder shorthand.
    pub fn table(name: &str) -> Self {
        Self {
            relation: QualifiedRelationId::bare(name),
            alias: None,
            source_range: SqlTextRange::new(0, 0),
            alias_range: None,
            source_kind: SqlRelationSourceKind::Table,
        }
    }
}

/// `SqlCteBinding`: one `WITH name AS (…)` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlCteBinding {
    pub name: String,
    pub name_range: SqlTextRange,
    /// `columnNames` (CTE column list) — carried for shape fidelity; the
    /// resolver reads only `name`.
    pub column_names: Option<Vec<String>>,
    pub column_list_range: Option<SqlTextRange>,
    /// The sub-query's own scope id (carried for shape fidelity).
    pub query_scope_id: String,
}

impl SqlCteBinding {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            name_range: SqlTextRange::new(0, 0),
            column_names: None,
            column_list_range: None,
            query_scope_id: String::new(),
        }
    }
}

/// `SqlProjectionAlias` — carried for shape fidelity (the completion
/// entry points do not read it; the scope shape does).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // shape-fidelity field of SqlScope
pub struct SqlProjectionAlias {
    pub alias: String,
    pub alias_range: SqlTextRange,
    pub expression_range: SqlTextRange,
}

/// `SqlScope`: one nested analysis scope with its bindings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlScope {
    pub id: String,
    pub range: SqlTextRange,
    pub parent_id: Option<String>,
    pub kind: SqlScopeKind,
    pub ctes: Vec<SqlCteBinding>,
    pub relations: Vec<SqlRelationBinding>,
    pub projection_aliases: Vec<SqlProjectionAlias>,
}

/// `SqlScope['kind']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlScopeKind {
    Select,
    Insert,
    Update,
    Delete,
    Subquery,
    Cte,
}

impl SqlScope {
    /// A `select` scope with no parent — the hand-built-fixture shorthand.
    pub fn select(id: &str, from: usize, to: usize) -> Self {
        Self {
            id: id.to_string(),
            range: SqlTextRange::new(from, to),
            parent_id: None,
            kind: SqlScopeKind::Select,
            ctes: Vec::new(),
            relations: Vec::new(),
            projection_aliases: Vec::new(),
        }
    }
}

/// `SqlSemanticModel` — the resolver-visible subset (see module docs).
#[derive(Clone, Debug)]
pub struct SqlSemanticModel {
    pub tokens: Vec<SqlToken>,
    pub scopes: Vec<SqlScope>,
    pub cursor_intent: SqlCursorIntent,
}

/// `RelationResolveResult`: unique binding, ambiguous candidates, or
/// nothing.
#[derive(Clone, Debug)]
pub enum RelationResolveResult {
    Unique(Box<SqlRelationBinding>),
    Ambiguous(Vec<SqlRelationBinding>),
    Unresolved,
}

/* -------------------------------------------------------------------------- */
/*  Scope walk (scopeModel.ts)                                                */
/* -------------------------------------------------------------------------- */

/// The innermost scope containing `cursor`; when the cursor sits past a
/// scope's end (trailing whitespace, a typed dot), the scope that starts
/// before or at the cursor wins — React's fallback, kept verbatim.
pub fn find_scope_at_cursor<'a>(
    scopes: &'a [SqlScope],
    cursor: usize,
) -> Option<&'a SqlScope> {
    let mut best: Option<&SqlScope> = None;
    let mut best_size = usize::MAX;
    for scope in scopes {
        if cursor >= scope.range.from && cursor <= scope.range.to {
            let size = scope.range.to - scope.range.from;
            if size < best_size {
                best = Some(scope);
                best_size = size;
            }
        }
    }
    if best.is_some() {
        return best;
    }

    // Fallback: pick the scope that starts before or at the cursor.
    for scope in scopes {
        if cursor >= scope.range.from {
            if best.is_none_or(|b| scope.range.from >= b.range.from) {
                best = Some(scope);
            }
        }
    }
    if best.is_none() {
        best = scopes.first();
    }
    best
}

/// The scope chain from `scope_id` out to the root (innermost first).
pub fn get_scope_chain<'a>(
    scopes: &'a [SqlScope],
    scope_id: &str,
) -> Vec<&'a SqlScope> {
    let mut chain = Vec::new();
    let mut current = scopes.iter().find(|s| s.id == scope_id);
    while let Some(scope) = current {
        chain.push(scope);
        current = scope
            .parent_id
            .as_deref()
            .and_then(|pid| scopes.iter().find(|s| s.id == pid));
    }
    chain
}

/* -------------------------------------------------------------------------- */
/*  Relation resolution (relationResolver.ts)                                 */
/* -------------------------------------------------------------------------- */

/// `cteToBinding`: a CTE resolves as a synthetic relation binding.
fn cte_to_binding(cte: &SqlCteBinding) -> SqlRelationBinding {
    SqlRelationBinding {
        relation: QualifiedRelationId {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new(&cte.name, false),
        },
        alias: Some(cte.name.clone()),
        alias_range: Some(cte.name_range),
        source_range: cte.name_range,
        source_kind: SqlRelationSourceKind::Cte,
    }
}

/// `bindingMatchesName`: an aliased relation matches only by its alias
/// (SQL correlation-name semantics); an unaliased one by its base name.
fn binding_matches_name(
    binding: &SqlRelationBinding,
    name: &str,
    dialect: &SqlCompletionDialect,
) -> bool {
    if let Some(alias) = &binding.alias {
        if dialect.compare_identifiers(alias, name) {
            return true;
        }
    }
    dialect.compare_identifiers(&fold_segment(&binding.relation.name, dialect), name)
}

/// `relationKey`/`foldSegment` (quoteHelper.ts): quoted segments pass
/// through unfolded, unquoted segments fold by the dialect.
fn fold_segment(
    segment: &SqlIdentifierSegment,
    dialect: &SqlCompletionDialect,
) -> String {
    if segment.quoted {
        segment.name.clone()
    } else {
        dialect.fold_unquoted_identifier(&segment.name)
    }
}

fn relation_key(id: &QualifiedRelationId, dialect: &SqlCompletionDialect) -> String {
    id.namespace_path
        .iter()
        .chain(std::iter::once(&id.name))
        .map(|segment| fold_segment(segment, dialect))
        .collect::<Vec<_>>()
        .join(".")
}

/// `resolveRelation`: resolve a table/alias name within the scope chain,
/// inner scopes shadowing outer. A CTE name shadows same-named relations
/// in the same scope. Ambiguity is reported, never guessed.
pub fn resolve_relation(
    name: &str,
    model: &SqlSemanticModel,
    cursor: Option<usize>,
    dialect: &SqlCompletionDialect,
) -> RelationResolveResult {
    let pos = cursor.unwrap_or(model.cursor_intent.replacement_range.from);
    let Some(scope) = find_scope_at_cursor(&model.scopes, pos) else {
        return RelationResolveResult::Unresolved;
    };

    for chain_scope in get_scope_chain(&model.scopes, &scope.id) {
        // A CTE name shadows any relation with the same name in this scope.
        let cte_matches: Vec<&SqlCteBinding> = chain_scope
            .ctes
            .iter()
            .filter(|cte| dialect.compare_identifiers(&cte.name, name))
            .collect();
        if cte_matches.len() == 1 {
            return RelationResolveResult::Unique(Box::new(cte_to_binding(cte_matches[0])));
        }
        if cte_matches.len() > 1 {
            return RelationResolveResult::Ambiguous(
                cte_matches.iter().map(|c| cte_to_binding(c)).collect(),
            );
        }

        let matches: Vec<&SqlRelationBinding> = chain_scope
            .relations
            .iter()
            .filter(|binding| binding_matches_name(binding, name, dialect))
            .collect();
        if matches.len() == 1 {
            return RelationResolveResult::Unique(Box::new(matches[0].clone()));
        }
        if matches.len() > 1 {
            return RelationResolveResult::Ambiguous(matches.into_iter().cloned().collect());
        }
    }

    RelationResolveResult::Unresolved
}

/// `listVisibleRelations`: every binding reachable at the cursor through
/// the scope chain, deduplicated by folded alias (innermost wins, first
/// insertion wins within a scope — React's shadowing order).
///
/// Returns owned bindings: React returns the scope's binding objects plus
/// `cteToBinding`-synthesized CTE bindings in one array; cloning the few
/// reachable bindings keeps that single-shape list without lifetime
/// acrobatics over synthesized values.
pub fn list_visible_relations(
    model: &SqlSemanticModel,
    cursor: Option<usize>,
    dialect: &SqlCompletionDialect,
) -> Vec<SqlRelationBinding> {
    let pos = cursor.unwrap_or(model.cursor_intent.replacement_range.from);
    let Some(scope) = find_scope_at_cursor(&model.scopes, pos) else {
        return Vec::new();
    };

    let mut by_alias: Vec<(String, SqlRelationBinding)> = Vec::new();

    for chain_scope in get_scope_chain(&model.scopes, &scope.id) {
        for binding in &chain_scope.relations {
            let key = binding
                .alias
                .clone()
                .unwrap_or_else(|| relation_key(&binding.relation, dialect));
            let folded = dialect.fold_unquoted_identifier(&key);
            if !by_alias.iter().any(|(k, _)| *k == folded) {
                by_alias.push((folded, binding.clone()));
            }
        }
        for cte in &chain_scope.ctes {
            let folded = dialect.fold_unquoted_identifier(&cte.name);
            if !by_alias.iter().any(|(k, _)| *k == folded) {
                by_alias.push((folded, cte_to_binding(cte)));
            }
        }
    }

    by_alias.into_iter().map(|(_, b)| b).collect()
}
