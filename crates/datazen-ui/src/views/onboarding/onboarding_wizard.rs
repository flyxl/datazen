//! Port of `packages/frontend/src/windows/onboarding/OnboardingWizard.tsx`
//! (T114): the first-run journey's step host.
//!
//! React's wizard is the shell around a four-step journey — S0 entry choice
//! → S1 step 1 of 2 (entry-specific: the inline import form, the connection
//! form or the sample dataset) → S2 step 2 of 2 (the AI provider, always) →
//! S3 done — plus the global footer (step dots + skip/back/continue/finish)
//! and the two completion handlers: `handleFinish` (step 2's way out) and
//! `handleOpen` (the done step's way out, which stashes the sample-database
//! open directive and persists the completed onboarding settings before
//! signalling the window).
//!
//! ## What maps where
//!
//! - **The root row is not rebuilt here.** React's wizard root
//!   (`flex h-screen bg-surface text-fg`, the `onboarding-wizard` testid)
//!   and the `<main>` column are already hoisted verbatim by the window
//!   shell (`onboarding_window.rs`, T072 — the seam this port lands into):
//!   [`OnboardingWizard::build`] returns the children of React's `<main>` —
//!   the padded step area plus the footer — as ONE `El`, exactly the shape
//!   `OnboardingWindow::wizard` mounts. The host reads the window's
//!   `completion_action()` getter for React's `onComplete` prop.
//! - `useReducer(wizardReducer, INITIAL_STATE)` is `wizardState.ts`
//!   (T323, landed: `wizard_state.rs`): the machine's outputs arrive as
//!   feed-in parameters —
//!   [`OnboardingWizard::step`] / [`OnboardingWizard::entry`] (the branch
//!   selectors), [`OnboardingWizard::can_continue`]
//!   (`canContinueFromStepOne(state)`) and
//!   [`OnboardingWizard::connection_name`] (the sample directive's
//!   connection) — while the transitions (ENTER_*/SAVE_SUCCESS/
//!   IMPORT_SUCCESS/SAMPLE_*/CONTINUE/FINISH/SKIP/BACK) stay host-side
//!   behind the action slots [`OnboardingWizard::on_skip`] /
//!   [`OnboardingWizard::on_back`] / [`OnboardingWizard::on_continue`] /
//!   [`OnboardingWizard::on_finish`] / [`OnboardingWizard::on_open`], which
//!   the host wires to its dispatch. The [`Step`] / [`Entry`] seam types
//!   are absorbed into `wizard_state.rs` (T323) and re-exported here
//!   under their T114 names (the T021→T287 absorb pattern — every existing
//!   import path stays untouched); [`ImportApp`] is absorbed into
//!   `use_detected_import_source.rs` (T306, the import track) and
//!   re-exported here the same way; [`ImportSource`] stays here until the
//!   import track's remaining tasks absorb it.
//! - `onboardingGate.ts` (T268, later): `handleOpen`'s
//!   `updateSettings({ onboarding: completedOnboardingState() })` — the
//!   completed-state payload is the host's feed-in at its `on_open` seam.
//!   This module contributes the sample-database directive
//!   ([`OnboardingWizard::post_onboarding_sample`]: the
//!   `entry === 'sample' && connectionName` gate plus this file's
//!   [`SAMPLE_PRESET_SQL`]); the `localStorage.setItem` try/catch no-op is
//!   host persistence — the host stashes the directive, then persists the
//!   settings, then fires the window's completion action (React's stash →
//!   `updateSettings().then(onComplete)` order).
//! - The AI draft (`useModelProfileDraft` + `useAiStore`) is the
//!   `AiProviderStep` track (T148, later):
//!   [`OnboardingWizard::has_credentials`] (`aiDraft.hasCredentials`) and
//!   [`OnboardingWizard::busy`] (`aiDraft.savingState`) are feed-ins, and
//!   [`OnboardingWizard::finish_path`] ports `handleFinish`'s branch —
//!   nothing typed → `FINISH` with `aiLabel: null` (the journey never
//!   forces a key); credentials typed → `requestSave`, whose high-risk
//!   egress confirmation reports back through the draft's `onSaved` seam as
//!   the host's `FINISH { aiLabel }` dispatch.
//! - `useDetectedImportSource` (T306, landed:
//!   `use_detected_import_source.rs`): the detection pre-check is a host
//!   concern; the wizard keeps only what the React file itself owns —
//!   the `importSource` `useState('file')`
//!   ([`OnboardingWizard::import_source`], fed to `ImportStep` as
//!   `initialSource` at the host's slot build) and the `useEffect` that
//!   adopts the first detected client
//!   ([`OnboardingWizard::resync_detected`]: a detected client switches the
//!   source; `None` — nothing detected, or detection failed — never resets
//!   it). `detectedApps` rides the host's own `ImportStep` build (T177),
//!   not this shell.
//! - The seven children — `WelcomeStep` (T247), `ManualConnectionStep`
//!   (T203), `ImportStep` (T177), `SampleStep` (T226), `AiProviderStep`
//!   (T148), `DoneStep` (T177) and `WizardFooter` (T247) — are later tasks:
//!   each arrives as a host-fed `El` slot (the `OnboardingWindow::wizard`
//!   convention) mounted inside this shell's keyed seat. The step
//!   components' own props (`detectedApp`, `importResult`, `onSaved`, …)
//!   are the host's wiring at those slot builds (the `data_sync` step-host
//!   precedent: each child task owns its callbacks).
//! - The footer wiring ([`OnboardingWizard::footer_wiring`]) is exactly the
//!   prop list React hands `WizardFooter`: `step`, `canContinue`,
//!   `showContinue` (this file's `continueVisible` — step 1 of a non-manual
//!   entry; the manual entry's own Save button is the single way forward),
//!   `showFinish` (`step === 's2'`), `busy`, and the four action slots. The
//!   footer renders at every step except S3; its seat reserves the shared
//!   42pt chrome height (`WIZARD_FOOTER_HEIGHT`) so the sidebar's version
//!   row and the step indicator keep ending on the same bottom line
//!   (T287's one rule) before T247 fills it.
//! - `visibleStepNumber` (this file's test-helper export) stays an export
//!   ([`visible_step_number`]); its `stepIndexFor` core is the T323 seam,
//!   absorbed into [`super::wizard_state::step_index_for`].
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim — the root `onboarding-wizard`
//! is the T072 window's, and the steps'/footer's own testids
//! (`onboarding-step-*`, `onboarding-footer`) belong to their later tasks.
//! This shell's derived keys sit under the one `onboarding-wizard-` prefix:
//!
//! | part | key |
//! |------|-----|
//! | the `<main>`-children wrapper (the T072 one-`El` seam; no React element) | `onboarding-wizard-body` |
//! | the padded step area (React `flex flex-1 overflow-y-auto p-[40px_48px_32px]`) | `onboarding-wizard-content` |
//! | S0 welcome seat (T247 fills) | `onboarding-wizard-step-s0` |
//! | S1 manual seat (T203 fills) | `onboarding-wizard-step-s1-manual` |
//! | S1 import seat (T177 fills) | `onboarding-wizard-step-s1-import` |
//! | S1 sample seat (T226 fills) | `onboarding-wizard-step-s1-sample` |
//! | S2 AI seat (T148 fills) | `onboarding-wizard-step-s2` |
//! | S3 done seat (T177 fills) | `onboarding-wizard-step-s3` |
//! | footer seat (T247 fills) | `onboarding-wizard-footer` |
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75 — `p-[40px_48px_32px]` → 30/36/24pt); `overflow-y-auto` becomes
//! `min_h(0)` plus a non-stick scroll container; React's content `div` is a
//! flex ROW whose single child stretches to the full height — the seats are
//! block-flow columns and the step components own their sizing (the step
//! tasks add `grow` where their React roots had `flex-1`); the shell paints
//! nothing itself (no colours, no texts, no icons — `build` takes no theme,
//! unlike the area's other ports).

// The production consumer is the onboarding window's wizard track (the
// step/footer/state-machine tasks T148–T323 wire the host side); until then
// the audit module is the only caller, so the dead-code lint is silenced
// here rather than in a crate-wide allow (the `data_sync` precedent).
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, El, ScrollSpec};

use super::onboarding_layout::WIZARD_FOOTER_HEIGHT;
// The T323 absorb: the journey's step/entry enums live in `wizard_state.rs`
// now; they are re-exported here under their T114 names so the shell's,
// the steps' and the audits' import paths stay untouched (the
// onboarding_layout ← brand_sidebar precedent).
pub(crate) use super::wizard_state::{Entry, Step};
// The T306 absorb: the import-track app enum lives in
// `use_detected_import_source.rs` now (the detection hook owns
// `IMPORT_APP_ORDER`, its ordered universe, and the probe ids); it is
// re-exported here under its T114 name so the steps'/audits' import
// paths stay untouched (the same precedent).
pub(crate) use super::use_detected_import_source::ImportApp;

/// The wizard's action slots (`Rc<dyn Fn(&mut S)>`, the crate convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// React `ConnectionImportSource` (`ConnectionShareDialog.tsx:15`):
/// `'file' | ConnectionImportApp` — the wizard's own `importSource` state.
/// A seam type (see [`ImportApp`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ImportSource {
    /// React `useState('file')`'s default: pick a connection file.
    #[default]
    File,
    /// A detected client config on this machine.
    App(ImportApp),
}

/// React `SAMPLE_PRESET_SQL`: the preset query filled into the editor when
/// the user enters the main workspace through the sample-database path.
pub(crate) const SAMPLE_PRESET_SQL: &str =
    "SELECT region, SUM(amount) AS total\nFROM demo_sales\nGROUP BY region\nORDER BY total DESC;";

/// React `PostOnboardingSample` (`lib/windowManager.ts:188`): the
/// `localStorage` payload the done journey stashes under
/// `POST_ONBOARDING_SAMPLE_KEY` so the connection page can auto-open the
/// sample connection and pre-fill a query. The host's stash converts at its
/// seam (there is no localStorage in Rune).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PostOnboardingSampleVm {
    /// The seeded sample connection's name.
    pub(crate) connection_name: String,
    /// The preset SQL ([`SAMPLE_PRESET_SQL`]).
    pub(crate) sql: String,
}

/// React `handleFinish`'s branch: nothing typed → the journey finishes with
/// AI unconfigured; credentials typed → the draft's save path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinishPath {
    /// `!aiDraft.hasCredentials`: dispatch `FINISH { aiLabel: null }` — the
    /// journey never forces a key.
    FinishUnconfigured,
    /// `aiDraft.requestSave()`: may open the high-risk egress confirmation;
    /// the saved profile reports back through the draft's `onSaved` seam as
    /// the host's `FINISH { aiLabel: profile.name }` dispatch.
    RequestSave,
}

/// The prop list React's wizard hands `WizardFooter` — the back/next footer
/// wiring, computed by this shell until the footer port (T247) renders it.
pub(crate) struct FooterWiring<S: 'static> {
    /// `step`.
    pub(crate) step: Step,
    /// `canContinue` — `canContinueFromStepOne(state)`, fed in (T323's).
    pub(crate) can_continue: bool,
    /// `showContinue` — React's `continueVisible`: step 1 of a non-manual
    /// entry (the manual entry's own Save button is the way forward).
    pub(crate) show_continue: bool,
    /// `showFinish` — `step === 's2'`.
    pub(crate) show_finish: bool,
    /// `busy` — `aiDraft.savingState`.
    pub(crate) busy: bool,
    /// `onSkip` → the host's `SKIP` dispatch.
    pub(crate) on_skip: Option<ActionFn<S>>,
    /// `onBack` → the host's `BACK` dispatch.
    pub(crate) on_back: Option<ActionFn<S>>,
    /// `onContinue` → the host's `CONTINUE` dispatch.
    pub(crate) on_continue: Option<ActionFn<S>>,
    /// `onFinish` → the host's `handleFinish` (see [`FinishPath`]).
    pub(crate) on_finish: Option<ActionFn<S>>,
}

/// The first-run journey's step host: the step list (exactly one step seat
/// per `(step, entry)`), the footer wiring and its seat, and the two
/// completion handlers' seams. The window shell (T072) mounts
/// [`OnboardingWizard::build`] as its wizard slot; the state machine (T323),
/// the detection pre-check (T306), the AI draft (T148) and the seven
/// children (T148–T247) are the host's feed-ins and slots.
pub(crate) struct OnboardingWizard<S: 'static> {
    /// `state.step` — the current screen (the T323 machine's output).
    step: Step,
    /// `state.entry` — which screen step 1 renders.
    entry: Entry,
    /// `canContinueFromStepOne(state)` — the T323 helper's output, fed in.
    can_continue: bool,
    /// `state.connectionName` — the sample directive's connection.
    connection_name: Option<String>,
    /// `aiDraft.hasCredentials` — the AI draft seam (T148).
    has_credentials: bool,
    /// `aiDraft.savingState` — the footer's busy flag (T148).
    busy: bool,
    /// The wizard's own `importSource` state (`useState('file')`).
    import_source: ImportSource,
    /// The S0 `WelcomeStep` slot (T247 fills it).
    welcome: Option<El>,
    /// The S1 manual `ManualConnectionStep` slot (T203 fills it).
    manual: Option<El>,
    /// The S1 import `ImportStep` slot (T177 fills it).
    import: Option<El>,
    /// The S1 sample `SampleStep` slot (T226 fills it).
    sample: Option<El>,
    /// The S2 `AiProviderStep` slot (T148 fills it).
    ai: Option<El>,
    /// The S3 `DoneStep` slot (T177 fills it).
    done: Option<El>,
    /// The `WizardFooter` slot (T247 fills it).
    footer: Option<El>,
    /// Footer `onSkip` → the host's `SKIP` dispatch.
    on_skip: Option<ActionFn<S>>,
    /// Footer `onBack` → the host's `BACK` dispatch.
    on_back: Option<ActionFn<S>>,
    /// Footer `onContinue` → the host's `CONTINUE` dispatch.
    on_continue: Option<ActionFn<S>>,
    /// Footer `onFinish` → the host's `handleFinish` (see [`FinishPath`]).
    on_finish: Option<ActionFn<S>>,
    /// `DoneStep`'s `onOpen` → the host's `handleOpen` wiring.
    on_open: Option<ActionFn<S>>,
}

impl<S: 'static> Default for OnboardingWizard<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: 'static> OnboardingWizard<S> {
    /// React's start: `INITIAL_STATE` (S0, the manual entry — so
    /// `canContinueFromStepOne` is false), `importSource = 'file'`, nothing
    /// fed and no dispatch wired.
    pub(crate) fn new() -> Self {
        Self {
            step: Step::S0,
            entry: Entry::Manual,
            can_continue: false,
            connection_name: None,
            has_credentials: false,
            busy: false,
            import_source: ImportSource::File,
            welcome: None,
            manual: None,
            import: None,
            sample: None,
            ai: None,
            done: None,
            footer: None,
            on_skip: None,
            on_back: None,
            on_continue: None,
            on_finish: None,
            on_open: None,
        }
    }

    // ---- feed-ins (the later tasks' outputs) --------------------------------

    /// `state.step` — the current screen.
    pub(crate) fn step(mut self, step: Step) -> Self {
        self.step = step;
        self
    }

    /// `state.entry` — which screen step 1 renders.
    pub(crate) fn entry(mut self, entry: Entry) -> Self {
        self.entry = entry;
        self
    }

    /// `canContinueFromStepOne(state)` — the T323 helper's output, fed in
    /// by the host that owns the state machine.
    pub(crate) fn can_continue(mut self, can_continue: bool) -> Self {
        self.can_continue = can_continue;
        self
    }

    /// `state.connectionName` — the connection created in step 1 (the
    /// sample directive's connection).
    pub(crate) fn connection_name(mut self, name: impl Into<String>) -> Self {
        self.connection_name = Some(name.into());
        self
    }

    /// `aiDraft.hasCredentials` — the AI draft seam (T148).
    pub(crate) fn has_credentials(mut self, has_credentials: bool) -> Self {
        self.has_credentials = has_credentials;
        self
    }

    /// `aiDraft.savingState` — the footer's busy flag (T148).
    pub(crate) fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    // ---- host-fed slots (the children's ports) ------------------------------

    /// The S0 `WelcomeStep` slot (T247 fills it).
    pub(crate) fn welcome(mut self, el: El) -> Self {
        self.welcome = Some(el);
        self
    }

    /// The S1 manual `ManualConnectionStep` slot (T203 fills it).
    pub(crate) fn manual(mut self, el: El) -> Self {
        self.manual = Some(el);
        self
    }

    /// The S1 import `ImportStep` slot (T177 fills it).
    pub(crate) fn import(mut self, el: El) -> Self {
        self.import = Some(el);
        self
    }

    /// The S1 sample `SampleStep` slot (T226 fills it).
    pub(crate) fn sample(mut self, el: El) -> Self {
        self.sample = Some(el);
        self
    }

    /// The S2 `AiProviderStep` slot (T148 fills it).
    pub(crate) fn ai(mut self, el: El) -> Self {
        self.ai = Some(el);
        self
    }

    /// The S3 `DoneStep` slot (T177 fills it).
    pub(crate) fn done(mut self, el: El) -> Self {
        self.done = Some(el);
        self
    }

    /// The `WizardFooter` slot (T247 fills it); the seat mounts it at every
    /// step except S3.
    pub(crate) fn footer(mut self, el: El) -> Self {
        self.footer = Some(el);
        self
    }

    // ---- action slots (the host's dispatch wiring) --------------------------

    /// Footer `onSkip` → the host's `SKIP` dispatch (T323).
    pub(crate) fn on_skip(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_skip = Some(Rc::new(f));
        self
    }

    /// Footer `onBack` → the host's `BACK` dispatch (T323).
    pub(crate) fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// Footer `onContinue` → the host's `CONTINUE` dispatch (T323).
    pub(crate) fn on_continue(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_continue = Some(Rc::new(f));
        self
    }

    /// Footer `onFinish` → the host's `handleFinish` (see
    /// [`Self::finish_path`]).
    pub(crate) fn on_finish(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_finish = Some(Rc::new(f));
        self
    }

    /// `DoneStep`'s `onOpen` → the host's `handleOpen`: stash
    /// [`Self::post_onboarding_sample`], persist the completed onboarding
    /// settings (T268's payload), then fire the window's completion action
    /// (T072's `completion_action()`).
    pub(crate) fn on_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }

    // ---- the wiring the shell computes --------------------------------------

    /// The `WizardFooter` prop list (T247 renders it) — exactly what React
    /// computes in the wizard and hands down.
    pub(crate) fn footer_wiring(&self) -> FooterWiring<S> {
        FooterWiring {
            step: self.step,
            can_continue: self.can_continue,
            // React's `continueVisible`: step 1 of the manual entry advances
            // through the connection form's own Save button.
            show_continue: self.step == Step::S1 && self.entry != Entry::Manual,
            show_finish: self.step == Step::S2,
            busy: self.busy,
            on_skip: self.on_skip.clone(),
            on_back: self.on_back.clone(),
            on_continue: self.on_continue.clone(),
            on_finish: self.on_finish.clone(),
        }
    }

    /// React `handleFinish`'s branch (the journey never forces a key).
    pub(crate) fn finish_path(&self) -> FinishPath {
        if self.has_credentials {
            FinishPath::RequestSave
        } else {
            FinishPath::FinishUnconfigured
        }
    }

    /// React `handleOpen`'s stash payload: the sample-database directive,
    /// `Some` only on the sample entry with a seeded connection. The host
    /// persists it under `POST_ONBOARDING_SAMPLE_KEY`'s semantics.
    pub(crate) fn post_onboarding_sample(&self) -> Option<PostOnboardingSampleVm> {
        if self.entry != Entry::Sample {
            return None;
        }
        self.connection_name.as_ref().map(|name| PostOnboardingSampleVm {
            connection_name: name.clone(),
            sql: SAMPLE_PRESET_SQL.to_owned(),
        })
    }

    /// The done step's `onOpen` action for the host's `DoneStep` slot build
    /// (T177) — the T072 `completion_action` convention.
    pub(crate) fn open_action(&self) -> Option<ActionFn<S>> {
        self.on_open.clone()
    }

    /// The wizard's own `importSource` state (`useState('file')`), fed to
    /// `ImportStep` as `initialSource` at the host's slot build (T177).
    pub(crate) fn import_source(&self) -> ImportSource {
        self.import_source
    }

    /// React's `useEffect` on `detected`: a detected client becomes the
    /// default import source; `None` (nothing detected, or detection
    /// failed) never resets what an earlier detection chose.
    pub(crate) fn resync_detected(&mut self, detected: Option<ImportApp>) {
        if let Some(app) = detected {
            self.import_source = ImportSource::App(app);
        }
    }

    // ---- the tree ------------------------------------------------------------

    /// Paint the `<main>` children (the T072 seam): the padded step area
    /// above the footer, as one `El`. No theme — the shell paints nothing
    /// itself; the step and footer slots carry their own theming when their
    /// tasks land.
    pub(crate) fn build(self) -> El {
        let OnboardingWizard {
            step,
            entry,
            welcome,
            manual,
            import,
            sample,
            ai,
            done,
            footer,
            ..
        } = self;

        // React's step branches: exactly one step renders per (step, entry).
        let step_seat = match (step, entry) {
            (Step::S0, _) => seat("onboarding-wizard-step-s0", welcome),
            (Step::S1, Entry::Manual) => seat("onboarding-wizard-step-s1-manual", manual),
            (Step::S1, Entry::Import) => seat("onboarding-wizard-step-s1-import", import),
            (Step::S1, Entry::Sample) => seat("onboarding-wizard-step-s1-sample", sample),
            (Step::S2, _) => seat("onboarding-wizard-step-s2", ai),
            (Step::S3, _) => seat("onboarding-wizard-step-s3", done),
        };

        // React's content area: `flex flex-1 overflow-y-auto
        // p-[40px_48px_32px]` — 40/48/32px → 30/36/24pt.
        let content = col()
            .key("onboarding-wizard-content")
            .grow(1.0) // flex-1
            .min_h(0.0) // the area yields before it scrolls
            .pt(30.0)
            .px(36.0)
            .pb(24.0)
            .scroll(ScrollSpec { stick_bottom: false }) // overflow-y-auto
            .child(step_seat);

        let mut body = col()
            .key("onboarding-wizard-body")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(content);

        // The global footer — every step except S3. The seat reserves the
        // shared chrome height so the sidebar's version row and the step
        // indicator keep ending on the same bottom line (T287's one rule)
        // before the footer port (T247) fills it.
        if step != Step::S3 {
            let mut footer_seat = col()
                .key("onboarding-wizard-footer")
                .w_full()
                .shrink()
                .h(WIZARD_FOOTER_HEIGHT);
            if let Some(footer) = footer {
                footer_seat = footer_seat.child(footer);
            }
            body = body.child(footer_seat);
        }

        body
    }
}

/// One step's keyed seat: the host-fed step `El` (its task's port) mounted
/// inside, or an empty mount that paints nothing until that task lands.
fn seat(key: &'static str, fed: Option<El>) -> El {
    let mut mount = col().key(key).w_full();
    if let Some(el) = fed {
        mount = mount.child(el);
    }
    mount
}

/// React `visibleStepNumber` (the test helper kept next to the shell so
/// tests can assert the visible step number): the step indicator's number —
/// `None` on the entry (S0) and done (S3) screens.
pub(crate) fn visible_step_number(step: Step) -> Option<u8> {
    match super::wizard_state::step_index_for(step) {
        0 => None,
        index => Some(index),
    }
}
