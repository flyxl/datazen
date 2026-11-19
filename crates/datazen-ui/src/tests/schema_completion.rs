//! T062-S1: schema completion's pure core
//! (`components/sql-editor/completion/schemaCompletion.ts`).
//!
//! The landed half builds completions from the metadata snapshot and the
//! token stream; the semantic-model / schema-tree entry points land with
//! S2/S3. These tests pin the React semantics on fixed inputs — a hand
//! built snapshot (postgres double-quote dialect unless stated) and hand
//! built token streams:
//!
//! 1. the dialect facade (quote styles, folding, comparison, the
//!    must-quote gate: reserved words, non-plain names, fold-case drift);
//! 2. `columnCompletionsFromRelation` — the three quoting policies, the
//!    boosts, the qualifier-prefixed `apply` text;
//! 3. `relationCompletionsFromSnapshot` — schema/database qualifier
//!    filtering and the related-table boost detail;
//! 4. `relationKeyFromLabel`;
//! 5. `allColumnsFromSnapshot` — the (folded label, folded qualifier,
//!    quotedness) dedup and the table-prefix toggle;
//! 6. `extractSelectColumns` — alias skip, function-call skip, quoted
//!    identifiers, terminators, and the in-paren collection quirk both
//!    runtimes share;
//! 7. `filterRelationsByColumns` — all-columns-must-match, folded.

use std::collections::BTreeMap;

use crate::components::sql_editor::CompletionQuotePolicy;
use crate::components::sql_editor::completion::schema_completion as sc;
use crate::relation_metadata::identity::{
    EditorRelationKind, QualifiedRelationId, SqlIdentifierSegment,
};
use crate::relation_metadata::schema::{
    ColumnSchema, EditorMetadataSnapshot, EditorRelationMetadata,
};

/// One relation's metadata for the fixture snapshot. `cols` are
/// (name, data_type, nullable, is_primary_key).
fn rel(
    name: &str,
    namespaces: &[&str],
    cols: &[(&str, &str, bool, bool)],
) -> EditorRelationMetadata {
    EditorRelationMetadata {
        key: format!("session::{name}"),
        identity: QualifiedRelationId {
            namespace_path: namespaces
                .iter()
                .map(|n| SqlIdentifierSegment::new(*n, false))
                .collect(),
            name: SqlIdentifierSegment::new(name, false),
        },
        kind: EditorRelationKind::Table,
        columns: cols
            .iter()
            .map(|(n, t, nullable, pk)| ColumnSchema {
                name: (*n).to_string(),
                data_type: (*t).to_string(),
                nullable: *nullable,
                is_primary_key: *pk,
                ..ColumnSchema::default()
            })
            .collect(),
        primary_key: Vec::new(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        comment: None,
        loaded_at: 0,
    }
}

fn snapshot(relations: Vec<EditorRelationMetadata>) -> EditorMetadataSnapshot {
    EditorMetadataSnapshot {
        db_session_id: "session".to_string(),
        database: "db".to_string(),
        schema: None,
        epoch: 1,
        relations: relations
            .into_iter()
            .map(|r| (format!("session::{}", r.identity.name.name), r))
            .collect(),
    }
}

#[test]
fn dialect_facade_quotes_folds_and_gates() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let mysql = sc::SqlCompletionDialect::for_dialect_id("mysql");
    let unknown = sc::SqlCompletionDialect::for_dialect_id("not-a-dialect");

    // Quote styles: double quotes (escaped), backticks, and the unknown-id
    // fallback resolving to standard.
    assert_eq!(pg.quote_identifier("order"), "\"order\"");
    assert_eq!(pg.quote_identifier("we\"ird"), "\"we\"\"ird\"");
    assert_eq!(mysql.quote_identifier("order"), "`order`");
    assert_eq!(unknown.quote_identifier("x"), "\"x\"");

    // Folding: postgres folds lower, comparison is fold-equality.
    assert_eq!(pg.fold_unquoted_identifier("Users"), "users");
    assert!(pg.compare_identifiers("Users", "users"));
    assert!(!pg.compare_identifiers("users", "orders"));

    // The must-quote gate: reserved words quote, non-plain names quote,
    // fold-case drift quotes, plain lowercase names do not.
    assert!(pg.should_quote_identifier("order"));
    assert!(pg.should_quote_identifier("has-dash"));
    assert!(pg.should_quote_identifier("MixedCase"));
    assert!(!pg.should_quote_identifier("name"));
    assert!(!mysql.should_quote_identifier("name"));
}

#[test]
fn column_completions_quote_policies() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let relation = rel(
        "users",
        &[],
        &[
            ("name", "text", true, false),
            ("order", "int", false, false),
            ("UserId", "uuid", false, true),
        ],
    );

    // Unquoted (default): plain stays bare, reserved and fold-drifting
    // names quote; detail carries type + PK + nullable.
    let unquoted =
        sc::column_completions_from_relation(&relation, "", &pg, CompletionQuotePolicy::Unquoted);
    assert_eq!(unquoted.len(), 3);
    assert_eq!(unquoted[0].label, "name");
    assert_eq!(unquoted[0].apply.as_deref(), Some("name"));
    assert_eq!(unquoted[0].detail.as_deref(), Some("text · nullable"));
    assert_eq!(unquoted[1].label, "\"order\"");
    assert_eq!(unquoted[1].apply.as_deref(), Some("\"order\""));
    assert_eq!(unquoted[2].label, "\"UserId\"");
    assert_eq!(unquoted[2].detail.as_deref(), Some("uuid · PK"));
    assert_eq!(unquoted[0].boost, Some(12));

    // Both: safe identifiers list twice (unquoted first at 12, quoted at
    // 10); must-quote ones list once.
    let both =
        sc::column_completions_from_relation(&relation, "", &pg, CompletionQuotePolicy::Both);
    assert_eq!(both.len(), 4);
    assert_eq!(both[0].label, "name");
    assert_eq!(both[1].label, "\"name\"");
    assert_eq!(both[1].boost, Some(10));
    assert_eq!(both[2].label, "\"order\"");
    assert_eq!(both[3].label, "\"UserId\"");

    // Always: everything quotes, at boost 10.
    let always =
        sc::column_completions_from_relation(&relation, "", &pg, CompletionQuotePolicy::Always);
    assert_eq!(always.len(), 3);
    assert!(always.iter().all(|c| c.label.starts_with('"')));
    assert!(always.iter().all(|c| c.boost == Some(10)));

    // The qualifier prefix lands in `apply`, label stays bare.
    let prefixed =
        sc::column_completions_from_relation(&relation, "e", &pg, CompletionQuotePolicy::Unquoted);
    assert_eq!(prefixed[0].apply.as_deref(), Some("e.name"));
    assert_eq!(prefixed[0].label, "name");
    assert_eq!(prefixed[1].apply.as_deref(), Some("e.\"order\""));

    // filter_text always carries the raw column name.
    let filters: Vec<&str> = unquoted
        .iter()
        .map(|c| c.filter_text.as_deref().expect("filter"))
        .collect();
    assert_eq!(filters, vec!["name", "order", "UserId"]);
}

#[test]
fn relation_completions_filter_by_qualifier_and_boost() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![
        rel("users", &["public"], &[("id", "int", false, true)]),
        rel("orders", &["sales"], &[("id", "int", false, true)]),
        rel("logs", &["db", "public"], &[("id", "int", false, true)]),
    ]);

    // No qualifier: every relation, kind as detail. Labels are QUOTED
    // (React quotes the relation name for the label) and BTreeMap order
    // lists the snapshot by key: logs, orders, users.
    let all = sc::relation_completions_from_snapshot(&snap, &pg, &[], None);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].label, "\"logs\"");
    assert_eq!(all[0].detail.as_deref(), Some("table"));
    assert_eq!(all[0].boost, Some(10));

    // One qualifier part matches the schema level (last namespace
    // segment) or the database level (second-to-last).
    let q = vec!["public".to_string()];
    let public = sc::relation_completions_from_snapshot(&snap, &pg, &q, None);
    let names: Vec<&str> = public.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(names, vec!["\"logs\"", "\"users\""]);

    let q = vec!["db".to_string()];
    let db = sc::relation_completions_from_snapshot(&snap, &pg, &q, None);
    let names: Vec<&str> = db.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(names, vec!["\"logs\""]);

    // Related-table boosts: rank up and extend the detail, without
    // excluding the unrelated tables.
    let mut boosts = BTreeMap::new();
    boosts.insert(
        "orders".to_string(),
        sc::RelatedTableBoost {
            boost: 7,
            via: "users.id".to_string(),
            origin: "fk".to_string(),
        },
    );
    let boosted = sc::relation_completions_from_snapshot(&snap, &pg, &[], Some(&boosts));
    let orders = boosted
        .iter()
        .find(|c| c.label == "\"orders\"")
        .expect("orders present");
    assert_eq!(orders.boost, Some(17));
    assert_eq!(orders.detail.as_deref(), Some("table · via users.id"));
    let users = boosted
        .iter()
        .find(|c| c.label == "\"users\"")
        .expect("users present");
    assert_eq!(users.boost, Some(10));
}

#[test]
fn relation_key_from_label_maps_quoted_names() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![rel(
        "users",
        &["public"],
        &[("id", "int", false, true)],
    )]);

    assert_eq!(
        sc::relation_key_from_label("\"users\"", &pg, &snap).as_deref(),
        Some("session::users")
    );
    assert_eq!(sc::relation_key_from_label("users", &pg, &snap), None);
    assert_eq!(sc::relation_key_from_label("\"missing\"", &pg, &snap), None);
}

#[test]
fn all_columns_dedup_and_table_prefix() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![
        rel(
            "users",
            &[],
            &[("id", "int", false, true), ("name", "text", true, false)],
        ),
        rel(
            "orders",
            &[],
            &[
                ("id", "int", false, true),
                ("total", "numeric", true, false),
            ],
        ),
    ]);

    // Shared column names list once per table (the qualifier differs),
    // each carrying its QUOTED table in the detail and the quoted-table
    // prefix in `apply` (React quotes the table name in both — verbatim).
    // BTreeMap iteration orders the snapshot by key: orders before users.
    let with_prefix =
        sc::all_columns_from_snapshot(&snap, true, &pg, CompletionQuotePolicy::Unquoted);
    assert_eq!(with_prefix.len(), 4);
    let ids: Vec<&str> = with_prefix
        .iter()
        .filter(|c| c.label == "id")
        .map(|c| c.apply.as_deref().expect("apply present"))
        .collect();
    assert_eq!(ids, vec!["\"orders\".id", "\"users\".id"]);
    assert_eq!(
        with_prefix[0].detail.as_deref(),
        Some("int · PK · \"orders\"")
    );

    // Without the prefix, `apply` is the bare label.
    let bare = sc::all_columns_from_snapshot(&snap, false, &pg, CompletionQuotePolicy::Unquoted);
    assert_eq!(bare[0].apply.as_deref(), Some("id"));

    // Two tables whose names fold equal share one qualifier slot: the
    // second table's columns dedup away (React's folded-key semantics).
    let folding = snapshot(vec![
        rel("users", &[], &[("id", "int", false, true)]),
        rel("USERS", &[], &[("id", "int", false, true)]),
    ]);
    let folded =
        sc::all_columns_from_snapshot(&folding, true, &pg, CompletionQuotePolicy::Unquoted);
    assert_eq!(folded.len(), 1);

    // Empty relations contribute nothing; an all-empty snapshot is empty.
    assert!(
        sc::all_columns_from_snapshot(
            &snapshot(vec![rel("empty", &[], &[])]),
            true,
            &pg,
            CompletionQuotePolicy::Unquoted
        )
        .is_empty()
    );
}

#[test]
fn extract_select_columns_walks_the_clause() {
    use sc::{SqlToken as T, SqlTokenKind as K};

    let tokens = |parts: &[T]| parts.to_vec();

    // Bare identifiers, AS alias, function calls, quoted names, and the
    // FROM terminator.
    let list = tokens(&[
        T::word("SELECT"),
        T::word("id"),
        T {
            kind: K::Other,
            text: ",".into(),
            paren_depth: 0,
        },
        T::word("name"),
        T::space(),
        T::word("AS"),
        T::space(),
        T::word("n"),
        T {
            kind: K::Other,
            text: ",".into(),
            paren_depth: 0,
        },
        T::word("count"),
        T {
            kind: K::OpenParen,
            text: "(".into(),
            paren_depth: 0,
        },
        T {
            kind: K::Other,
            text: "*".into(),
            paren_depth: 1,
        },
        T {
            kind: K::CloseParen,
            text: ")".into(),
            paren_depth: 0,
        },
        T {
            kind: K::Other,
            text: ",".into(),
            paren_depth: 0,
        },
        T {
            kind: K::DoubleQuoted,
            text: "\"Quoted Col\"".into(),
            paren_depth: 0,
        },
        T::space(),
        T::word("FROM"),
        T::word("t"),
    ]);
    assert_eq!(
        sc::extract_select_columns(&list),
        vec!["id", "name", "quoted col"]
    );

    // SELECT inside a subquery (paren depth > 0) is not the clause head;
    // the outer SELECT is.
    let nested = tokens(&[
        T::word("SELECT"),
        T::word("outer_col"),
        T::word("FROM"),
        T {
            kind: K::OpenParen,
            text: "(".into(),
            paren_depth: 0,
        },
        T::word("SELECT"),
        T::word("inner_col"),
        T::word("FROM"),
        T::word("x"),
        T {
            kind: K::CloseParen,
            text: ")".into(),
            paren_depth: 0,
        },
    ]);
    assert_eq!(sc::extract_select_columns(&nested), vec!["outer_col"]);

    // A terminator at depth 0 ends the clause; the subquery that FOLLOWS
    // the outer FROM is outside the clause. (A subquery inside the select
    // list would collect its inner identifiers verbatim — a documented
    // quirk of the React extractor, matched by the port and kept out of
    // these fixtures.)
    let deep = tokens(&[
        T::word("SELECT"),
        T::word("outer_col"),
        T::word("FROM"),
        T {
            kind: K::OpenParen,
            text: "(".into(),
            paren_depth: 0,
        },
        T {
            kind: K::Other,
            text: "SELECT".into(),
            paren_depth: 1,
        },
        T {
            kind: K::Other,
            text: "inner_col".into(),
            paren_depth: 1,
        },
        T {
            kind: K::Other,
            text: "FROM".into(),
            paren_depth: 1,
        },
        T {
            kind: K::Other,
            text: "x".into(),
            paren_depth: 1,
        },
        T {
            kind: K::CloseParen,
            text: ")".into(),
            paren_depth: 0,
        },
    ]);
    assert_eq!(sc::extract_select_columns(&deep), vec!["outer_col"]);
    // No SELECT keyword → nothing.
    assert!(
        sc::extract_select_columns(&tokens(&[
            T::word("UPDATE"),
            T::word("t"),
            T::word("SET"),
            T::word("a")
        ]))
        .is_empty()
    );
    // No terminator → the clause runs to the end of the stream.
    assert_eq!(
        sc::extract_select_columns(&tokens(&[T::word("SELECT"), T::word("id")])),
        vec!["id"]
    );
    // Function call names skip: identifier followed by `(` is a call.
    assert_eq!(
        sc::extract_select_columns(&tokens(&[
            T::word("SELECT"),
            T::word("coalesce"),
            T {
                kind: K::OpenParen,
                text: "(".into(),
                paren_depth: 0
            },
            T::word("a"),
            T {
                kind: K::CloseParen,
                text: ")".into(),
                paren_depth: 0
            },
        ])),
        vec!["a"]
    );
}

#[test]
fn filter_relations_by_columns_requires_all() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![
        rel(
            "users",
            &[],
            &[("id", "int", false, true), ("name", "text", true, false)],
        ),
        rel(
            "orders",
            &[],
            &[
                ("id", "int", false, true),
                ("total", "numeric", true, false),
            ],
        ),
    ]);

    let both = sc::filter_relations_by_columns(&snap, &["id".to_string(), "name".to_string()], &pg);
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].identity.name.name, "users");

    // Case folds: `NAME` matches the folded column set.
    let folded =
        sc::filter_relations_by_columns(&snap, &["ID".to_string(), "NAME".to_string()], &pg);
    assert_eq!(folded.len(), 1);

    // A column no table has → nothing (all must match).
    assert!(
        sc::filter_relations_by_columns(&snap, &["id".to_string(), "ghost".to_string()], &pg)
            .is_empty()
    );
    // Empty request → nothing.
    assert!(sc::filter_relations_by_columns(&snap, &[], &pg).is_empty());
}

#[test]
fn short_prefix_prefilter_and_constants() {
    assert_eq!(sc::SHORT_PREFIX_COLUMN_CAP, 300);
    assert_eq!(sc::SHORT_PREFIX_LEN, 2);

    assert!(sc::matches_short_prefix("order_date", "or"));
    assert!(sc::matches_short_prefix("order_date", "date"));
    assert!(!sc::matches_short_prefix("order_date", "zz"));
    // Case-insensitive both ways.
    assert!(sc::matches_short_prefix("OrderDate", "or"));
}

/* -------------------------------------------------------------------------- */
/*  T062 S2/S3: semantic model, schema tree, and the entry points             */
/* -------------------------------------------------------------------------- */

use crate::components::sql_editor::completion::semantic_model as sm;
use crate::components::sql_editor::completion::{
    completion_valid_for, create_schema_completion_source, produce_schema_completions,
    resolve_alias_dot_completions, resolve_schema_dot_completions, SchemaCompletionOptions,
};
use crate::components::sql_editor::sql_namespace::SqlNamespace;

fn intent(
    kind: sm::SqlCursorIntentKind,
    qualifier: &[&str],
    from: usize,
) -> sm::SqlCursorIntent {
    sm::SqlCursorIntent {
        kind,
        prefix: String::new(),
        replacement_range: sm::SqlTextRange::new(from, from),
        qualifier_parts: qualifier.iter().map(|q| q.to_string()).collect(),
        scope_id: None,
    }
}

fn model(
    scopes: Vec<sm::SqlScope>,
    cursor_intent: sm::SqlCursorIntent,
    tokens: Vec<sc::SqlToken>,
) -> sm::SqlSemanticModel {
    sm::SqlSemanticModel { tokens, scopes, cursor_intent }
}

fn binding(name: &str, alias: Option<&str>) -> sm::SqlRelationBinding {
    sm::SqlRelationBinding {
        relation: QualifiedRelationId {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::new(name, false),
        },
        alias: alias.map(str::to_string),
        source_range: sm::SqlTextRange::new(0, 100),
        alias_range: None,
        source_kind: crate::relation_metadata::identity::SqlRelationSourceKind::Table,
    }
}

#[test]
fn alias_dot_returns_columns_for_a_unique_alias() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let users = rel(
        "users",
        &[],
        &[
            ("id", "integer", false, true),
            ("name", "varchar", true, false),
            ("email", "varchar", false, false),
        ],
    );
    let snap = snapshot(vec![users]);

    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("users", Some("u")));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::QualifiedColumn, &["u"], 3), vec![]);

    let completions = resolve_alias_dot_completions(
        &["u".to_string()], &mdl, &snap, &pg, "postgresql", None,
        CompletionQuotePolicy::Unquoted,
    )
    .expect("unique alias resolves");
    let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["id", "name", "email"]);

    let both = resolve_alias_dot_completions(
        &["u".to_string()], &mdl, &snap, &pg, "postgresql", None,
        CompletionQuotePolicy::Both,
    )
    .expect("both policy still resolves");
    assert_eq!(both.len(), 6);
    let both_labels: Vec<&str> = both.iter().map(|c| c.label.as_str()).collect();
    for expected in ["\"id\"", "id", "\"name\"", "name", "\"email\"", "email"] {
        assert!(both_labels.contains(&expected), "missing {expected} in {both_labels:?}");
    }
}

#[test]
fn alias_dot_quotes_reserved_keywords_and_carries_comment() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let mut orders = rel(
        "orders",
        &[],
        &[
            ("id", "integer", false, true),
            ("order", "varchar", true, false),
        ],
    );
    orders.columns[1].comment = Some("The order column".to_string());
    let snap = snapshot(vec![orders]);

    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("orders", Some("o")));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::QualifiedColumn, &["o"], 3), vec![]);

    let completions = resolve_alias_dot_completions(
        &["o".to_string()], &mdl, &snap, &pg, "postgresql", None,
        CompletionQuotePolicy::Unquoted,
    )
    .unwrap();
    assert_eq!(completions[1].label, "\"order\"");
    assert_eq!(
        completions[1].detail.as_deref(),
        Some("varchar · nullable · The order column"),
        "detail carries type, nullable and comment"
    );
}

#[test]
fn alias_dot_returns_none_for_ambiguous_unresolved_or_non_relation() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let users = rel("users", &[], &[("id", "integer", false, true)]);
    let snap = snapshot(vec![users]);

    // Ambiguous: two bindings match the qualifier.
    let mut ambiguous = sm::SqlScope::select("s1", 0, 100);
    ambiguous.relations.push(binding("users", Some("u")));
    ambiguous.relations.push(binding("user_archive", Some("U")));
    let mdl = model(
        vec![ambiguous],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["u"], 3),
        vec![],
    );
    assert!(
        resolve_alias_dot_completions(
            &["u".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none(),
        "an ambiguous alias never guesses"
    );

    // Unresolved: nothing matches.
    let mut clean = sm::SqlScope::select("s1", 0, 100);
    clean.relations.push(binding("users", Some("u")));
    let mdl = model(vec![clean], intent(sm::SqlCursorIntentKind::QualifiedColumn, &["z"], 3), vec![]);
    assert!(
        resolve_alias_dot_completions(
            &["z".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none()
    );

    // CTE: resolves, but CTEs have no snapshot metadata.
    let mut cte_scope = sm::SqlScope::select("s1", 0, 100);
    cte_scope.ctes.push(sm::SqlCteBinding::named("active_users"));
    let mdl = model(
        vec![cte_scope],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["active_users"], 3),
        vec![],
    );
    assert!(
        resolve_alias_dot_completions(
            &["active_users".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none(),
        "CTE bindings are not real relations"
    );

    // Missing metadata: the alias resolves to a relation the snapshot
    // knows nothing about.
    let mut no_meta = sm::SqlScope::select("s1", 0, 100);
    no_meta.relations.push(binding("ghosts", Some("g")));
    let mdl = model(vec![no_meta], intent(sm::SqlCursorIntentKind::QualifiedColumn, &["g"], 3), vec![]);
    assert!(
        resolve_alias_dot_completions(
            &["g".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none()
    );

    // Multi-part qualifiers are not alias dots.
    let mut two = sm::SqlScope::select("s1", 0, 100);
    two.relations.push(binding("users", Some("u")));
    let mdl = model(
        vec![two],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["db", "u"], 3),
        vec![],
    );
    assert!(
        resolve_alias_dot_completions(
            &["db".to_string(), "u".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none()
    );
}

#[test]
fn schema_dot_resolves_precise_and_deep_paths() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");

    // Precise two-part walk: shop.buyer → buyer's tables.
    let shop = SqlNamespace::branch([(
        "shop",
        SqlNamespace::branch([
            ("buyer", SqlNamespace::branch([
                ("orders", SqlNamespace::table(["id"])),
                ("profiles", SqlNamespace::table(["buyer_id"])),
            ])),
            ("seller", SqlNamespace::branch([("listings", SqlNamespace::table(["id"]))])),
        ]),
    )]);
    let out = resolve_schema_dot_completions(
        &["shop".to_string(), "buyer".to_string()], Some(&shop), &pg,
    )
    .expect("precise walk resolves");
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"orders\"", "\"profiles\""]);
    // apply carries ONLY the table name (the qualifier is already typed).
    assert_eq!(out[0].apply.as_deref(), Some("\"orders\""));
    assert_eq!(out[0].detail.as_deref(), Some("table"));
    assert_eq!(out[0].boost, Some(10));
    assert_eq!(out[0].filter_text.as_deref(), Some("orders"));

    // Unknown qualifier: None — the caller keeps its behavior.
    assert!(resolve_schema_dot_completions(&["ghost".to_string()], Some(&shop), &pg).is_none());

    // A table leaf is not a namespace: flat roots never resolve.
    let flat = SqlNamespace::branch([("users", SqlNamespace::table(["id"]))]);
    assert!(resolve_schema_dot_completions(&["users".to_string()], Some(&flat), &pg).is_none());

    // More than two parts: never resolved.
    assert!(
        resolve_schema_dot_completions(
            &["a".to_string(), "b".to_string(), "c".to_string()], Some(&shop), &pg,
        )
        .is_none()
    );

    // Deep search below the current database hoist: the qualifier is bare
    // `buyer` while the tree nests it under `winam`.
    let nested = SqlNamespace::branch([(
        "winam",
        SqlNamespace::branch([(
            "buyer",
            SqlNamespace::branch([("autoscale_configs", SqlNamespace::table(["id"]))]),
        )]),
    )]);
    let out = resolve_schema_dot_completions(&["buyer".to_string()], Some(&nested), &pg)
        .expect("deep search resolves a hoisted namespace");
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"autoscale_configs\""]);
}

#[test]
fn produce_relation_completions_for_join_target() {
    let snap = snapshot(vec![
        rel("users", &[], &[("id", "integer", false, true)]),
        rel("orders", &[], &[("id", "integer", false, true)]),
    ]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::JoinTarget, &[], 10), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    // BTreeMap snapshot order (see the all-columns test): sorted keys —
    // orders before users.
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|c| c.boost == Some(10)));
    assert!(out.iter().all(|c| matches!(c.kind, sc::SchemaCompletionKind::Type)));
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"orders\"", "\"users\""]);
}

#[test]
fn produce_empty_for_ambiguous_qualifier_without_a_schema() {
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("users", Some("u")));
    scope.relations.push(binding("user_archive", Some("U")));
    let mdl = model(
        vec![scope],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["u"], 3),
        vec![],
    );
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    assert!(
        produce_schema_completions(&options).is_empty(),
        "an ambiguous qualifier returns nothing — no guessing"
    );
}

#[test]
fn produce_schema_dot_fallback_for_a_namespace_qualifier() {
    // `buyer.` where buyer is a schema (not a FROM binding): the schema
    // tree offers its tables.
    let snap = snapshot(vec![]);
    let tree = SqlNamespace::branch([(
        "buyer",
        SqlNamespace::branch([("orders", SqlNamespace::table(["id"]))]),
    )]);
    let mdl = model(
        vec![],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["buyer"], 7),
        vec![],
    );
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"orders\""]);
}

#[test]
fn produce_from_hint_lists_matching_tables_first() {
    // SELECT id, name + FROM-context intent: tables containing BOTH
    // columns come first with the matched-columns detail, the rest follow.
    let snap = snapshot(vec![
        rel("users", &[], &[("id", "integer", false, true), ("name", "text", true, false)]),
        rel("events", &[], &[("id", "integer", false, true), ("ts", "timestamptz", false, false)]),
    ]);
    let mdl = model(
        vec![],
        intent(sm::SqlCursorIntentKind::Relation, &[], 20),
        vec![
            sc::SqlToken::word("SELECT"),
            sc::SqlToken::space(),
            sc::SqlToken::word("id"),
            sc::SqlToken { kind: sc::SqlTokenKind::Other, text: ",".to_string(), paren_depth: 0 },
            sc::SqlToken::space(),
            sc::SqlToken::word("name"),
            sc::SqlToken::space(),
            sc::SqlToken::word("FROM"),
        ],
    );
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    assert_eq!(out[0].label, "\"users\"");
    assert_eq!(out[0].boost, Some(15));
    assert_eq!(out[0].detail.as_deref(), Some("table (matched columns)"));
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"users\"", "\"events\""], "non-matching tables follow");
}

#[test]
fn produce_projection_lists_aliases_then_columns() {
    let snap = snapshot(vec![
        rel(
            "orders",
            &[],
            &[("order_no", "text", false, false), ("buyer_id", "int", true, false)],
        ),
        rel("customers", &[], &[("name", "text", true, false)]),
    ]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("orders", Some("eo")));
    scope.relations.push(binding("customers", Some("ec")));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::Projection, &[], 40), vec![]);
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.include_table_prefix = true;

    let out = produce_schema_completions(&options);
    let alias_items: Vec<&sc::SchemaCompletionItem> = out
        .iter()
        .filter(|c| matches!(c.kind, sc::SchemaCompletionKind::Variable))
        .collect();
    let alias_labels: Vec<&str> = alias_items.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(alias_labels, vec!["eo", "ec"], "alias completions come first");
    assert_eq!(alias_items[0].detail.as_deref(), Some("alias for orders"));
    assert_eq!(alias_items[0].boost, Some(8));

    let column_items: Vec<&sc::SchemaCompletionItem> = out
        .iter()
        .filter(|c| matches!(c.kind, sc::SchemaCompletionKind::Property))
        .collect();
    let column_labels: Vec<&str> = column_items.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(column_labels, vec!["order_no", "buyer_id", "name"]);
    assert_eq!(column_items[0].apply.as_deref(), Some("eo.order_no"));
}

#[test]
fn produce_projection_falls_back_to_all_columns_from_the_snapshot() {
    // No visible relations: ALL columns from ALL snapshot tables.
    let snap = snapshot(vec![
        rel("users", &[], &[("id", "integer", false, true), ("name", "text", true, false)]),
        rel("orders", &[], &[("order_no", "text", false, false)]),
        rel("empty", &[], &[]),
    ]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Projection, &[], 8), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    // Snapshot iteration order: the landed snapshot stores relations in a
    // BTreeMap (sorted keys, segment 1's landed type), so completions follow
    // sorted-key order — `session::orders` before `session::users` — where
    // React's insertion-ordered Map lists users first. Same items, same
    // boosts; only the pre-fuzzy order differs.
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["order_no", "id", "name"]);
    assert_eq!(out[1].apply.as_deref(), Some("\"users\".id"));
}

#[test]
fn produce_projection_falls_back_to_the_editor_schema_tree() {
    // Empty snapshot: the full tree provides the columns, nested
    // namespaces included, with the quoted table prefix.
    let snap = snapshot(vec![]);
    let tree = SqlNamespace::branch([
        ("users", SqlNamespace::table(["id", "name"])),
        (
            "main",
            SqlNamespace::branch([("orders", SqlNamespace::table(["order_no"])), ("misc", SqlNamespace::table(["x"]))]),
        ),
    ]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Projection, &[], 8), vec![]);
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["id", "name", "order_no", "x"]);
    // The table qualifier is the LEAF key only — React's walk passes the
    // nested namespace's name down unchanged, so a table under `main`
    // still prefixes just `"orders"`, not `"main"."orders"`.
    assert_eq!(out[2].apply.as_deref(), Some("\"orders\".order_no"));
    assert_eq!(out[2].detail.as_deref(), Some("orders"));
}

#[test]
fn produce_snapshot_columns_win_over_the_schema_tree() {
    // Snapshot has data: the tree fallback is not consulted.
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let tree = SqlNamespace::branch([("users", SqlNamespace::table(["id", "tree_only"]))]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Projection, &[], 8), vec![]);
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["id"], "tree-only columns do not appear");
}

#[test]
fn produce_short_prefix_hints_cap_the_tree_fallback() {
    // 350 columns matching a 1-char hint plus one non-matching column: the
    // deletion fast-path caps the built items at SHORT_PREFIX_COLUMN_CAP
    // (300) and the pre-filter keeps the non-matching column out.
    let mut columns: Vec<String> = vec!["zebra".to_string()];
    columns.extend((0..350).map(|i| format!("c{i:03}")));
    let tree = SqlNamespace::branch([("wide", SqlNamespace::table(columns))]);
    let snap = snapshot(vec![]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Projection, &[], 8), vec![]);
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);
    options.prefix_hint = Some("c");

    let out = produce_schema_completions(&options);
    assert_eq!(out.len(), sc::SHORT_PREFIX_COLUMN_CAP);
    assert!(out.iter().all(|c| c.label.starts_with('c')), "the pre-filter drops non-matching columns");

    // Without a hint (or with a long one) the full set is kept.
    options.prefix_hint = None;
    assert_eq!(produce_schema_completions(&options).len(), 351);
    options.prefix_hint = Some("c00");
    assert_eq!(produce_schema_completions(&options).len(), 351);
}

#[test]
fn produce_relation_merges_schema_tree_extras() {
    // Snapshot knows one table; the tree adds another the snapshot does
    // not have yet.
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let tree = SqlNamespace::branch([
        ("users", SqlNamespace::table(["id"])),
        ("legacy", SqlNamespace::table(["old_id"])),
    ]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Relation, &[], 10), vec![]);
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"users\"", "\"legacy\""], "snapshot tables first, tree extras after");
}

#[test]
fn produce_unknown_intent_falls_back_to_snapshot_relations() {
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Unknown, &[], 5), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["\"users\""]);
}

#[test]
fn source_wraps_produce_into_a_result() {
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mdl = model(vec![], intent(sm::SqlCursorIntentKind::Relation, &[], 42), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let result = create_schema_completion_source(&options).expect("non-empty produces a result");
    assert_eq!(result.from, 42, "from is the intent's replacement start");
    assert!(!result.options.is_empty());

    let empty = snapshot(vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &empty);
    assert!(
        create_schema_completion_source(&options).is_none(),
        "empty completions produce no popup"
    );
}

#[test]
fn valid_for_matches_the_react_regex() {
    // /^[A-Za-z_$"][\w$"']*$/
    assert!(completion_valid_for("abc"));
    assert!(completion_valid_for("_x"));
    assert!(completion_valid_for("$y"));
    assert!(completion_valid_for("\"z"));
    assert!(completion_valid_for("a1_\"$'"));
    assert!(!completion_valid_for("1abc"), "digits cannot start");
    assert!(!completion_valid_for("-abc"), "operators cannot start");
    assert!(!completion_valid_for("a-b"), "operators cannot continue");
    assert!(!completion_valid_for(""), "empty never matches");
}

#[test]
fn resolver_scopes_shadow_and_report_ambiguity() {
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");

    // Inner scope shadows outer by alias.
    let mut outer = sm::SqlScope::select("s0", 0, 100);
    outer.relations.push(binding("users", Some("u")));
    let mut inner = sm::SqlScope::select("s1", 10, 50);
    inner.parent_id = Some("s0".to_string());
    inner.relations.push(binding("uploads", Some("u")));
    let mdl = model(
        vec![outer, inner],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["u"], 20),
        vec![],
    );

    match sm::resolve_relation("u", &mdl, None, &pg) {
        sm::RelationResolveResult::Unique(binding) => {
            assert_eq!(binding.relation.name.name, "uploads", "inner scope wins");
        }
        _ => panic!("inner alias must resolve uniquely"),
    }

    let visible = sm::list_visible_relations(&mdl, None, &pg);
    let names: Vec<&str> = visible.iter().map(|b| b.relation.name.name.as_str()).collect();
    assert_eq!(names, vec!["uploads"], "the shadowed outer alias is deduplicated");

    // Ambiguity inside one scope is reported, never guessed.
    let mut ambiguous = sm::SqlScope::select("s1", 0, 100);
    ambiguous.relations.push(binding("users", Some("u")));
    ambiguous.relations.push(binding("user_archive", Some("u")));
    let mdl = model(
        vec![ambiguous],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["u"], 3),
        vec![],
    );
    assert!(matches!(
        sm::resolve_relation("u", &mdl, None, &pg),
        sm::RelationResolveResult::Ambiguous(_)
    ));

    // A CTE resolves as a synthetic CTE binding.
    let mut cte_scope = sm::SqlScope::select("s1", 0, 100);
    cte_scope.ctes.push(sm::SqlCteBinding::named("recent"));
    let mdl = model(
        vec![cte_scope],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["recent"], 3),
        vec![],
    );
    match sm::resolve_relation("recent", &mdl, None, &pg) {
        sm::RelationResolveResult::Unique(binding) => {
            assert_eq!(binding.source_kind, crate::relation_metadata::identity::SqlRelationSourceKind::Cte);
            assert_eq!(binding.alias.as_deref(), Some("recent"));
        }
        _ => panic!("a unique CTE must resolve"),
    }
}

#[test]
fn find_scope_at_cursor_picks_innermost_then_falls_back() {
    let scopes = vec![
        sm::SqlScope::select("s0", 0, 100),
        sm::SqlScope::select("s1", 10, 50),
    ];
    // Innermost containing scope wins.
    assert_eq!(sm::find_scope_at_cursor(&scopes, 20).map(|s| s.id.as_str()), Some("s1"));
    // Cursor past every scope's end: the scope starting at-or-before the
    // cursor wins, and on from-ties the LATER scope in the list replaces
    // the earlier one (React's `scope.from >= best.from` comparison).
    assert_eq!(sm::find_scope_at_cursor(&scopes, 150).map(|s| s.id.as_str()), Some("s1"));
}

#[test]
fn alias_dot_returns_none_for_zero_column_metadata() {
    // The doc rule: an empty hit must NOT short-circuit the caller's
    // schema-tree fallback — it means "no columns known", not "resolved".
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![rel("orders", &[], &[])]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("orders", Some("o")));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::QualifiedColumn, &["o"], 3), vec![]);

    assert!(
        resolve_alias_dot_completions(
            &["o".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none(),
        "zero-column metadata resolves to None, not to empty completions"
    );
}

#[test]
fn produce_schema_dot_after_an_empty_alias_hit() {
    // `buyer.` where a FROM binding named buyer resolves to a relation
    // with zero known columns: the alias hit is empty, so the schema
    // namespace fallback still offers buyer's tables.
    let snap = snapshot(vec![rel("orders", &[], &[])]);
    let tree = SqlNamespace::branch([(
        "buyer",
        SqlNamespace::branch([("orders", SqlNamespace::table(["id"]))]),
    )]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("orders", Some("buyer")));
    let mdl = model(
        vec![scope],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["buyer"], 7),
        vec![],
    );
    let mut options = SchemaCompletionOptions::new(&mdl, &snap);
    options.schema = Some(&tree);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["\"orders\""],
        "an empty alias hit falls through to the schema namespace"
    );
}

#[test]
fn produce_projection_dedups_same_name_qualifiers() {
    // Two bindings from different namespaces sharing the table name:
    // listVisibleRelations keeps both (different relation keys) but the
    // projection qualifier folds both to `users`, so the columns list
    // once.
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    let mut namespaced = binding("users", None);
    namespaced.relation.namespace_path =
        vec![SqlIdentifierSegment::new("db", false), SqlIdentifierSegment::new("public", false)];
    scope.relations.push(namespaced);
    scope.relations.push(binding("users", None));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::Projection, &[], 40), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    let column_labels: Vec<&str> = out
        .iter()
        .filter(|c| matches!(c.kind, sc::SchemaCompletionKind::Property))
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(column_labels, vec!["id"]);
}

#[test]
fn alias_dot_excludes_a_cte_even_when_the_snapshot_has_the_name() {
    // A CTE named like a real table: resolution finds the CTE binding
    // first, and the source-kind gate keeps it from serving the table's
    // columns.
    let pg = sc::SqlCompletionDialect::for_dialect_id("postgresql");
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.ctes.push(sm::SqlCteBinding::named("users"));
    let mdl = model(
        vec![scope],
        intent(sm::SqlCursorIntentKind::QualifiedColumn, &["users"], 3),
        vec![],
    );

    assert!(
        resolve_alias_dot_completions(
            &["users".to_string()], &mdl, &snap, &pg, "postgresql", None,
            CompletionQuotePolicy::Unquoted,
        )
        .is_none(),
        "a CTE binding never serves the like-named table's columns"
    );
}

#[test]
fn produce_projection_falls_back_when_visible_bindings_resolve_to_nothing() {
    // Visible relations exist but none resolves to metadata and none has
    // an alias: the empty projection result falls through to the
    // all-columns fallback instead of ending the search.
    let snap = snapshot(vec![rel("users", &[], &[("id", "integer", false, true)])]);
    let mut scope = sm::SqlScope::select("s1", 0, 100);
    scope.relations.push(binding("ghosts", None));
    let mdl = model(vec![scope], intent(sm::SqlCursorIntentKind::Projection, &[], 40), vec![]);
    let options = SchemaCompletionOptions::new(&mdl, &snap);

    let out = produce_schema_completions(&options);
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["id"], "the all-columns fallback still runs");
}
