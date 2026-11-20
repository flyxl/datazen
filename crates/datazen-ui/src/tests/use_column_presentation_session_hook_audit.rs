//! T158 audit: the useColumnPresentationSession hook port, verified
//! against the React source re-read line by line
//! (`hooks/useColumnPresentationSession.ts`, its store dependency
//! `columnPresentationSession.ts` — T09's port — and the React test
//! `hooks/__tests__/useColumnPresentationSession.test.tsx`, every case
//! mirrored, with the memoization the React suite cannot observe added
//! as the merge-count pin).
//!
//! Tester-written: the coder's inline suites are treated as a black box;
//! every expectation below is re-derived from the React files.

use crate::hooks::use_column_presentation_session::ColumnPresentationSessionHook;
use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::column_presentation_session::ColumnPresentationSession;
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::presentation_plan::{
    PresentationEnvironmentSnapshot, ValuePresentationSnapshot, column_override_key,
};

fn source() -> ColumnSourceBase<'static> {
    ColumnSourceBase {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: Some("public"),
        table: "orders",
    }
}

fn base() -> ValuePresentationSnapshot {
    ValuePresentationSnapshot {
        revision: 2,
        environment: PresentationEnvironmentSnapshot {
            locale: "en-US".into(),
            local_time_zone: "UTC".into(),
        },
        configs: Default::default(),
        enabled_renderer_ids: Vec::new(),
        overrides: Default::default(),
        rules: Vec::new(),
    }
}

/// React test 1: while the session is empty the hook returns the base
/// snapshot itself — same revision, equal content (the T09 port's ruling
/// on JS identity), and the memo does not recompute on re-reads.
#[test]
fn empty_session_returns_the_base_snapshot() {
    let mut session = ColumnPresentationSession::new();
    let mut hook = ColumnPresentationSessionHook::new();
    let base = base();

    let merged = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(merged, base);
    assert_eq!(merged.revision, 2);
    // The React identity check (`toBe(BASE)`) is equality here; the
    // memoization pin: re-reading with unchanged inputs does not re-merge.
    assert_eq!(hook.merge_count, 1);
    let again = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(again, base);
    assert_eq!(hook.merge_count, 1, "unchanged inputs must not re-merge");
}

/// React test 2: a session write merges the override in, bumps the
/// revision (base 2 + session 1), and the change is visible on the next
/// read (the next frame here — useSyncExternalStore's re-render).
#[test]
fn session_write_merges_and_bumps_the_revision() {
    let mut session = ColumnPresentationSession::new();
    let mut hook = ColumnPresentationSessionHook::new();
    let base = base();

    let before = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(before, base);

    session.set_column_override(source(), "epoch_ms", &ColumnPresentationOverride::Raw);
    let after = hook.session_value_presentation_snapshot(&base, &session);
    assert_ne!(after, base);
    assert_eq!(after.revision, 3);
    let key = column_override_key(&source().with_column("epoch_ms"));
    assert_eq!(after.overrides.get(&key), Some(&ColumnPresentationOverride::Raw));
    // The memo recomputed exactly once more (one session revision bump).
    assert_eq!(hook.merge_count, 2);
}

/// React test 3: after the session is reset (a fresh store — the test
/// isolation reset), the hook returns the base snapshot again.
#[test]
fn fresh_session_returns_to_the_base_snapshot() {
    let mut session = ColumnPresentationSession::new();
    let mut hook = ColumnPresentationSessionHook::new();
    let base = base();

    session.set_column_override(source(), "epoch_ms", &ColumnPresentationOverride::Raw);
    let merged = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(merged.revision, 3);

    // `resetColumnPresentationSession`: a fresh store (revision 0).
    let fresh = ColumnPresentationSession::new();
    let after = hook.session_value_presentation_snapshot(&base, &fresh);
    assert_eq!(after, base);
    assert_eq!(after.revision, 2);
    // ...and the memo followed the session-identity change (revision 0
    // differs from 1), not just the equal result.
    assert_eq!(hook.merge_count, 2);
}

/// PRD acceptance 2: the grid and the aside read the same session store
/// through their own hook instances and see the same merged snapshot —
/// the two-consumer consistency the hook exists for.
#[test]
fn grid_and_aside_see_the_same_merged_snapshot() {
    let mut session = ColumnPresentationSession::new();
    let mut grid = ColumnPresentationSessionHook::new();
    let mut aside = ColumnPresentationSessionHook::new();
    let base = base();

    session.set_column_override(source(), "created_at", &ColumnPresentationOverride::Raw);
    let grid_snapshot = grid.session_value_presentation_snapshot(&base, &session);
    let aside_snapshot = aside.session_value_presentation_snapshot(&base, &session);
    assert_eq!(grid_snapshot, aside_snapshot);
    assert_eq!(grid_snapshot.revision, 3);
    // `useColumnPresentationSessionState`: both consumers' plain reads
    // also agree (the shared store).
    assert_eq!(grid.session_state(&session), aside.session_state(&session));
    assert_eq!(grid.session_state(&session).revision, 1);

    // A later write invalidates both memos on their next frame.
    session.set_column_override(source(), "updated_at", &ColumnPresentationOverride::Raw);
    let grid_next = grid.session_value_presentation_snapshot(&base, &session);
    let aside_next = aside.session_value_presentation_snapshot(&base, &session);
    assert_eq!(grid_next, aside_next);
    assert_eq!(grid_next.revision, 4);
    assert_eq!(grid.merge_count, 2);
    assert_eq!(aside.merge_count, 2);
}

/// The memo's base dependency: a NEW base content (the host rebuilt its
/// snapshot) recomputes even when the session did not move — React's
/// `useMemo` dep on the base object. An equal-valued base does not.
#[test]
fn memo_follows_the_base_snapshot_identity() {
    let mut session = ColumnPresentationSession::new();
    let mut hook = ColumnPresentationSessionHook::new();
    let base = base();

    session.set_column_override(source(), "epoch_ms", &ColumnPresentationOverride::Raw);
    let _ = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(hook.merge_count, 1);

    // Same content: memo hit.
    let same = base.clone();
    let _ = hook.session_value_presentation_snapshot(&same, &session);
    assert_eq!(hook.merge_count, 1);

    // Different content (a new revision from the host): recompute, and
    // the merged revision is the new base's + the session's.
    let mut rebuilt = base.clone();
    rebuilt.revision = 5;
    let merged = hook.session_value_presentation_snapshot(&rebuilt, &session);
    assert_eq!(hook.merge_count, 2);
    assert_eq!(merged.revision, 6);
}

/// Session mutations that do not change the state must not invalidate the
/// memo: saving `inherit` with no stored entry is a documented no-op (no
/// revision bump, no notification).
#[test]
fn no_op_writes_do_not_invalidate_the_memo() {
    let mut session = ColumnPresentationSession::new();
    let mut hook = ColumnPresentationSessionHook::new();
    let base = base();

    let _ = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(hook.merge_count, 1);

    // inherit with nothing stored: no bump, no notify, no recompute.
    session.set_column_override(source(), "epoch_ms", &ColumnPresentationOverride::Inherit);
    assert_eq!(session.revision(), 0);
    let _ = hook.session_value_presentation_snapshot(&base, &session);
    assert_eq!(hook.merge_count, 1);
}
