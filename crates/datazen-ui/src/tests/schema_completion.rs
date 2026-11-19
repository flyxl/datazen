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
