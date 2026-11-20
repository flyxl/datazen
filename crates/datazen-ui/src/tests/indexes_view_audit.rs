//! Independent audit for T448: `IndexesView` port.
//!
//! Fill branch coverage the coder does not drive: the builtin dialect method
//! sets / quote chars, the pure name helpers, and the error alert role.

use crate::views::indexes_view::{
    build_create_index_sql, build_drop_index_sql, quote_char, suggested_index_name,
    supported_methods, IndexMethod, IndexesIdentity, IndexesViewState, ERROR_KEY,
};

#[test]
fn builtin_dialect_method_sets_and_quote_chars_are_pinned() {
    assert_eq!(supported_methods("postgresql"), vec![
        IndexMethod::BTree, IndexMethod::Hash, IndexMethod::Gin, IndexMethod::Gist
    ]);
    assert_eq!(supported_methods("mysql"), vec![IndexMethod::BTree, IndexMethod::Hash]);
    assert_eq!(supported_methods("mariadb"), vec![IndexMethod::BTree, IndexMethod::Hash]);
    assert_eq!(supported_methods("sqlite"), vec![IndexMethod::BTree]);
    assert_eq!(quote_char("mysql"), "`");
    assert_eq!(quote_char("mariadb"), "`");
    assert_eq!(quote_char("postgresql"), "\"");
    assert_eq!(quote_char("sqlite"), "\"");
}

#[test]
fn standard_index_dialects_are_complete() {
    // React's `standardIndex(dropPattern)` (extra.ts:71-87): methods
    // `['btree','hash']`, one create form, and a drop pattern per dialect.
    let cols = vec!["id".to_owned()];
    assert!(
        !build_create_index_sql("sqlserver", "users", "idx_foo", &cols, false, IndexMethod::BTree)
            .is_empty(),
        "React offers Hash on SQL Server, so the dialect must build create SQL"
    );
    let standard = [
        "sqlserver",
        "clickhouse",
        "duckdb",
        "oracle",
        "elasticsearch",
        "mongodb",
        "generic",
    ];
    for dialect in standard {
        assert_eq!(
            supported_methods(dialect),
            vec![IndexMethod::BTree, IndexMethod::Hash],
            "standardIndex methods on {dialect}"
        );
        assert_eq!(
            build_create_index_sql(
                dialect,
                "users",
                "idx_foo",
                &cols,
                false,
                IndexMethod::BTree
            ),
            "CREATE INDEX \"idx_foo\" ON \"users\" (\"id\")",
            "standardIndex create form on {dialect}"
        );
        let expected_drop = if matches!(dialect, "sqlserver" | "clickhouse") {
            "DROP INDEX \"idx_foo\" ON \"users\""
        } else {
            "DROP INDEX \"idx_foo\""
        };
        assert_eq!(
            build_drop_index_sql(dialect, "idx_foo", "users"),
            expected_drop,
            "standardIndex drop pattern on {dialect}"
        );
    }
}

#[test]
fn suggested_and_effective_names_follow_the_react_helpers() {
    assert_eq!(suggested_index_name("  users  ", &[]), "idx_users");
    assert_eq!(suggested_index_name("   ", &[]), "idx_table");
    assert_eq!(
        suggested_index_name("users", &["email".to_owned(), "id".to_owned()]),
        "idx_users_email_id"
    );

    let mut state = IndexesViewState::default();
    state.index_name = rune::TextEdit::new("  ".to_owned());
    state.selected_cols = vec!["email".to_owned()];
    assert_eq!(
        state.effective_name("users"),
        "idx_users_email",
        "blank typed name falls back to the auto name"
    );
    state.index_name = rune::TextEdit::new(" custom ".to_owned());
    assert_eq!(state.effective_name("users"), "custom");
}

#[test]
fn load_failure_falls_back_to_the_translated_message_and_clears_both_lists() {
    let mut state = IndexesViewState::default();
    state.loading = true;
    state.submitting = true;
    state.apply_load(Err(String::new()));
    assert!(!state.loading && !state.submitting);
    assert_eq!(state.error, crate::i18n::t("indexes.loadFailed"));
    assert!(state.indexes.is_empty() && state.columns.is_empty());

    state.apply_load(Err("boom".to_owned()));
    assert_eq!(state.error, "boom", "a concrete message is preserved verbatim");
    assert_eq!(ERROR_KEY, "idx-error");
}
