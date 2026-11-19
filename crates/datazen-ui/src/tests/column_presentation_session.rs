//! Independent audit for T123 (the columnPresentationSession port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/columnPresentationSession.ts`
//! and its React test `__tests__/columnPresentationSession.test.ts` —
//! keyed storage by the full column source, inherit deletes (no-op
//! without an entry: no bump, no notify), clear notifies, the empty
//! session passes the snapshot through with its revision untouched, the
//! merge lets session values win and sums the revisions, and the input
//! snapshot is never mutated.

use std::cell::Cell;

use serde_json::json;

use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::column_presentation_session::{
    ColumnPresentationSession,
};
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::presentation_plan::{
    PresentationEnvironmentSnapshot, ValuePresentationSnapshot, column_override_key,
};

fn source_base() -> ColumnSourceBase<'static> {
    ColumnSourceBase {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: None,
        table: "orders",
    }
}

fn custom_unix_ms() -> ColumnPresentationOverride {
    ColumnPresentationOverride::Custom {
        renderer_id: "datazen.timestamp".to_string(),
        options: json!({
            "interpretation": { "kind": "unix", "unit": "milliseconds" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
        }),
    }
}

fn override_key_for(column: &str) -> String {
    column_override_key(&source_base().with_column(column))
}

fn base_snapshot(
    overrides: Vec<(String, ColumnPresentationOverride)>,
) -> ValuePresentationSnapshot {
    ValuePresentationSnapshot {
        revision: 3,
        environment: PresentationEnvironmentSnapshot {
            locale: "en-US".to_string(),
            local_time_zone: "UTC".to_string(),
        },
        configs: Default::default(),
        enabled_renderer_ids: Vec::new(),
        overrides: overrides.into_iter().collect(),
        rules: Vec::new(),
    }
}

#[test]
fn stores_raw_and_custom_keyed_by_the_full_source() {
    let mut session = ColumnPresentationSession::new();
    session.set_column_override(
        source_base(),
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
    );
    let state = session.state();
    assert_eq!(state.revision, 1);
    assert_eq!(
        state.overrides.get(&override_key_for("epoch_ms")),
        Some(&ColumnPresentationOverride::Raw)
    );
    // The same table name under another connection is a different key.
    session.set_column_override(
        ColumnSourceBase { connection_id: "conn-2", ..source_base() },
        "epoch_ms",
        &ColumnPresentationOverride::Raw,
    );
    assert_eq!(session.state().overrides.len(), 2);
}

#[test]
fn saving_inherit_deletes_the_entry() {
    let mut session = ColumnPresentationSession::new();
    session.set_column_override(source_base(), "epoch_ms", &ColumnPresentationOverride::Raw);
    assert_eq!(session.state().revision, 1);
    session.set_column_override(
        source_base(),
        "epoch_ms",
        &ColumnPresentationOverride::Inherit,
    );
    let state = session.state();
    assert!(state.overrides.is_empty());
    assert_eq!(state.revision, 2);
}

#[test]
fn saving_inherit_without_entry_is_a_no_op() {
    thread_local! {
        static NOTIFIED: Cell<usize> = const { Cell::new(0) };
    }
    NOTIFIED.with(|cell| cell.set(0));
    let mut session = ColumnPresentationSession::new();
    let subscription = session.subscribe(Box::new(|| {
        NOTIFIED.with(|cell| cell.set(cell.get() + 1));
    }));
    session.set_column_override(
        source_base(),
        "epoch_ms",
        &ColumnPresentationOverride::Inherit,
    );
    assert_eq!(session.state().revision, 0);
    assert_eq!(NOTIFIED.with(|cell| cell.get()), 0);
    session.unsubscribe(subscription);
}

#[test]
fn clear_removes_the_entry_and_notifies() {
    thread_local! {
        static NOTIFIED: Cell<usize> = const { Cell::new(0) };
    }
    NOTIFIED.with(|cell| cell.set(0));
    let mut session = ColumnPresentationSession::new();
    session.set_column_override(source_base(), "epoch_ms", &ColumnPresentationOverride::Raw);
    let subscription = session.subscribe(Box::new(|| {
        NOTIFIED.with(|cell| cell.set(cell.get() + 1));
    }));
    session.clear_column_override(source_base(), "epoch_ms");
    assert!(session.state().overrides.is_empty());
    assert_eq!(NOTIFIED.with(|cell| cell.get()), 1);
    session.unsubscribe(subscription);
}

#[test]
fn empty_session_passes_the_snapshot_through() {
    let session = ColumnPresentationSession::new();
    let snapshot = base_snapshot(Vec::new());
    let merged = session.apply_session_column_overrides(&snapshot);
    // Identity is JS-only; the pinned behavior is equality + the same
    // revision (the passthrough invariant).
    assert_eq!(merged.revision, snapshot.revision);
    assert_eq!(merged.overrides, snapshot.overrides);
}

#[test]
fn merge_lets_session_values_win_and_sums_revisions() {
    let mut session = ColumnPresentationSession::new();
    let snapshot = base_snapshot(vec![(
        override_key_for("epoch_ms"),
        ColumnPresentationOverride::Raw,
    )]);
    session.set_column_override(source_base(), "epoch_ms", &custom_unix_ms());
    session.set_column_override(source_base(), "other", &ColumnPresentationOverride::Raw);

    let merged = session.apply_session_column_overrides(&snapshot);
    assert_eq!(merged.revision, 3 + 2);
    assert_eq!(
        merged.overrides.get(&override_key_for("epoch_ms")),
        Some(&custom_unix_ms()),
        "the session value must win over the snapshot's own"
    );
    assert_eq!(
        merged.overrides.get(&override_key_for("other")),
        Some(&ColumnPresentationOverride::Raw)
    );
    // The base snapshot is not mutated.
    assert_eq!(
        snapshot.overrides.get(&override_key_for("epoch_ms")),
        Some(&ColumnPresentationOverride::Raw)
    );
    assert_eq!(snapshot.revision, 3);
}
