//! Tester's independent re-verification: **which read-only reasons can the
//! user actually see today?**
//!
//! The branch under test ships eleven back-end reason codes (and fourteen on
//! the front end, three of which are page-level facts). The claim to check is
//! about *reachability*, not about the code that produces them:
//!
//! * every driver in the workspace answers the origin contract with the
//!   default implementation, i.e. `Undeterminable` — so the back end can
//!   only ever produce **`UnknownOrigin`**;
//! * one more reason is reachable without any driver help: a read-only
//!   session is checked **before** the origin, so **`SessionReadOnly`**;
//! * the nine shape reasons (`AmbiguousOrigin`, `MultiTable`, `Aggregate`,
//!   `Distinct`, `SetOperation`, `DerivedSource`, `View`, `NoPrimaryKey`,
//!   `KeyNotSelected`) need a `Determined` origin, which needs a driver that
//!   implements the contract — none does today;
//! * the three page-level reasons (`KeyValueNull`, `DuplicateIdentity`,
//!   `RowNotFound`) are front-end facts and need a **writable** back-end
//!   verdict — impossible today.
//!
//! So the answer to "what does the user see?" is two codes, not fourteen.

use datazen_driver_api::{ResultOrigin, UndeterminableReason};
use datazen_feature_core::services::result_edit::{
    evaluate_result_editability, GridReadOnlyReason,
};

fn keys() -> Vec<String> {
    vec!["id".to_string()]
}

/// The reasons the back end can produce without a driver that implements the
/// origin contract.
#[test]
fn only_two_reasons_are_producible_without_a_determined_origin() {
    let mut produced: Vec<GridReadOnlyReason> = Vec::new();

    for reason in [
        &UndeterminableReason::NotImplemented,
        &UndeterminableReason::UnsupportedDialect,
        &UndeterminableReason::MetadataUnavailable,
    ] {
        let verdict = evaluate_result_editability(
            &ResultOrigin::Undeterminable(reason.clone()),
            &keys(),
            true,
            2,
        );
        assert!(!verdict.writable);
        assert_eq!(
            verdict.reason,
            Some(GridReadOnlyReason::UnknownOrigin),
            "{reason:?} must be reported as an unknown origin"
        );
        produced.push(verdict.reason.expect("a reason"));
    }

    // A read-only session is checked before the origin, so it wins even when
    // the driver cannot tell.
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented),
        &keys(),
        false,
        2,
    );
    assert!(!verdict.writable);
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::SessionReadOnly));
    produced.push(verdict.reason.expect("a reason"));

    produced.sort_by_key(|reason| format!("{reason:?}"));
    produced.dedup();
    assert_eq!(
        produced,
        vec![
            GridReadOnlyReason::SessionReadOnly,
            GridReadOnlyReason::UnknownOrigin,
        ],
        "the set of reasons reachable without a driver override"
    );
}

/// The nine shape reasons all need a `Determined` origin — and a `Determined`
/// origin is exactly what no driver produces today.
#[test]
fn the_shape_reasons_are_unreachable_while_every_driver_says_undeterminable() {
    // A `Determined` origin reaches the shape reasons ...
    let determined = ResultOrigin::Determined(datazen_driver_api::ResultOriginDetail {
        shape: datazen_driver_api::ResultShape::MultiTable,
        table: None,
        referenced_tables: 2,
        deduplicated: false,
        aggregated: false,
        set_operation: None,
        has_derived_source: false,
        object_kind: datazen_driver_api::ResultObjectKind::BaseTable,
        columns: vec![
            datazen_driver_api::ResultColumnOrigin {
                column: "id".to_string(),
                passthrough: false,
                table: None,
                source_column: None,
            },
            datazen_driver_api::ResultColumnOrigin {
                column: "total".to_string(),
                passthrough: false,
                table: None,
                source_column: None,
            },
        ],
    });
    let verdict = evaluate_result_editability(&determined, &keys(), true, 2);
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::MultiTable));

    // ... while the origin every driver actually returns reaches none of them.
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented),
        &keys(),
        true,
        2,
    );
    assert_ne!(verdict.reason, Some(GridReadOnlyReason::MultiTable));
}

/// A verdict the back end refuses can never be turned into a writable one by
/// the page: the page-level reasons are front-end facts and need a writable
/// back-end verdict in the first place.
#[test]
fn a_refused_verdict_reports_no_identity_and_no_editable_columns() {
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented),
        &keys(),
        true,
        2,
    );
    assert!(verdict.identity_columns.is_empty());
    assert!(verdict.editable_columns.is_empty());
    assert!(verdict.table.is_none());
}
