//! Port of `packages/frontend/src/hooks/useValuePresentationSnapshot.ts`
//! (T451) — the snapshot provider hook (issue-32 T06, design §6/§7).
//!
//! 面板层共享 hook：快照由本 hook 产出，再由提供方
//! （PanelContentRenderer / ContentView / ResultTableView）经 props
//! 注入 DataTable 与 DetailPanel；组件本身不直接读 Store 或 IPC
//! （快照之外的读取）。
//!
//! ## What maps where
//!
//! - The stored-value read
//!   (`useSettingsStore(s => s.settings.valuePresentation)`) + the
//!   revision read (`s.valuePresentationRevision ?? 0`) are the
//!   state's `stored_raw` + `settings_revision`: the store bumps the
//!   counter only when the valuePresentation JSON CHANGES (the O-5
//!   revision invariant that counter carries — an unchanged save
//!   does not move it).
//! - The defensive lenient parse
//!   (`parseStoredValuePresentationSettings(storedRaw)` — mocked /
//!   pre-T06 store states may lack the field entirely; the parse
//!   coerces unknown input to the default instead of throwing) is
//!   the [`Self::reconcile`] fold's first step (T184's own parse).
//! - The registry revision subscription (the T04 hook's own
//!   `useSyncExternalStore` shape) is the `registry_revision`
//!   report ([`Self::note_registry_revision`]) — a register or
//!   unregister bumps it and the next reconcile rebuilds.
//! - `environment = resolvePresentationEnvironment(settings.language)`
//!   is T157's resolver over the normalized UI language ('en' /
//!   'zh-CN'; its own fallback covers the absent case).
//! - The snapshot memo (deps: the parsed identity, the environment,
//!   the registry revision, the settings revision — the registry
//!   instance itself is stable) is [`Self::reconcile`]: the build
//!   runs through T184's `build_value_presentations_snapshot` only
//!   when one of the four carriers changed.
//!
//! Known deltas: the store and registry reads arrive as method
//! parameters (the host owns the store); `build_value…`'s registry
//! parameter is the default-contributions registry (React's global
//! `valuePresentationRegistry` after the bootstrap registers the
//! timestamp contribution).

// No production consumer yet (the panel provider track wires it);
// this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde_json::{Value, json};

use crate::support::value_presentation::presentation_plan::{
    PresentationEnvironmentSnapshot, ValuePresentationSnapshot,
    resolve_presentation_environment,
};
use crate::support::value_presentation::settings::{
    build_value_presentation_snapshot, parse_stored_value_presentation_settings,
};

/// `useValuePresentationSnapshot()`'s state machine.
#[derive(Clone, Debug, PartialEq)]
pub struct ValuePresentationSnapshotHookState {
    /// The store's raw stored value (`settings.valuePresentation`).
    stored_raw: Value,
    /// The store's revision counter (`valuePresentationRevision ??
    /// 0`) — bumped only when the JSON changes (O-5).
    settings_revision: u64,
    /// The normalized UI language.
    language: String,
    /// The registry revision last observed.
    registry_revision: u64,
    /// The memo's dependency bundle the last build ran under (the
    /// four carriers: the stored JSON, the environment, the
    /// registry revision, the settings revision).
    built_under: Option<(String, String, u64, u64)>,
    /// The built snapshot (the memo's output; the default snapshot
    /// before the first build — revision 0, empty collections: the
    /// loading state).
    snapshot: ValuePresentationSnapshot,
}

impl Default for ValuePresentationSnapshotHookState {
    fn default() -> Self {
        Self {
            stored_raw: json!({ "version": 1, "renderers": {} }),
            settings_revision: 0,
            language: "en".to_owned(),
            registry_revision: 0,
            built_under: None,
            snapshot: ValuePresentationSnapshot::default_snapshot(
                resolve_presentation_environment(None),
            ),
        }
    }
}

impl ValuePresentationSnapshotHookState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The store's change report: the raw stored value, its revision
    /// counter and the UI language (the three store reads).
    pub fn note_store(
        &mut self,
        stored_raw: Value,
        settings_revision: u64,
        language: &str,
    ) {
        self.stored_raw = stored_raw;
        self.settings_revision = settings_revision;
        self.language = language.to_owned();
    }

    /// The registry's revision report (the external store's change
    /// notification).
    pub fn note_registry_revision(&mut self, revision: u64) {
        self.registry_revision = revision;
    }

    /// The observed registry revision.
    pub fn registry_revision(&self) -> u64 {
        self.registry_revision
    }

    /// The observed settings revision (`?? 0`).
    pub fn settings_revision(&self) -> u64 {
        self.settings_revision
    }

    /// The snapshot (the memo's output; the default snapshot before
    /// the first reconcile — temporal columns answer `disabled` and
    /// render with the default display).
    pub fn snapshot(&self) -> &ValuePresentationSnapshot {
        &self.snapshot
    }

    /// The memo's reconcile: parse the stored value leniently,
    /// resolve the environment from the language, and rebuild the
    /// snapshot through T184's builder — but only when one of the
    /// four carriers changed (the memo's own dependency bundle:
    /// the stored JSON, the environment, the registry revision,
    /// the settings revision; equal bundle, equal snapshot, no
    /// rebuild). Answers whether the build ran.
    pub fn reconcile(&mut self) -> bool {
        let environment = self.environment();
        let bundle = (
            self.stored_raw.to_string(),
            format!("{}|{}", environment.locale, environment.local_time_zone),
            self.registry_revision,
            self.settings_revision,
        );
        if self.built_under.as_ref() == Some(&bundle) {
            return false;
        }
        // The defensive parse: unknown input coerces to the default
        // instead of throwing (mocked / pre-T06 store states).
        let stored = parse_stored_value_presentation_settings(&self.stored_raw);
        self.snapshot = build_value_presentation_snapshot(
            &stored,
            &environment,
            &default_registry(),
            self.settings_revision,
        );
        self.built_under = Some(bundle);
        true
    }

    /// `resolvePresentationEnvironment(language)` — the environment
    /// the build uses.
    fn environment(&self) -> PresentationEnvironmentSnapshot {
        let locale = if self.language.is_empty() { None } else { Some(self.language.as_str()) };
        resolve_presentation_environment(locale)
    }
}

/// The registry the snapshot builds against: the default registry
/// with the bundled contributions registered (React's global
/// `valuePresentationRegistry` after the bootstrap).
fn default_registry() -> crate::support::value_presentation::registry::ValuePresentationRegistry {
    crate::support::value_presentation::presentation_plan::registry_with_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_hook_answers_the_default_snapshot() {
        let hook = ValuePresentationSnapshotHookState::new();
        let snapshot = hook.snapshot();
        assert_eq!(snapshot.revision, 0, "the loading state");
        assert!(snapshot.configs.is_empty());
        assert!(snapshot.enabled_renderer_ids.is_empty());
        assert!(snapshot.rules.is_empty());
        assert_eq!(hook.settings_revision(), 0);
        assert_eq!(hook.registry_revision(), 0);
    }

    #[test]
    fn the_reconcile_builds_over_the_store_reads() {
        let mut hook = ValuePresentationSnapshotHookState::new();
        // The accessors report the observed carriers.
        assert_eq!(hook.settings_revision(), 0);
        assert_eq!(hook.registry_revision(), 0);
        // The default store: the parse's default shape, the build
        // lands the revision the counter carries.
        assert!(hook.reconcile());
        assert_eq!(hook.snapshot().revision, 0);
        // A store change with a bumped counter: the revision rides.
        hook.note_store(
            json!({ "version": 1, "renderers": {} }),
            5,
            "zh-CN",
        );
        assert_eq!(hook.settings_revision(), 5, "the observed counter");
        assert!(hook.reconcile());
        assert_eq!(hook.snapshot().revision, 5, "O-5: the counter carries the change");
        assert_eq!(hook.snapshot().environment.locale, "zh-CN");
        // An unchanged store: nothing moves (the caller may still
        // reconcile; the snapshot's equality is the memo).
        assert!(!hook.reconcile(), "the memo answers no-change");
        // The revision carrier alone (a re-save of the same JSON
        // still bumps only on change; the snapshot only moves with
        // the carriers).
        hook.note_registry_revision(3);
        assert!(hook.reconcile(), "the registry revision rebuilds");
        assert_eq!(hook.snapshot().revision, 5, "the settings revision is unchanged");
        assert_eq!(hook.registry_revision(), 3);
    }

    #[test]
    fn a_malformed_store_value_coerces_instead_of_throwing() {
        let mut hook = ValuePresentationSnapshotHookState::new();
        // Mocked / pre-T06 shapes: a string, a number, an array —
        // the lenient parse coerces every one to the default.
        for raw in [json!("junk"), json!(7), json!([1, 2])] {
            hook.note_store(raw.clone(), 1, "en");
            assert!(hook.reconcile());
            assert_eq!(
                hook.snapshot().revision, 1,
                "the build lands over the coerced default (raw: {raw})"
            );
            assert!(hook.snapshot().configs.is_empty());
        }
        // An unknown version: the empty snapshot; the original stays
        // in the store and is never written back with defaults
        // (even with a valid rule present — its future shape is
        // unknown, so nothing is interpreted).
        hook.note_store(
            json!({
                "version": 9,
                "renderers": { "datazen.timestamp": { "enabled": true, "options": {} } },
                "rules": [ {
                    "name": "r1",
                    "enabled": true,
                    "match": { "connectionId": "c-1" },
                    "options": { "interpretation": { "kind": "native" }, "format": "iso", "displayTimeZone": { "kind": "local" } }
                } ]
            }),
            2,
            "en",
        );
        assert!(hook.reconcile());
        assert_eq!(hook.snapshot().revision, 2);
        assert!(hook.snapshot().configs.is_empty(), "unknown version → empty");
        assert!(
            hook.snapshot().rules.is_empty(),
            "the valid rule stays unparsed under the unknown version"
        );
        // The same rule under v1 DOES ride (the gate is the version
        // arm, not a rule-shape accident).
        hook.note_store(
            json!({
                "version": 1,
                "renderers": { "datazen.timestamp": { "enabled": true, "options": {} } },
                "rules": [ {
                    "name": "r1",
                    "enabled": true,
                    "match": { "connectionId": "c-1" },
                    "options": { "interpretation": { "kind": "native" }, "format": "iso", "displayTimeZone": { "kind": "local" } }
                } ]
            }),
            3,
            "en",
        );
        assert!(hook.reconcile());
        assert_eq!(hook.snapshot().rules.len(), 1, "v1 carries the rule");
    }


    #[test]
    fn an_empty_language_falls_back_through_the_resolver() {
        let mut hook = ValuePresentationSnapshotHookState::new();
        hook.note_store(json!({ "version": 1, "renderers": {} }), 1, "");
        assert!(hook.reconcile());
        // The resolver's own fallback (the absent-language arm).
        assert_eq!(hook.snapshot().environment.locale, "en-US");
    }
}
