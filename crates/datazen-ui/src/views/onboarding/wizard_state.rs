//! Port of `packages/frontend/src/windows/onboarding/wizardState.ts`
//! (T323): the onboarding wizard's state machine — a pure reducer plus its
//! derivations, no view.
//!
//! The React file is the contract the wizard shell (`OnboardingWizard.tsx`,
//! T114) and the footer (`WizardFooter.tsx`, T247) sit on: the four-screen
//! journey S0 (welcome / entry choice) → S1 (step 1 of 2, *entry specific* —
//! the inline import form, the connection form or the sample dataset panel)
//! → S2 (step 2 of 2, *always* the AI provider step) → S3 (done); step 1's
//! completeness rules; the step-indicator numbering; and the done screen's
//! connection summary. Q2: skip is reachable at every step except S3. Q5:
//! the persisted onboarding state is versioned (`{ completed, version }`),
//! but that payload is `onboardingGate.ts`'s (T268), not this file's.
//!
//! ## What maps where
//!
//! - `useReducer(wizardReducer, INITIAL_STATE)` → the host owns a
//!   [`WizardState`] and dispatches [`WizardAction`]s through
//!   [`wizard_reducer`]. The outputs the T114 shell feeds on (`state.step`,
//!   `state.entry`, `canContinueFromStepOne(state)`, `state.connectionName`)
//!   are [`WizardState`]'s fields and
//!   [`WizardState::can_continue_from_step_one`]; the window-side wiring
//!   (who owns the state, who persists) stays host-side — this crate keeps
//!   no store/IPC concerns.
//! - The seam types the earlier tasks left behind are absorbed here (the
//!   T021→T287 pattern): [`Step`] and [`Entry`] (React `EntryType`) move
//!   out of `onboarding_wizard.rs`, which re-exports them under its T114
//!   names so every existing import path stays untouched;
//!   [`step_index_for`] absorbs the shell's private copy and
//!   [`step_label_key`] the footer's; [`ImportSummary`] absorbs
//!   `import_step.rs`'s (the producer re-exports it); and the done
//!   screen's [`ConnectionSummary`] / [`imported_count`] /
//!   [`connection_summary`] cores absorb `done_step.rs`'s (the
//!   part-taking signatures stay verbatim for the re-export, the
//!   state-taking React halves become [`WizardState`] methods).
//! - `SAMPLE_READY`'s identity guard (React returns the *same* object so a
//!   StrictMode double-report cannot re-render the shell in a loop) ports
//!   as a value-equality check that returns an equal clone — Rune
//!   re-renders are dirty-marked, so a fresh-but-equal value cannot loop.
//! - The reducer's transition map (the React file's own doc) is reproduced
//!   verbatim in [`wizard_reducer`]'s table below.
//!
//! ## Keys
//!
//! The React file renders nothing; its one derived-key surface is the step
//! indicator's i18n keys, which stay verbatim:
//!
//! | step | [`step_label_key`] |
//! |------|--------------------|
//! | S2 | `onboarding.s2.stepLabel` |
//! | every other step (S1 is the only other reachable one — S0/S3 hide the indicator) | `onboarding.s1.stepLabel` |
//!
//! Known deltas (documented, not silent): JS `undefined` and `null` both
//! port to `None` (the React fields never distinguish them observably);
//! the `overwritten ?? 0` / `?? 0` / `?? ''` fallbacks are no-ops (the
//! interface already required the fields — the ported struct keeps them
//! non-optional); `Boolean(samplePath)` becomes `Some` *and* non-empty
//! (an empty string is falsy in JS); counts are `u32`.

// The state machine's window-side wiring (the host that owns the
// WizardState and dispatches) is a later task; until then the shell's
// feed-ins and this module's tests are the only callers, so the dead-code
// lint is silenced here rather than in a crate-wide allow (the
// onboarding_wizard.rs precedent).
#![allow(dead_code)]

/// React `Step` (`wizardState.ts:15`): the journey's four screens.
///
/// Absorbed from `onboarding_wizard.rs` (T114's seam), which re-exports it
/// under its old path; variant docs carry over verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// S0 — the welcome / entry choice.
    S0,
    /// S1 — step 1 of 2, entry-specific.
    S1,
    /// S2 — step 2 of 2, always the AI provider step.
    S2,
    /// S3 — done.
    S3,
}

/// React `EntryType` (`wizardState.ts:16`): which screen step 1 renders for
/// the entry picked in S0. Kept under the T114 seam name `Entry` (the
/// shell's established name — the absorb re-export preserves it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// The connection form (its own Save is the way forward).
    Manual,
    /// The inline import form.
    Import,
    /// The sample dataset panel.
    Sample,
}

/// React `ImportSummary` (`wizardState.ts:21`): what the import actually
/// wrote — the payload of the wizard's `IMPORT_SUCCESS` dispatch and the
/// done screen's connection row. Absorbed from `import_step.rs` (the
/// producer's side of the seam), which re-exports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportSummary {
    /// Brand-new connections the import created.
    pub(crate) imported: u32,
    /// Existing connections it replaced (same id).
    pub(crate) overwritten: u32,
    /// The human-readable source (the backend's format name, or the
    /// picker's label).
    pub(crate) source: String,
}

/// React `WizardState` (`wizardState.ts:28`): the reducer's state — the
/// whole journey in one value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WizardState {
    /// `step` — the current screen.
    pub(crate) step: Step,
    /// `entry` — which screen step 1 renders for the picked entry.
    pub(crate) entry: Entry,
    /// `importResult` — the inline import's result (import entry only).
    pub(crate) import_result: Option<ImportSummary>,
    /// `samplePath` — the seeded sample database path (sample entry only).
    pub(crate) sample_path: Option<String>,
    /// `connectionName` — the connection created in step 1 (manual /
    /// sample entries).
    pub(crate) connection_name: Option<String>,
    /// `aiLabel` — the provider label configured in step 2, `None` when
    /// AI was skipped.
    pub(crate) ai_label: Option<String>,
    /// `aiConfigured` — step 2 happened (the done summary's gate).
    pub(crate) ai_configured: bool,
}

impl WizardState {
    /// React `INITIAL_STATE` (`wizardState.ts:56`): the journey starts on
    /// the welcome screen with the manual entry preselected (so
    /// `canContinueFromStepOne` is false) and AI unconfigured.
    pub(crate) fn initial() -> Self {
        Self {
            step: Step::S0,
            entry: Entry::Manual,
            import_result: None,
            sample_path: None,
            connection_name: None,
            ai_label: None,
            ai_configured: false,
        }
    }

    /// React `importedCount(state)` (`wizardState.ts:149`): how many
    /// connections the import actually wrote — brand-new ones plus existing
    /// ones it replaced. Re-importing an export of the same machine only
    /// overwrites, and that still counts as a completed step 1.
    pub(crate) fn imported_count(&self) -> u32 {
        imported_count(self.import_result.as_ref())
    }

    /// React `canLeaveStepOne(state)` (`wizardState.ts:133`): step 1 is
    /// complete — the manual form saved (always, the form's own Save is
    /// the gate), the import succeeded (`imported + overwritten > 0`) or
    /// the sample is seeded (`Boolean(samplePath)`).
    pub(crate) fn can_leave_step_one(&self) -> bool {
        match self.entry {
            Entry::Import => self.imported_count() > 0,
            Entry::Sample => self.sample_path.as_deref().map_or(false, |p| !p.is_empty()),
            Entry::Manual => true,
        }
    }

    /// React `canContinueFromStepOne(state)` (`wizardState.ts:161`):
    /// whether the footer's Continue button advances out of step 1. The
    /// manual entry has no Continue: its own Save button is the single way
    /// forward (the connection must be persisted before the AI step).
    pub(crate) fn can_continue_from_step_one(&self) -> bool {
        self.entry != Entry::Manual && self.can_leave_step_one()
    }

    /// React `connectionSummary(state)` (`wizardState.ts:186`): how the
    /// done screen's connection row is filled in.
    pub(crate) fn connection_summary(&self) -> ConnectionSummary {
        connection_summary(self.entry, self.import_result.as_ref(), self.connection_name.as_deref())
    }
}

impl Default for WizardState {
    fn default() -> Self {
        Self::initial()
    }
}

/// React `WizardAction` (`wizardState.ts:43`): the eleven dispatches the
/// shell, the step components and the footer fire at the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WizardAction {
    /// `ENTER_MANUAL` — S0's manual card.
    EnterManual,
    /// `ENTER_IMPORT` — S0's import card.
    EnterImport,
    /// `ENTER_SAMPLE` — S0's sample card.
    EnterSample,
    /// `IMPORT_SUCCESS` — the inline import form's result (no dialog
    /// round-trip; the step stays S1).
    ImportSuccess {
        /// `result`.
        result: ImportSummary,
    },
    /// `SAMPLE_READY` — the sample database seeded.
    SampleReady {
        /// `path`.
        path: String,
        /// `connectionName`.
        connection_name: String,
    },
    /// `SAMPLE_FAILED` — the seeding failed (step 1 stays locked).
    SampleFailed,
    /// `SAVE_SUCCESS` — the connection form's Save (the manual entry's
    /// way forward).
    SaveSuccess {
        /// `connectionName`.
        connection_name: String,
    },
    /// `CONTINUE` — the footer's Continue (step 1 of a non-manual entry,
    /// only when complete).
    Continue,
    /// `FINISH` — step 2's way out; `ai_label: None` finishes with AI
    /// unconfigured (the journey never forces a key).
    Finish {
        /// `aiLabel`.
        ai_label: Option<String>,
    },
    /// `SKIP` — reachable at every step except S3; lands on S3 with
    /// nothing configured.
    Skip,
    /// `BACK` — one step back (S2→S1, S1→S0).
    Back,
}

/// React `wizardReducer` (`wizardState.ts:77`): the pure transition core —
/// the host dispatches, the machine answers with the next state.
///
/// Transition map (the React file's own):
///
/// | state + action | next |
/// |---|---|
/// | S0 + `ENTER_MANUAL` | S1, entry manual (the connection form) |
/// | S0 + `ENTER_IMPORT` | S1, entry import (the inline import form) |
/// | S0 + `ENTER_SAMPLE` | S1, entry sample (the sample dataset panel) |
/// | S1 + `SAVE_SUCCESS` (manual) | S2 |
/// | S1 + `CONTINUE` (import / sample, only when ready) | S2 |
/// | S2 + `FINISH` | S3 (`aiConfigured = aiLabel !== null`) |
/// | any step + `SKIP` | S3, AI unconfigured |
/// | S1 + `BACK` | S0 |
/// | S2 + `BACK` | S1 |
/// | everything else | unchanged |
pub(crate) fn wizard_reducer(state: &WizardState, action: &WizardAction) -> WizardState {
    let mut next = state.clone();
    match action {
        WizardAction::EnterManual => {
            next.step = Step::S1;
            next.entry = Entry::Manual;
        }
        WizardAction::EnterImport => {
            next.step = Step::S1;
            next.entry = Entry::Import;
        }
        WizardAction::EnterSample => {
            next.step = Step::S1;
            next.entry = Entry::Sample;
        }
        WizardAction::ImportSuccess { result } => {
            // Note: no step change — the inline form reports in place.
            next.entry = Entry::Import;
            next.import_result = Some(result.clone());
        }
        WizardAction::SampleReady { path, connection_name } => {
            // Idempotent: the sample step may report the same result more
            // than once (StrictMode remount). React returns the SAME
            // object so the shell cannot re-render in a loop; the port
            // returns an equal clone (the documented delta — value
            // equality, not identity).
            if state.entry == Entry::Sample
                && state.sample_path.as_deref() == Some(path.as_str())
                && state.connection_name.as_deref() == Some(connection_name.as_str())
            {
                return next;
            }
            next.entry = Entry::Sample;
            next.sample_path = Some(path.clone());
            next.connection_name = Some(connection_name.clone());
        }
        WizardAction::SampleFailed => {
            // Clears the seeded path (locking step 1 again) but keeps the
            // connection name — React does the same.
            next.entry = Entry::Sample;
            next.sample_path = None;
        }
        WizardAction::SaveSuccess { connection_name } => {
            // The connection form's Save button is the manual entry's way
            // forward.
            next.connection_name = Some(connection_name.clone());
            next.step = Step::S2;
        }
        WizardAction::Continue => {
            // Only from S1, only when step 1 is complete for a non-manual
            // entry — evaluated on the incoming state, like React's.
            if state.step != Step::S1 || !state.can_continue_from_step_one() {
                return next;
            }
            next.step = Step::S2;
        }
        WizardAction::Finish { ai_label } => {
            next.step = Step::S3;
            next.ai_label = ai_label.clone();
            next.ai_configured = ai_label.is_some();
        }
        WizardAction::Skip => {
            next.step = Step::S3;
            next.ai_label = None;
            next.ai_configured = false;
        }
        WizardAction::Back => {
            if state.step == Step::S2 {
                next.step = Step::S1;
            } else if state.step == Step::S1 {
                next.step = Step::S0;
            }
        }
    }
    next
}

/// React `stepIndexFor` (`wizardState.ts:169`): the step dots exist only
/// for step 1 and step 2 — the indicator is hidden (0) on the entry screen
/// (S0) and the done screen (S3). Absorbed from the T114 shell (its
/// `visible_step_number` helper reduces on this).
pub(crate) fn step_index_for(step: Step) -> u8 {
    match step {
        Step::S1 => 1,
        Step::S2 => 2,
        Step::S0 | Step::S3 => 0,
    }
}

/// React `stepLabelKey` (`wizardState.ts:176`): the step indicator label's
/// i18n key — S2 reads its own label, every other step step 1's (the label
/// only renders while the indicator is visible, i.e. on S1/S2). Absorbed
/// from the footer's private copy (T247).
pub(crate) fn step_label_key(step: Step) -> &'static str {
    if step == Step::S2 {
        "onboarding.s2.stepLabel"
    } else {
        "onboarding.s1.stepLabel"
    }
}

// ---- the done screen's summary cores (absorbed from done_step.rs) -----------
// The part-taking signatures are T177's verbatim — done_step re-exports
// them, and the state-taking React halves live on WizardState above.

/// React `ConnectionSummary` (`wizardState.ts:181`): how the done screen's
/// connection row is filled in. Absorbed from `done_step.rs` (T177's
/// seam), which re-exports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionSummary {
    /// The import entry wrote connections: the counts and the source.
    Imported {
        /// `imported` — brand-new connections.
        imported: u32,
        /// `overwritten` — existing ones replaced.
        overwritten: u32,
        /// `source` — the import's human-readable source.
        source: String,
    },
    /// A connection was created (the manual / sample entries).
    Connection {
        /// `name` — the created connection's name.
        name: String,
    },
    /// Nothing was configured (the skip path).
    None,
}

/// React `importedCount` (`wizardState.ts:149`), the part-taking half the
/// done screen's port established: brand-new plus replaced connections.
/// (`overwritten ?? 0` is a no-op — the ported field is non-optional.)
pub(crate) fn imported_count(import_result: Option<&ImportSummary>) -> u32 {
    import_result.map_or(0, |r| r.imported + r.overwritten)
}

/// React `connectionSummary` (`wizardState.ts:186`), the part-taking half
/// the done screen's port established: the import entry with a non-zero
/// written count reports its counts and source; else a created connection
/// reports its name; else nothing was configured.
pub(crate) fn connection_summary(
    entry: Entry,
    import_result: Option<&ImportSummary>,
    connection_name: Option<&str>,
) -> ConnectionSummary {
    if entry == Entry::Import && imported_count(import_result) > 0 {
        // The count gate implies the summary exists; the map_or arms are
        // React's own `?? 0` / `?? ''` fallbacks, unreachable here.
        return ConnectionSummary::Imported {
            imported: import_result.map_or(0, |r| r.imported),
            overwritten: import_result.map_or(0, |r| r.overwritten),
            source: import_result.map_or_else(String::new, |r| r.source.clone()),
        };
    }
    if let Some(name) = connection_name {
        return ConnectionSummary::Connection { name: name.to_owned() };
    }
    ConnectionSummary::None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// React's `atStepOne` helper (`__tests__/wizardState.test.ts:13`):
    /// INITIAL_STATE at S1 with the entry set.
    fn at_step_one(entry: Entry) -> WizardState {
        WizardState { step: Step::S1, entry, ..WizardState::initial() }
    }

    /// A one-line `ImportSummary`.
    fn summary(imported: u32, overwritten: u32, source: &str) -> ImportSummary {
        ImportSummary { imported, overwritten, source: source.to_owned() }
    }

    // --- wizardReducer — entry selection (wizardState.test.ts:17) ------------

    #[test]
    fn s0_to_s1_manual_via_enter_manual() {
        let next = wizard_reducer(&WizardState::initial(), &WizardAction::EnterManual);
        assert_eq!(next.step, Step::S1);
        assert_eq!(next.entry, Entry::Manual);
    }

    #[test]
    fn s0_to_s1_import_via_enter_import() {
        // The import form is a step, not a dialog.
        let next = wizard_reducer(&WizardState::initial(), &WizardAction::EnterImport);
        assert_eq!(next.step, Step::S1);
        assert_eq!(next.entry, Entry::Import);
    }

    #[test]
    fn s0_to_s1_sample_via_enter_sample() {
        let next = wizard_reducer(&WizardState::initial(), &WizardAction::EnterSample);
        assert_eq!(next.step, Step::S1);
        assert_eq!(next.entry, Entry::Sample);
    }

    // --- step 2 is always the AI step (wizardState.test.ts:37) ----------------

    #[test]
    fn every_entry_reaches_s2_before_s3_and_finish_configures_ai() {
        for entry in [Entry::Manual, Entry::Import, Entry::Sample] {
            let mut state = at_step_one(entry);
            state = match entry {
                Entry::Manual => wizard_reducer(
                    &state,
                    &WizardAction::SaveSuccess { connection_name: "Prod".to_owned() },
                ),
                Entry::Import => wizard_reducer(
                    &state,
                    &WizardAction::ImportSuccess { result: summary(3, 0, "DBeaver") },
                ),
                Entry::Sample => wizard_reducer(
                    &state,
                    &WizardAction::SampleReady {
                        path: "/tmp/playground.db".to_owned(),
                        connection_name: "Sample Playground".to_owned(),
                    },
                ),
            };
            if entry != Entry::Manual {
                // The import/sample results report in place — CONTINUE is
                // the step 1 → 2 edge.
                assert_eq!(state.step, Step::S1);
                state = wizard_reducer(&state, &WizardAction::Continue);
            }
            assert_eq!(state.step, Step::S2);

            state = wizard_reducer(
                &state,
                &WizardAction::Finish { ai_label: Some("OpenAI".to_owned()) },
            );
            assert_eq!(state.step, Step::S3);
            assert!(state.ai_configured);
        }
    }

    #[test]
    fn manual_entry_advances_to_s2_only_through_save_success() {
        let state = at_step_one(Entry::Manual);
        assert!(!state.can_continue_from_step_one());
        assert_eq!(wizard_reducer(&state, &WizardAction::Continue).step, Step::S1);
        assert_eq!(
            wizard_reducer(&state, &WizardAction::SaveSuccess { connection_name: "Prod".to_owned() })
                .step,
            Step::S2
        );
    }

    // --- canLeaveStepOne (wizardState.test.ts:76) -----------------------------

    #[test]
    fn manual_is_complete_once_the_connection_is_saved() {
        let state = WizardState { entry: Entry::Manual, ..WizardState::initial() };
        assert!(state.can_leave_step_one());
    }

    #[test]
    fn import_needs_at_least_one_written_connection() {
        assert!(!at_step_one(Entry::Import).can_leave_step_one());
        let empty =
            WizardState { import_result: Some(summary(0, 0, "DBeaver")), ..at_step_one(Entry::Import) };
        assert!(!empty.can_leave_step_one());
        let fresh =
            WizardState { import_result: Some(summary(2, 0, "DBeaver")), ..at_step_one(Entry::Import) };
        assert!(fresh.can_leave_step_one());
        // Re-importing an export of this very machine only overwrites —
        // still done.
        let reimport =
            WizardState { import_result: Some(summary(0, 3, "DataZen")), ..at_step_one(Entry::Import) };
        assert!(reimport.can_leave_step_one());
    }

    #[test]
    fn sample_needs_a_seeded_database() {
        assert!(!at_step_one(Entry::Sample).can_leave_step_one());
        let seeded = WizardState {
            sample_path: Some("/tmp/playground.db".to_owned()),
            ..at_step_one(Entry::Sample)
        };
        assert!(seeded.can_leave_step_one());
    }

    #[test]
    fn continue_is_a_no_op_while_step_one_is_incomplete() {
        let state = at_step_one(Entry::Import);
        assert_eq!(wizard_reducer(&state, &WizardAction::Continue).step, Step::S1);
    }

    // --- navigation and skip (wizardState.test.ts:117) -------------------------

    #[test]
    fn back_from_s1_lands_on_s0_and_back_from_s2_on_s1_for_any_entry() {
        assert_eq!(wizard_reducer(&at_step_one(Entry::Manual), &WizardAction::Back).step, Step::S0);
        for entry in [Entry::Manual, Entry::Import, Entry::Sample] {
            let s2 = WizardState { step: Step::S2, entry, ..WizardState::initial() };
            assert_eq!(wizard_reducer(&s2, &WizardAction::Back).step, Step::S1);
        }
    }

    #[test]
    fn skip_from_any_step_lands_on_s3_with_ai_unconfigured() {
        for step in [Step::S0, Step::S1, Step::S2] {
            let next =
                wizard_reducer(&WizardState { step, ..WizardState::initial() }, &WizardAction::Skip);
            assert_eq!(next.step, Step::S3);
            assert!(!next.ai_configured);
            assert_eq!(next.ai_label, None);
        }
    }

    #[test]
    fn back_is_a_no_op_on_s0_and_s3() {
        assert_eq!(wizard_reducer(&WizardState::initial(), &WizardAction::Back).step, Step::S0);
        let s3 = WizardState { step: Step::S3, entry: Entry::Sample, ..WizardState::initial() };
        assert_eq!(wizard_reducer(&s3, &WizardAction::Back).step, Step::S3);
    }

    // --- the step indicator (wizardState.test.ts:143) ---------------------------

    #[test]
    fn numbers_step_1_and_2_and_hides_the_indicator_elsewhere() {
        assert_eq!(step_index_for(Step::S0), 0);
        assert_eq!(step_index_for(Step::S1), 1);
        assert_eq!(step_index_for(Step::S2), 2);
        assert_eq!(step_index_for(Step::S3), 0);
        assert_eq!(step_label_key(Step::S1), "onboarding.s1.stepLabel");
        assert_eq!(step_label_key(Step::S2), "onboarding.s2.stepLabel");
    }

    // --- connectionSummary (wizardState.test.ts:154) ------------------------------

    #[test]
    fn connection_summary_reports_imports_connections_and_nothing() {
        let imported = WizardState {
            entry: Entry::Import,
            import_result: Some(summary(4, 1, "DBeaver JSON")),
            ..WizardState::initial()
        };
        assert_eq!(
            imported.connection_summary(),
            ConnectionSummary::Imported { imported: 4, overwritten: 1, source: "DBeaver JSON".to_owned() }
        );
        let manual = WizardState {
            entry: Entry::Manual,
            connection_name: Some("Prod".to_owned()),
            ..WizardState::initial()
        };
        assert_eq!(manual.connection_summary(), ConnectionSummary::Connection { name: "Prod".to_owned() });
        let bare = WizardState { entry: Entry::Sample, ..WizardState::initial() };
        assert_eq!(bare.connection_summary(), ConnectionSummary::None);
    }

    // --- the journeys (wizardState.test.ts:170, 200) --------------------------------

    #[test]
    fn import_entry_runs_the_whole_wizard() {
        let mut state = WizardState::initial();

        state = wizard_reducer(&state, &WizardAction::EnterImport);
        assert_eq!(state.step, Step::S1);
        assert!(!state.can_leave_step_one());

        // The inline form reports the import result without any dialog
        // round-trip.
        state = wizard_reducer(
            &state,
            &WizardAction::ImportSuccess { result: summary(6, 0, "TablePlus") },
        );
        assert_eq!(state.step, Step::S1);
        assert!(state.can_leave_step_one());

        state = wizard_reducer(&state, &WizardAction::Continue);
        assert_eq!(state.step, Step::S2);

        state = wizard_reducer(&state, &WizardAction::Finish { ai_label: Some("DeepSeek".to_owned()) });
        assert_eq!(state.step, Step::S3);
        assert_eq!(
            state.connection_summary(),
            ConnectionSummary::Imported { imported: 6, overwritten: 0, source: "TablePlus".to_owned() }
        );
    }

    #[test]
    fn sample_entry_seeds_then_still_configures_ai() {
        let mut state = WizardState::initial();

        state = wizard_reducer(&state, &WizardAction::EnterSample);
        assert_eq!(state.step, Step::S1);
        // A failed seeding keeps step 1 locked instead of advancing.
        state = wizard_reducer(&state, &WizardAction::SampleFailed);
        assert!(!state.can_leave_step_one());

        state = wizard_reducer(
            &state,
            &WizardAction::SampleReady {
                path: "/data/sample/playground.db".to_owned(),
                connection_name: "Sample Playground".to_owned(),
            },
        );
        assert!(state.can_leave_step_one());

        state = wizard_reducer(&state, &WizardAction::Continue);
        assert_eq!(state.step, Step::S2);

        state = wizard_reducer(&state, &WizardAction::Skip);
        assert_eq!(state.step, Step::S3);
        assert_eq!(
            state.connection_summary(),
            ConnectionSummary::Connection { name: "Sample Playground".to_owned() }
        );
    }

    // --- beyond the React suite: the reducer's own documented edges ---------------

    #[test]
    fn sample_ready_is_idempotent_for_the_same_payload() {
        let ready = WizardAction::SampleReady {
            path: "/data/sample/playground.db".to_owned(),
            connection_name: "Sample Playground".to_owned(),
        };
        let mut state = wizard_reducer(
            &wizard_reducer(&WizardState::initial(), &WizardAction::EnterSample),
            &ready,
        );
        let reported_twice = wizard_reducer(&state, &ready);
        assert_eq!(reported_twice, state, "a repeat report changes nothing");
        state = wizard_reducer(
            &state,
            &WizardAction::SampleReady {
                path: "/data/sample/other.db".to_owned(),
                connection_name: "Sample Playground".to_owned(),
            },
        );
        assert_eq!(state.sample_path.as_deref(), Some("/data/sample/other.db"));
    }

    #[test]
    fn sample_failed_clears_the_seeded_path_but_keeps_the_connection_name() {
        let mut state = wizard_reducer(
            &wizard_reducer(&WizardState::initial(), &WizardAction::EnterSample),
            &WizardAction::SampleReady {
                path: "/data/sample/playground.db".to_owned(),
                connection_name: "Sample Playground".to_owned(),
            },
        );
        assert!(state.can_leave_step_one());
        state = wizard_reducer(&state, &WizardAction::SampleFailed);
        assert_eq!(state.sample_path, None, "the path is cleared — step 1 locks again");
        assert!(!state.can_leave_step_one());
        assert_eq!(state.connection_name.as_deref(), Some("Sample Playground"));
    }

    #[test]
    fn finish_without_a_label_leaves_ai_unconfigured() {
        // The journey never forces a key.
        let state = wizard_reducer(
            &WizardState { step: Step::S2, ..WizardState::initial() },
            &WizardAction::Finish { ai_label: None },
        );
        assert_eq!(state.step, Step::S3);
        assert_eq!(state.ai_label, None);
        assert!(!state.ai_configured);
    }

    #[test]
    fn the_initial_state_is_the_default_and_starts_locked() {
        assert_eq!(WizardState::initial(), WizardState::default());
        let initial = WizardState::initial();
        assert_eq!(initial.step, Step::S0);
        assert_eq!(initial.entry, Entry::Manual);
        assert_eq!(initial.imported_count(), 0);
        assert!(!initial.can_continue_from_step_one());
        assert_eq!(initial.connection_summary(), ConnectionSummary::None);
    }
}
