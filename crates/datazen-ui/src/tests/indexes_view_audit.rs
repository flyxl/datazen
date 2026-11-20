//! Independent audit for T448: `IndexesView` port.
//!
//! Fill branch coverage the coder does not drive: the builtin dialect method
//! sets / quote chars, the pure name helpers, and the error alert role.

use crate::views::indexes_view::{
    quote_char, suggested_index_name, supported_methods, IndexMethod, IndexesIdentity,
    IndexesViewState, ERROR_KEY,
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
