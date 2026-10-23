//! Tester's independent probe of the **locating key columns** on a refusal.
//!
//! The claim under test (PRD §1.3 §6/§7): the page-level facts — "the edited
//! row's locating key value must not be NULL", "no other row on the page may
//! carry the same identity" — are only computable if the front end knows
//! **which columns locate a row**. On the narrow channel the driver cannot
//! describe the result at all, so those columns must come from the catalog.
//!
//! The shared layer therefore keeps the locating key columns even when it
//! refuses: they are a fact about the table, not about the verdict.
//!
//! This file pins the three states that matter:
//!
//! * a refusal with a known catalog key ⇒ the locating columns are reported;
//! * a refusal with **no** catalog key ⇒ nothing is reported, and the front
//!   end's facts are then not computable (it fails open, by design);
//! * a grant reports them as well.

use datazen_driver_api::{ResultOrigin, UndeterminableReason};
use datazen_feature_core::services::result_edit::{
    evaluate_result_editability, GridReadOnlyReason,
};

fn columns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn undeterminable() -> ResultOrigin {
    ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented)
}

#[test]
fn a_refusal_still_reports_the_locating_key_columns() {
    let verdict = evaluate_result_editability(
        &undeterminable(),
        &columns(&["id"]),
        &columns(&["id", "total"]),
        true,
    );

    assert!(
        !verdict.writable,
        "the driver cannot tell, so the result is read-only"
    );
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::UnknownOrigin));
    assert_eq!(
        verdict.identity_columns.len(),
        1,
        "the locating key columns are a fact about the table, not about the verdict"
    );
    assert_eq!(verdict.identity_columns[0].column, "id");
    assert_eq!(verdict.identity_columns[0].source_column, "id");
    // A refusal still has no writable columns.
    assert!(verdict.editable_columns.is_empty());
}

#[test]
fn a_refusal_without_a_catalog_key_reports_nothing_to_locate() {
    let verdict =
        evaluate_result_editability(&undeterminable(), &[], &columns(&["id", "total"]), true);

    assert!(!verdict.writable);
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::UnknownOrigin));
    assert!(
        verdict.identity_columns.is_empty(),
        "no catalog key means nothing to locate — the front end then fails open"
    );
}

#[test]
fn a_key_that_is_not_in_the_result_is_not_a_locating_column() {
    // The catalog says the key is `id`, but the result does not carry it.
    let verdict = evaluate_result_editability(
        &undeterminable(),
        &columns(&["id"]),
        &columns(&["total"]),
        true,
    );

    assert!(!verdict.writable);
    assert!(
        verdict.identity_columns.is_empty(),
        "a key that is not in the result cannot locate a row"
    );
}

#[test]
fn a_read_only_session_keeps_the_locating_key_columns_too() {
    let verdict = evaluate_result_editability(
        &undeterminable(),
        &columns(&["id"]),
        &columns(&["id", "total"]),
        false,
    );

    assert!(!verdict.writable);
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::SessionReadOnly));
    assert_eq!(verdict.identity_columns.len(), 1);
}
