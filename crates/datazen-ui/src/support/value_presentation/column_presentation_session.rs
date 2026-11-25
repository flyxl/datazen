//! Port of `packages/frontend/src/lib/valuePresentation/columnPresentationSession.ts`
//! (issue-32 T09, design §7/§9) plus the area's `index.ts` re-export shell
//! (X-class: its duty is the area `mod.rs`'s module surface, which the
//! earlier tasks already wired).
//!
//! The session-effect seam for column overrides: the panel's saves write
//! here first (optimistic — a failed persist keeps the user's fresh
//! choice), and the snapshot consumer merges the session over the
//! persisted snapshot so grid, aside and preview agree.
//!
//! This is **not** a persistence layer: no settings store, no backend, no
//! disk. Refresh/restart loses session overrides; persistence is
//! `columnOverrides.ts` (T037), which also clears the same key's session
//! entry after a successful save.
//!
//! Semantics (React verbatim):
//! - `inherit` stores nothing: saving inherit deletes the column's session
//!   entry (clear-override-restores-inheritance, PRD acceptance 4); with
//!   no entry it is a no-op — no revision bump, no notification.
//! - `raw` / `custom` store one entry per [`column_override_key`].
//! - The merge: session values win over the snapshot's own overrides; an
//!   empty session returns the snapshot unchanged (same revision — the
//!   revision-passthrough invariant the snapshot tests pin).
//! - revision = snapshot revision + session revision: both only grow, so
//!   the merged revision grows monotonically and plan caches invalidate.
//!
//! # What maps where
//!
//! - React's module-level mutable singleton + `useSyncExternalStore`
//!   subscription becomes an owned [`ColumnPresentationSession`] struct
//!   the host holds (AppState / the snapshot hook); `reset` for test
//!   isolation becomes simply constructing a fresh instance — no global
//!   to clean. Listeners unsubscribe via tokens (the registry's pattern).
//! - The empty-session identity return ("the very same object") becomes
//!   an equal clone with the same revision — Rust ownership replaces the
//!   JS identity optimization; the pinned behavior is that the revision
//!   passes through untouched.

use std::collections::BTreeMap;

use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::presentation_plan::{
    ValuePresentationSnapshot, column_override_key,
};

/// A read-only view of the session state (React
/// `ColumnPresentationSessionState`).
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnPresentationSessionState {
    pub overrides: BTreeMap<String, ColumnPresentationOverride>,
    pub revision: u64,
}

/// A subscription release handle (single-ownership, like the registry's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionSubscription {
    token: u64,
}

struct Listener {
    token: u64,
    callback: Box<dyn Fn()>,
}

/// The in-process session override store (React's module singleton,
/// owned).
#[derive(Default)]
pub struct ColumnPresentationSession {
    overrides: BTreeMap<String, ColumnPresentationOverride>,
    revision_counter: u64,
    listeners: Vec<Listener>,
    next_token: u64,
}

impl ColumnPresentationSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current state (a snapshot; callers cannot mutate the store
    /// through it).
    pub fn state(&self) -> ColumnPresentationSessionState {
        ColumnPresentationSessionState {
            overrides: self.overrides.clone(),
            revision: self.revision_counter,
        }
    }

    /// The revision alone — the hook layer's poll key (T158). Cheaper
    /// than [`Self::state`], which clones the override map for every
    /// read; every mutation bumps the revision, so "revision moved" is
    /// exactly "the state object changed" (React's `useSyncExternalStore`
    /// re-render condition).
    pub fn revision(&self) -> u64 {
        self.revision_counter
    }

    /// Subscribe to change notifications; the handle unsubscribes.
    pub fn subscribe(&mut self, callback: Box<dyn Fn()>) -> SessionSubscription {
        self.next_token += 1;
        let token = self.next_token;
        self.listeners.push(Listener { token, callback });
        SessionSubscription { token }
    }

    pub fn unsubscribe(&mut self, handle: SessionSubscription) {
        self.listeners.retain(|listener| listener.token != handle.token);
    }

    /// Save one column's session override. `inherit` deletes the entry
    /// (a no-op without one: no bump, no notify); `raw`/`custom` store by
    /// source key. Callers have already validated the override.
    pub fn set_column_override(
        &mut self,
        source: ColumnSourceBase<'_>,
        column_name: &str,
        override_value: &ColumnPresentationOverride,
    ) {
        let key = column_override_key(&source.with_column(column_name));
        if matches!(override_value, ColumnPresentationOverride::Inherit) {
            if !self.overrides.contains_key(&key) {
                return;
            }
            self.overrides.remove(&key);
            self.changed();
            return;
        }
        self.overrides.insert(key, override_value.clone());
        self.changed();
    }

    /// Clear one column's session override (saving `inherit`).
    pub fn clear_column_override(
        &mut self,
        source: ColumnSourceBase<'_>,
        column_name: &str,
    ) {
        self.set_column_override(source, column_name, &ColumnPresentationOverride::Inherit);
    }

    /// The single merge point: session overrides over the snapshot's own,
    /// session values winning; the merged revision is the sum (both parts
    /// only grow, so the sum grows monotonically). An empty session
    /// returns the snapshot with its revision untouched.
    pub fn apply_session_column_overrides(
        &self,
        snapshot: &ValuePresentationSnapshot,
    ) -> ValuePresentationSnapshot {
        if self.revision_counter == 0 || self.overrides.is_empty() {
            return snapshot.clone();
        }
        let mut overrides = snapshot.overrides.clone();
        for (key, value) in &self.overrides {
            overrides.insert(key.clone(), value.clone());
        }
        ValuePresentationSnapshot {
            revision: snapshot.revision + self.revision_counter,
            overrides,
            ..snapshot.clone()
        }
    }

    fn changed(&mut self) {
        self.revision_counter += 1;
        let listeners = std::mem::take(&mut self.listeners);
        for Listener { callback, .. } in &listeners {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
        }
        self.listeners = listeners;
    }
}
