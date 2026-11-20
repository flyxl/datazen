//! Port of `packages/frontend/src/hooks/useColumnPresentationSession.ts`
//! (issue-32 T09): the hook seam through which column-override sessions
//! take effect.
//!
//! The grid (DataTable) and the aside (DetailPanel) each call
//! `useSessionValuePresentationSnapshot(baseSnapshot)`, merging the
//! in-process session overrides (columnPresentationSession.ts) into the
//! host snapshot and building their plans from the merged result. Both
//! read the same session store and the same merge function, so one
//! column override is instantly consistent between cell and aside (PRD
//! acceptance 2); when T10-style persistence changes the input source,
//! only the merge's input moves — the call sites do not.
//!
//! ## What maps where
//!
//! - `useSyncExternalStore(subscribe, get, get)` — the engine's frame
//!   loop IS the subscription: views re-run per frame and re-read the
//!   session, so a revision bump is picked up on the next frame without a
//!   re-render scheduler. The session's listener registry (T09's port)
//!   remains for host-triggered relayout.
//! - `useMemo(applySessionColumnOverrides(base, session), [base,
//!   session])` becomes the owned memo below: recompute only when the
//!   base's content changed (React keys on object identity; `PartialEq`
//!   on the snapshot is the ownership world's stand-in — an equal base
//!   yields an equal merge, so skipping the recompute cannot change the
//!   result) or the session revision moved (every mutation bumps it, so
//!   "revision moved" is exactly "the session state object changed").
//! - An empty session returns the base snapshot (T09's port already
//!   rules the JS identity optimization out: same revision, equal
//!   content).
//!
//! Known deltas: none behavioural — the merge itself is T09's
//! [`ColumnPresentationSession::apply_session_column_overrides`], the
//! single merge point, reused verbatim.

use crate::support::value_presentation::column_presentation_session::{
    ColumnPresentationSession, ColumnPresentationSessionState,
};
use crate::support::value_presentation::presentation_plan::ValuePresentationSnapshot;

/// One memo entry: the inputs consumed and the merged result cached.
struct SessionSnapshotMemo {
    /// The base the memo merged (React's `baseSnapshot` dep).
    base: ValuePresentationSnapshot,
    /// The session revision the memo merged (React's `session` dep; the
    /// state's identity proxy).
    session_revision: u64,
    /// `useMemo`'s cached result.
    merged: ValuePresentationSnapshot,
}

/// The hook's state: the `useMemo` cache for
/// `useSessionValuePresentationSnapshot`. Each consuming surface (grid,
/// aside) owns one; the session store itself is shared (AppState).
#[derive(Default)]
pub(crate) struct ColumnPresentationSessionHook {
    memo: Option<SessionSnapshotMemo>,
    /// How many times the merge has actually run — the observable half of
    /// the memoization (tests pin that an unchanged input pair does not
    /// recompute).
    pub(crate) merge_count: u64,
}

impl ColumnPresentationSessionHook {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `useColumnPresentationSessionState` — subscribe to the session
    /// override store and read it (a revision change is a re-render in
    /// React, a next-frame read here).
    pub(crate) fn session_state(
        &self,
        session: &ColumnPresentationSession,
    ) -> ColumnPresentationSessionState {
        session.state()
    }

    /// `useSessionValuePresentationSnapshot` — merge the session
    /// overrides into the host snapshot; an empty session returns the
    /// base snapshot (same revision, equal content). The memo recomputes
    /// only when either input changed since the last call.
    pub(crate) fn session_value_presentation_snapshot(
        &mut self,
        base: &ValuePresentationSnapshot,
        session: &ColumnPresentationSession,
    ) -> ValuePresentationSnapshot {
        let session_revision = session.revision();
        if let Some(memo) = &self.memo {
            if memo.session_revision == session_revision && &memo.base == base {
                return memo.merged.clone();
            }
        }
        let merged = session.apply_session_column_overrides(base);
        self.merge_count += 1;
        self.memo = Some(SessionSnapshotMemo {
            base: base.clone(),
            session_revision,
            merged: merged.clone(),
        });
        merged
    }
}
