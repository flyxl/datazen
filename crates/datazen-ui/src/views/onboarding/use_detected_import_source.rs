//! Port of
//! `packages/frontend/src/windows/onboarding/useDetectedImportSource.ts`
//! (T306): the detection pre-check behind the onboarding import entry —
//! which of the five supported clients (DBeaver, DataGrip, Navicat,
//! TablePlus, DBX) has a config file on this machine.
//!
//! The React hook fires one `detect_connection_import_path` invoke per
//! client in `IMPORT_APP_ORDER`, keeps every fulfilled-and-`found` one in
//! `detectedApps` (in that order) and offers the first as `detected` —
//! the default import source the wizard's adopt-the-first-detected effect
//! switches to (Q6: detection over known config paths only). Failures
//! degrade to "nothing detected": the entry card shows the generic copy
//! and the user can still pick a file. `enabled` (the call site's gate —
//! `false` skips detection entirely) is the effect's only dependency.
//!
//! ## What maps where
//!
//! - `IMPORT_APP_ORDER` stays the exported const [`IMPORT_APP_ORDER`] —
//!   the pre-check order (which is NOT the `ConnectionImportApp` union's
//!   declaration order; React's own two tables disagree, and the hook's
//!   is this one).
//! - The two `useState` cells (`detectedApps: []`, `loading: enabled`)
//!   are [`DetectedImportSource`]'s fields; the returned interface
//!   `{ detected, detectedApps, loading }` is the struct plus its
//!   getters, with `detected` deriving `detectedApps[0] ?? null`.
//! - The `useEffect` on `[enabled]` is
//!   [`DetectedImportSource::resync_detection`]: the host calls it on
//!   mount and whenever `enabled` changes (the dep array — a same-value
//!   re-call must not happen, exactly as the effect does not re-fire).
//!   The disabled branch (`setDetectedApps([]); setLoading(false)`) runs
//!   here; the enabled branch only re-arms `loading` — it does NOT clear
//!   the previous hits, as React's body does not either. The probes
//!   themselves are the host's IO seam (no IPC in the UI crate): the host
//!   runs `detect_connection_import_path(app.source_id())` for each
//!   [`IMPORT_APP_ORDER`] entry and reports back.
//! - `Promise.allSettled(...)` + the hits filter + the `finally` are
//!   [`DetectedImportSource::report_probes`]: one [`ImportProbe`] per
//!   order entry (index-aligned, like `results[i]`), the hits land in
//!   [`IMPORT_APP_ORDER`] order and `loading` ends. A report tagged with
//!   a round the effect has since invalidated is dropped whole — React's
//!   `cancelled` guard, ported as the round counter
//!   [`DetectedImportSource::resync_detection`] hands the host.
//! - `ConnectionImportApp` (`ConnectionShareDialog.tsx:13`) is absorbed
//!   here as [`ImportApp`] — the import-track absorber the wizard's seam
//!   note names (T177 kept the wizard's; T306 takes it over);
//!   `onboarding_wizard.rs` re-exports it under its T114 name so every
//!   existing import path stays untouched (the T323 `Step`/`Entry`
//!   precedent). [`ImportApp::source_id`] carries the union's literals —
//!   the `source` argument of the host's probe invokes, and the one truth
//!   `import_step.rs`'s picker ids delegate to.
//! - The consumers this port aligns with: the host feeds
//!   [`DetectedImportSource::detected`] into the wizard's
//!   `resync_detected` (T114) and
//!   [`DetectedImportSource::detected_apps`] into
//!   `ImportStep::detected_apps` (T177's checkmark feed).
//!
//! ## Known deltas (documented, not silent)
//!
//! - React's default argument `enabled = true` is [`Default`]
//!   (= `new(true)`).
//! - The report is one batch for the whole round — `allSettled` waits for
//!   all five settles before the hook's `then` runs, so per-app streaming
//!   completion has no React behavior to preserve.
//! - A report shorter than [`IMPORT_APP_ORDER`] treats its missing
//!   entries as not-found (`results[i]?.`'s optional chain) — the host
//!   always passes all five; the guard is the faithful edge.
//!
//! Keys: this is a logic port; the entry cards that consume it
//! (`steps/welcome_step.rs`, `steps/import_step.rs`) own the element keys.

// The onboarding window host (T072's wizard track — the probe round and
// the wizard/import-step feeds wire there) is the production consumer;
// until then this crate's tests are the only caller, so the dead-code
// lint is silenced here rather than crate-wide (the onboarding_wizard.rs
// precedent).
#![allow(dead_code)]

/// React `ConnectionImportApp` (`ConnectionShareDialog.tsx:13`): the five
/// clients whose local config the import entry can pre-detect.
///
/// Absorbed from `onboarding_wizard.rs` (T114's seam — the import track's
/// T306 absorber, per the type's own note), which re-exports it under its
/// T114 name so the steps'/audits' import paths stay untouched (the T323
/// `Step`/`Entry` precedent); variant order is the React union's,
/// verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportApp {
    Dbx,
    Navicat,
    Datagrip,
    Dbeaver,
    Tableplus,
}

impl ImportApp {
    /// The React union's string literal (`'dbx'` …) —
    /// `detectConnectionImportPath(app)`'s `source` argument, verbatim.
    pub(crate) fn source_id(self) -> &'static str {
        match self {
            ImportApp::Dbx => "dbx",
            ImportApp::Navicat => "navicat",
            ImportApp::Datagrip => "datagrip",
            ImportApp::Dbeaver => "dbeaver",
            ImportApp::Tableplus => "tableplus",
        }
    }
}

/// React `IMPORT_APP_ORDER` (`useDetectedImportSource.ts:10`): the
/// pre-check order for the "import connections" entry card. The first
/// client whose config is found on this machine is offered as the default
/// import source (Q6: detection over known config paths only).
pub(crate) const IMPORT_APP_ORDER: [ImportApp; 5] = [
    ImportApp::Dbeaver,
    ImportApp::Datagrip,
    ImportApp::Navicat,
    ImportApp::Tableplus,
    ImportApp::Dbx,
];

/// One probe's outcome as the hook reads it — the host's report entry for
/// [`DetectedImportSource::report_probes`], mirroring the
/// `Promise.allSettled` result the hook filters
/// (`results[i]?.status === 'fulfilled' && results[i].value?.found === true`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportProbe {
    /// `status === 'rejected'` — or the invoke never settled: not a hit
    /// (failures degrade to "nothing detected").
    Rejected,
    /// `status === 'fulfilled'`: the `detect_connection_import_path`
    /// payload's `found` flag — only `true` is a hit.
    Fulfilled { found: bool },
}

impl ImportProbe {
    /// The hook's per-entry predicate: fulfilled AND `found === true`.
    fn is_hit(self) -> bool {
        matches!(self, ImportProbe::Fulfilled { found: true })
    }
}

/// React `useDetectedImportSource(enabled)` — the hook's state cells plus
/// its effect bookkeeping as one host-owned value (the
/// `useModelProfileDraft` convention: no IPC in the UI crate; the host
/// runs the probes and reports back). The returned interface
/// `{ detected, detectedApps, loading }` is this struct and its getters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetectedImportSource {
    /// `detectedApps` — every client whose config was found, in
    /// [`IMPORT_APP_ORDER`] order (`useState([])`'s empty start).
    detected_apps: Vec<ImportApp>,
    /// `loading` (`useState(enabled)`'s start; every enabled round re-arms
    /// it, the round's report ends it).
    loading: bool,
    /// The effect cleanup's `cancelled` flag as a round counter: every
    /// [`DetectedImportSource::resync_detection`] invalidates the round
    /// before it, so a report tagged with a stale round is dropped whole —
    /// neither its hits nor the `finally`'s `setLoading(false)` land.
    round: u32,
}

impl DetectedImportSource {
    /// The hook's mount: `useState([])` + `useState(enabled)` — `loading`
    /// starts as the `enabled` argument (React's default argument is
    /// `true`; see [`Default`]).
    pub(crate) fn new(enabled: bool) -> Self {
        Self { detected_apps: Vec::new(), loading: enabled, round: 0 }
    }

    /// React's `useEffect` on `[enabled]` — the host calls this on mount
    /// and whenever `enabled` changes (the hook's only dependency; the
    /// dep array means a same-value re-call must not happen). The
    /// synchronous half lives here: the disabled branch clears
    /// `detectedApps` and ends `loading`; the enabled branch re-arms
    /// `loading` alone (it does NOT clear the previous hits — React's
    /// body touches only `loading`). The return is the round the host
    /// must tag its probe report with; when enabled, the host then runs
    /// the five `detect_connection_import_path` probes (the IO seam —
    /// [`IMPORT_APP_ORDER`] × [`ImportApp::source_id`], in order) and
    /// reports back through [`report_probes`].
    pub(crate) fn resync_detection(&mut self, enabled: bool) -> u32 {
        self.round += 1; // the cleanup: the previous round is cancelled
        if enabled {
            self.loading = true;
        } else {
            self.detected_apps.clear();
            self.loading = false;
        }
        self.round
    }

    /// The probe round's report — `Promise.allSettled(...)`'s `then` plus
    /// its `finally`: one outcome per [`IMPORT_APP_ORDER`] entry
    /// (index-aligned, like the hook's `results[i]`), the hits land in
    /// [`IMPORT_APP_ORDER`] order, and `loading` ends. A report tagged
    /// with a stale round (the effect re-ran after the probes were kicked
    /// off) is dropped whole — React's `cancelled` guard.
    pub(crate) fn report_probes(&mut self, round: u32, results: &[ImportProbe]) {
        if round != self.round {
            return;
        }
        self.detected_apps = probe_hits(results);
        self.loading = false;
    }

    /// `detected` — `detectedApps[0] ?? null`: the first client in
    /// [`IMPORT_APP_ORDER`] whose config was found, `None` when nothing
    /// was (or nothing has been reported yet). The host feeds this into
    /// the wizard's `resync_detected` (T114's adopt-the-first-detected
    /// effect).
    pub(crate) fn detected(&self) -> Option<ImportApp> {
        self.detected_apps.first().copied()
    }

    /// `detectedApps` — every client whose config was found, in
    /// [`IMPORT_APP_ORDER`] order. The host feeds this into
    /// `ImportStep::detected_apps` (T177's checkmark feed).
    pub(crate) fn detected_apps(&self) -> &[ImportApp] {
        &self.detected_apps
    }

    /// `loading`.
    pub(crate) fn loading(&self) -> bool {
        self.loading
    }
}

impl Default for DetectedImportSource {
    /// React's default argument — `useDetectedImportSource(enabled =
    /// true)`: the hook called with no argument mounts loading.
    fn default() -> Self {
        Self::new(true)
    }
}

/// The hook's hits filter (`IMPORT_APP_ORDER.filter((_, i) =>
/// results[i]?.status === 'fulfilled' && results[i].value?.found ===
/// true)`): the fulfilled-and-`found` entries, in [`IMPORT_APP_ORDER`]
/// order — a rejected, not-`found` or missing entry is not a hit.
fn probe_hits(results: &[ImportProbe]) -> Vec<ImportApp> {
    IMPORT_APP_ORDER
        .iter()
        .enumerate()
        .filter(|&(i, _)| results.get(i).is_some_and(|probe| probe.is_hit()))
        .map(|(_, app)| *app)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use crate::views::onboarding::onboarding_wizard::{ImportSource, OnboardingWizard};

    // --- the tables (useDetectedImportSource.ts:10, ConnectionShareDialog.tsx:13) --

    #[test]
    fn import_app_order_is_the_pre_check_order() {
        // The pre-check order is NOT the type's union order — React's own
        // two tables disagree, and the hook's is this one.
        assert_eq!(
            IMPORT_APP_ORDER,
            [
                ImportApp::Dbeaver,
                ImportApp::Datagrip,
                ImportApp::Navicat,
                ImportApp::Tableplus,
                ImportApp::Dbx,
            ]
        );
    }

    #[test]
    fn source_ids_are_the_react_literals() {
        // detectConnectionImportPath(app)'s `source` argument, verbatim.
        assert_eq!(ImportApp::Dbx.source_id(), "dbx");
        assert_eq!(ImportApp::Navicat.source_id(), "navicat");
        assert_eq!(ImportApp::Datagrip.source_id(), "datagrip");
        assert_eq!(ImportApp::Dbeaver.source_id(), "dbeaver");
        assert_eq!(ImportApp::Tableplus.source_id(), "tableplus");
    }

    // --- the mount (useState([]) + useState(enabled)) --------------------------

    #[test]
    fn mounts_empty_and_loading_when_enabled() {
        let hook = DetectedImportSource::new(true);
        assert_eq!(hook.detected_apps(), &[]);
        assert_eq!(hook.detected(), None);
        assert!(hook.loading());
        // React's default argument: `useDetectedImportSource()` mounts the
        // same way as `useDetectedImportSource(true)`.
        assert_eq!(DetectedImportSource::default(), DetectedImportSource::new(true));
    }

    #[test]
    fn mounts_settled_when_disabled() {
        let hook = DetectedImportSource::new(false);
        assert_eq!(hook.detected_apps(), &[]);
        assert_eq!(hook.detected(), None);
        assert!(!hook.loading());
    }

    // --- the hits filter (the allSettled `then`) --------------------------------

    #[test]
    fn only_fulfilled_and_found_probes_are_hits_in_order() {
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(
            round,
            &[
                ImportProbe::Fulfilled { found: true },  // dbeaver — a hit
                ImportProbe::Rejected,                   // datagrip — rejected
                ImportProbe::Fulfilled { found: false }, // navicat — found must be exactly true
                ImportProbe::Fulfilled { found: true },  // tableplus — a hit
                ImportProbe::Rejected,                   // dbx — rejected
            ],
        );
        assert_eq!(hook.detected_apps(), &[ImportApp::Dbeaver, ImportApp::Tableplus]);
        assert_eq!(hook.detected(), Some(ImportApp::Dbeaver), "the first in ORDER wins");
        assert!(!hook.loading(), "the finally ends loading");
    }

    #[test]
    fn every_probe_failing_degrades_to_nothing_detected() {
        // All rejected (or found=false): the entry card shows the generic
        // copy and the user can still pick a file — detection failure is
        // never an error state.
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(round, &[ImportProbe::Rejected; 5]);
        assert_eq!(hook.detected_apps(), &[]);
        assert_eq!(hook.detected(), None);
        assert!(!hook.loading());
    }

    #[test]
    fn a_missing_report_entry_is_not_a_hit() {
        // `results[i]?.` — the optional chain: a short report's missing
        // entries can never be hits.
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(round, &[ImportProbe::Fulfilled { found: true }]);
        assert_eq!(hook.detected_apps(), &[ImportApp::Dbeaver]);
        assert_eq!(hook.detected(), Some(ImportApp::Dbeaver));
    }

    // --- the enabled effect (useEffect, [enabled]) ------------------------------

    #[test]
    fn disabling_clears_the_hits_and_ends_loading() {
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(round, &[ImportProbe::Fulfilled { found: true }; 5]);
        assert_eq!(hook.detected_apps().len(), 5);
        hook.resync_detection(false);
        assert_eq!(hook.detected_apps(), &[], "setDetectedApps([])");
        assert_eq!(hook.detected(), None);
        assert!(!hook.loading(), "setLoading(false)");
    }

    #[test]
    fn an_enabled_round_re_arms_loading_without_clearing_reported_hits() {
        // React's enabled branch only does `setLoading(true)` — the
        // previous hits stay visible until the new round reports. (The
        // dep array means the host double-fires this only by violating
        // the effect's contract; the body's rule is still exactly this.)
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(round, &[ImportProbe::Fulfilled { found: true }; 5]);
        assert!(!hook.loading());
        hook.resync_detection(true);
        assert!(hook.loading(), "setLoading(true)");
        assert_eq!(hook.detected_apps().len(), 5, "the enabled branch does not clear");
        assert_eq!(hook.detected(), Some(ImportApp::Dbeaver));
    }

    // --- the cancellation guard (the effect cleanup's `cancelled`) ----------------

    #[test]
    fn a_stale_round_report_is_dropped_whole() {
        // Results that land after the effect re-ran change nothing —
        // neither the hits nor the finally's loading flip.
        let mut hook = DetectedImportSource::new(true);
        let first = hook.resync_detection(true);
        hook.resync_detection(false); // disable mid-flight: `cancelled = true`
        hook.report_probes(first, &[ImportProbe::Fulfilled { found: true }; 5]);
        assert_eq!(hook.detected_apps(), &[], "the stale hits do not land");
        assert!(!hook.loading(), "the stale finally does not land");

        // The fresh round still reports normally afterwards.
        let second = hook.resync_detection(true);
        assert!(hook.loading());
        hook.report_probes(
            second,
            &[
                ImportProbe::Rejected,
                ImportProbe::Rejected,
                ImportProbe::Fulfilled { found: true }, // navicat
                ImportProbe::Rejected,
                ImportProbe::Rejected,
            ],
        );
        assert_eq!(hook.detected(), Some(ImportApp::Navicat));
        assert!(!hook.loading());
    }

    #[test]
    fn a_stale_report_after_re_enabling_keeps_the_new_round_pending() {
        let mut hook = DetectedImportSource::new(true);
        let first = hook.resync_detection(true);
        let second = hook.resync_detection(true); // re-enable before reporting
        hook.report_probes(first, &[ImportProbe::Fulfilled { found: true }; 5]);
        assert_eq!(hook.detected_apps(), &[], "the stale round's hits do not land");
        assert!(hook.loading(), "the new round is still pending");
        hook.report_probes(second, &[ImportProbe::Fulfilled { found: false }; 5]);
        assert_eq!(hook.detected_apps(), &[]);
        assert!(!hook.loading());
    }

    // --- the consumer seams (T114 / T177) ----------------------------------------

    #[test]
    fn detected_feeds_the_wizard_resync_seam() {
        // The host hands `detected()` to `OnboardingWizard::resync_detected`:
        // a detected client switches the import source; `None` — nothing
        // detected yet, or detection failed — never resets it.
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        let mut wizard = OnboardingWizard::<()>::new();
        assert_eq!(wizard.import_source(), ImportSource::File, "useState('file')");
        wizard.resync_detected(hook.detected()); // still loading, nothing reported
        assert_eq!(wizard.import_source(), ImportSource::File);
        hook.report_probes(
            round,
            &[
                ImportProbe::Fulfilled { found: true },  // dbeaver
                ImportProbe::Rejected,
                ImportProbe::Rejected,
                ImportProbe::Fulfilled { found: true },  // tableplus
                ImportProbe::Rejected,
            ],
        );
        wizard.resync_detected(hook.detected());
        assert_eq!(
            wizard.import_source(),
            ImportSource::App(ImportApp::Dbeaver),
            "the first detected client wins"
        );
    }

    #[test]
    fn detected_apps_feed_the_import_step_checkmark_list() {
        // T177's feed takes the found list as `impl Into<Vec<ImportApp>>` —
        // the same [`ImportApp`] seam type, in [`IMPORT_APP_ORDER`] order.
        let mut hook = DetectedImportSource::new(true);
        let round = hook.resync_detection(true);
        hook.report_probes(
            round,
            &[
                ImportProbe::Fulfilled { found: true },  // dbeaver
                ImportProbe::Rejected,
                ImportProbe::Fulfilled { found: true },  // navicat
                ImportProbe::Rejected,
                ImportProbe::Fulfilled { found: true },  // dbx
            ],
        );
        let fed: Vec<ImportApp> = hook.detected_apps().to_vec();
        assert_eq!(fed, [ImportApp::Dbeaver, ImportApp::Navicat, ImportApp::Dbx]);
        assert_eq!(hook.detected(), Some(ImportApp::Dbeaver));
    }
}
