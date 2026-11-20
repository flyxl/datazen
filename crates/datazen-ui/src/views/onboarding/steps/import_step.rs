//! Port of `packages/frontend/src/windows/onboarding/steps/ImportStep.tsx`
//! (T177): S1 for the import entry — step 1 of 2 of the first-run journey
//! when the user picks "import connections" on the welcome screen.
//!
//! The React file is the inline (never-a-dialog) mount of the shared
//! connection-import flow: the six-button source picker (`file` plus the
//! five clients, in `SOURCE_ORDER`, with a green check on every client
//! whose config was detected on this machine), the bordered card carrying
//! `ConnectionImportFields` in its `inline` variant with the success
//! banner above the footer, and the card's footer — the primary submit
//! button (flask glyph; `导入中…` while busy) and the flex-1 mono
//! detecting line.
//!
//! ## What maps where
//!
//! - `useState(initialSource)` (the only state this file itself owns) is
//!   [`ImportStepVm`] — one field, the chosen [`ImportSource`] — kept in
//!   the host's `S` behind [`ImportStep::on_step`] (the
//!   `ManualStepVm`/`driverQuery` convention: the picker buttons write
//!   through the accessor, `setSource(s)` verbatim).
//! - `useConnectionImport({ source, onImportSuccess: handleSuccess })` is
//!   T346 (later): the hook's machinery arrives as feed-ins and action
//!   slots, never as IPC in this crate.
//!   - The hook's state snapshot is the T103 [`ImportVm`] (the cloneable
//!     `ConnectionImportState` projection the fields port already
//!     defines: `password`/`dataPath`/`pathFound`/`busy`/`submitting`/
//!     `localError`/`selectedFile`/`passwordRequired`), handed in at
//!     [`ImportStep::new`] with the two ports back into `S`
//!     ([`ImportStep::on_vm`] / [`ImportStep::on_action`], forwarded to
//!     `ConnectionImportFields`). The variant prop pins
//!     `vm.inline = true` here, exactly as React passes
//!     `variant="inline"`.
//!   - `detecting` — the hook's flag the footer's mono line needs ALONE
//!     (the vm folds it into `busy = submitting || detecting`) — is the
//!     separate feed-in [`ImportStep::detecting`].
//!   - `imp.submit()` is the action slot [`ImportStep::on_submit`]; the
//!     host runs the T346 import flow behind it.
//!   - The hook's source-change effect (reset the fields, re-detect the
//!     client's config path) is the host's resync between this step's
//!     `source` writes and its machinery seam: the host observes the
//!     accessor's writes and runs its detect task, reporting back through
//!     the `ImportVm` halves.
//! - `handleSuccess` (the hook's `onImportSuccess` arm) builds the summary
//!   the wizard's `IMPORT_SUCCESS` dispatch carries:
//!   [`import_summary`] ports the pure half — `{ imported, overwritten,
//!   source: result.sourceFormat ?? sourceLabel(source, …) }`. The
//!   backend result itself is the seam struct [`ImportOutcome`] (the
//!   three fields of `ConnectionImportResult` this file reads; T346's
//!   port absorbs it).
//! - `ImportSummary` (`wizardState.ts:21`) is absorbed into
//!   `wizard_state.rs` (T323, the state machine's port) and re-exported
//!   here — the producer's side of the seam; `DoneStep`'s summary reads
//!   it.
//! - `initialSource` is the wizard's own `importSource` state (T114's
//!   [`OnboardingWizard::import_source`], seeded by
//!   `resync_detected`); `detectedApps` is `useDetectedImportSource`'s
//!   output (T306, landed: `use_detected_import_source.rs`) — the feed-in
//!   [`ImportStep::detected_apps`], every client whose config was found.
//! - `CONNECTION_IMPORT_APP_LABEL` (`ConnectionShareDialog.tsx:17`) is
//!   reused from T247's port of the same map
//!   ([`super::welcome_step::import_app_label`]); the ConnectionShareDialog
//!   port (T137) absorbs it.
//! - `importResult?: ImportSummary` (the wizard state's summary, for the
//!   success banner) is the feed-in [`ImportStep::import_result`], gated
//!   by `written = imported + overwritten > 0` exactly as React's
//!   `importResult && written > 0`.
//! - `imp.primaryActionLabel` (`!appImport && !selectedFile ?
//!   chooseImportFile : importAction`) is derived from the vm at build,
//!   as the hook derives it from its own state.
//! - i18n: the seven reads (`onboarding.s1.stepLabel` — via the shared
//!   tag — `importTitle`, `importSubtitle`, `importFileSource`,
//!   `importing`, `importDetecting`, and the banner's `importSuccess` /
//!   `importUpdated`) port to `crate::i18n::t` with the same keys; the
//!   `{count}`/`{source}` placeholders are string-replaced, exactly as
//!   React's `t(…, { count, source })` does.
//! - Icons: the registry's `ICON_CIRCLE_CHECK` (lucide `circle-check-big`
//!   — React 0.487's `CheckCircle2`, the T107 ruling) and
//!   `ICON_FLASK_CONICAL`. No new registry entries.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; this step's own derived keys
//! sit under the one `onboarding-step-s1-import-` prefix and the picker /
//! form / banner parts under React's own `onboarding-import-` testid
//! prefix (the manual step's ruling):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid="onboarding-step-s1-import"`) | `onboarding-step-s1-import` |
//! | heading `h2` | `onboarding-step-s1-import-title` |
//! | subtitle `p` | `onboarding-step-s1-import-subtitle` |
//! | source picker row (`data-testid="onboarding-import-sources"`) | `onboarding-import-sources` |
//! | one source button (`data-testid={`onboarding-import-source-${s}`}`) | `onboarding-import-source-<id>` |
//! | that button's label `span` | `onboarding-import-source-<id>-label` |
//! | detected checkmark (`data-testid={`onboarding-import-found-${s}`}`) | `onboarding-import-found-<id>` |
//! | the form card (`data-testid="onboarding-import-form"`) | `onboarding-import-form` |
//! | the card's scroll area | `onboarding-import-form-scroll` |
//! | success banner (`data-testid="onboarding-import-success"`) | `onboarding-import-success` |
//! | the banner's check glyph / text | `onboarding-import-success-icon` / `-text` |
//! | the card's footer | `onboarding-import-footer` |
//! | submit button (`data-testid="onboarding-import-submit"`) | `onboarding-import-submit` |
//! | the detecting line | `onboarding-import-detecting` |
//!
//! `<id>` is the React source id verbatim (`file`, `dbeaver`, `datagrip`,
//! `navicat`, `tableplus`, `dbx` — [`source_id`]). The fields' own keys
//! are T103's (`import-data-path` & co, the `connection_import_fields.rs`
//! table). Known deltas (documented, not silent): pixel classes become
//! points (×0.75); the unselected source buttons' `hover:border-edge-hi
//! hover:text-fg` recolours have no Rune counterpart (`El::hover` is a
//! fill hover — the `welcome_step.rs` ruling) — they hover the kit's
//! raised surface instead, while the SELECTED button keeps React's
//! hover-free class list; `transition-colors` is static; `flex-wrap` on
//! the picker row keeps React's wrap at narrow widths.

// The onboarding wizard host (T114's `-step-s1-import` seat) is the
// production consumer; until the window track wires it, this crate's
// audit is the only caller.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, ScrollSpec, TextStyle};

use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::connection::connection_import_fields::{
    ConnectionImportFields, ImportAction, ImportVm,
};
use crate::i18n::t;
use crate::icons::{ICON_CIRCLE_CHECK, ICON_FLASK_CONICAL};
use crate::ui::kit::kit;
use crate::views::onboarding::onboarding_wizard::{ImportApp, ImportSource};
// The T323 absorb: the summary the import produces lives in `wizard_state.rs`
// (the state machine's `IMPORT_SUCCESS` payload); re-exported here so the
// audits' `import_step::ImportSummary` path stays untouched.
pub(crate) use crate::views::onboarding::wizard_state::ImportSummary;

use super::step_tag::step_tag;
use super::welcome_step::import_app_label;

/// React `max-w-[56ch]` on the subtitle: 56 × the "0" advance (≈ 0.5em at
/// 13.5px → 6.75px) ≈ 378px → 283.5pt. Approximate by construction — the
/// `ch` unit has no Rune counterpart (the `ai_provider_step.rs` ruling).
const SUBTITLE_MAX_W: f32 = 283.5;

/// The step-local VM accessor (the `ManualStepVm` convention).
type StepVmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ImportStepVm + 'static>;
/// The import machinery's VM accessor, forwarded to the T103 fields.
type VmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ImportVm + 'static>;
/// The fields' action port, forwarded to the T103 fields.
type ImportActionFn<S> = Rc<dyn Fn(&mut S, ImportAction) + 'static>;
/// The step's plain action slots (`Rc<dyn Fn(&mut S)>`, the crate
/// convention).
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// React `SOURCE_ORDER` (`ImportStep.tsx:27`): the picker's six sources in
/// React's order — the file flow first, then the five clients.
pub(crate) const SOURCE_ORDER: [ImportSource; 6] = [
    ImportSource::File,
    ImportSource::App(ImportApp::Dbeaver),
    ImportSource::App(ImportApp::Datagrip),
    ImportSource::App(ImportApp::Navicat),
    ImportSource::App(ImportApp::Tableplus),
    ImportSource::App(ImportApp::Dbx),
];

/// The React source id behind a picker button's testid (`'file'` or the
/// client id) — the `onboarding-import-source-<id>` /
/// `onboarding-import-found-<id>` postfixes, verbatim. The client ids
/// delegate to [`ImportApp::source_id`] (T306's absorb — the one truth
/// the host's detection probes share).
pub(crate) fn source_id(source: ImportSource) -> &'static str {
    match source {
        ImportSource::File => "file",
        ImportSource::App(app) => app.source_id(),
    }
}

/// React `sourceLabel`: the file flow's translated label, or the client's
/// display name from `CONNECTION_IMPORT_APP_LABEL` (T247's port of the
/// same map).
pub(crate) fn source_label<'a>(source: ImportSource, file_label: &'a str) -> &'a str {
    match source {
        ImportSource::File => file_label,
        ImportSource::App(app) => import_app_label(app),
    }
}

/// The half of React's `ConnectionImportResult`
/// (`useConnectionImport.ts:9`) this file reads in `handleSuccess`. A
/// T346 seam type (the hook's port absorbs it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportOutcome {
    /// `imported`.
    pub(crate) imported: u32,
    /// `overwritten`.
    pub(crate) overwritten: u32,
    /// `sourceFormat` — the backend's format name when it reports one.
    pub(crate) source_format: Option<String>,
}

/// React `handleSuccess` (the hook's `onImportSuccess` arm): the summary
/// the wizard's `IMPORT_SUCCESS` dispatch carries — `source` is the
/// backend's format name when it reports one, else the picker's label for
/// the source that ran. The host runs this when its T346 import flow
/// resolves, then dispatches.
pub(crate) fn import_summary(source: ImportSource, outcome: &ImportOutcome) -> ImportSummary {
    ImportSummary {
        imported: outcome.imported,
        overwritten: outcome.overwritten,
        source: outcome.source_format.clone().unwrap_or_else(|| {
            source_label(source, t("onboarding.s1.importFileSource")).to_owned()
        }),
    }
}

/// The half of React's state this file itself owns: the chosen source
/// (`useState(initialSource)`). The host keeps this in its `S` and clones
/// it per frame, exactly the T103 `FormBodyVm` discipline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportStepVm {
    /// `source` — the import source the picker has chosen. The default is
    /// React's `useState('file')` fallback; the wizard seeds the real
    /// `initialSource` (T114's `import_source()`).
    pub(crate) source: ImportSource,
}

/// The journey's import-entry step: the source picker, the bordered card
/// carrying the shared import fields (inline variant) with the success
/// banner, and the submit footer. Built per frame over the host's
/// [`ImportStepVm`] and the T346 machinery's [`ImportVm`].
pub(crate) struct ImportStep<S: 'static> {
    /// The step-local half of React's state (the chosen source).
    step: ImportStepVm,
    /// The import machinery's snapshot (the T346 seam, T103's `ImportVm`).
    imp: ImportVm,
    /// The machinery's `detecting` flag alone (`ImportVm` folds it into
    /// `busy`; the footer's mono line needs it separately).
    detecting: bool,
    /// `importResult` — the wizard state's summary, for the success banner.
    import_result: Option<ImportSummary>,
    /// `detectedApps` — every client whose config was found on this
    /// machine (T306's output).
    detected_apps: Vec<ImportApp>,
    /// The step-local VM accessor (`setSource`'s writes land in `S`).
    on_step: Option<StepVmFn<S>>,
    /// The machinery's VM accessor, forwarded to the T103 fields.
    on_vm: Option<VmFn<S>>,
    /// The fields' action port, forwarded to the T103 fields.
    on_action: Option<ImportActionFn<S>>,
    /// `imp.submit()` — the host's import flow (T346).
    on_submit: Option<ActionFn<S>>,
}

impl<S: 'static> ImportStep<S> {
    /// React's mount on S1-import: the wizard's `initialSource` (the VM
    /// the host seeds from T114's `import_source()`) and the machinery's
    /// snapshot, no seams wired until the host hands them over.
    pub(crate) fn new(step: ImportStepVm, imp: ImportVm) -> Self {
        ImportStep {
            step,
            imp,
            detecting: false,
            import_result: None,
            detected_apps: Vec::new(),
            on_step: None,
            on_vm: None,
            on_action: None,
            on_submit: None,
        }
    }

    /// The machinery's `detecting` flag — the footer's mono line.
    pub(crate) fn detecting(mut self, detecting: bool) -> Self {
        self.detecting = detecting;
        self
    }

    /// `importResult` — the wizard state's import summary (`None` is
    /// React's undefined: no import has succeeded yet).
    pub(crate) fn import_result(mut self, result: ImportSummary) -> Self {
        self.import_result = Some(result);
        self
    }

    /// `detectedApps` — every client whose config was found on this
    /// machine (T306's `useDetectedImportSource` output).
    pub(crate) fn detected_apps(mut self, apps: impl Into<Vec<ImportApp>>) -> Self {
        self.detected_apps = apps.into();
        self
    }

    /// The step-local VM accessor (`source`'s writes land in `S`).
    pub(crate) fn on_step(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut ImportStepVm + 'static,
    ) -> Self {
        self.on_step = Some(Rc::new(f));
        self
    }

    /// The machinery's VM accessor — the T103 `on_vm` seam, forwarded.
    pub(crate) fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut ImportVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    /// The fields' action port — the T103 `on_action` seam, forwarded.
    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, ImportAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// `imp.submit()` — the host's import flow (T346); the machinery's
    /// success reports back through [`import_summary`] and the host's
    /// `IMPORT_SUCCESS` dispatch.
    pub(crate) fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// Paint the step. `dark` drives the kit tokens.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let ImportStep {
            step,
            mut imp,
            detecting,
            import_result,
            detected_apps,
            on_step,
            on_vm,
            on_action,
            on_submit,
        } = self;

        // React pins the fields' variant here: `variant="inline"`.
        imp.inline = true;

        let file_label = t("onboarding.s1.importFileSource");

        // React's `written`: the banner shows only when the import
        // actually wrote something (brand-new plus replaced).
        let written = import_result
            .as_ref()
            .map_or(0, |r| r.imported + r.overwritten);

        // The shared fields body over the forwarded seams (T103).
        let mut fields = ConnectionImportFields::<S>::new(imp.clone());
        if let Some(vm_fn) = on_vm.clone() {
            fields = fields.on_vm(move |s: &mut S| vm_fn(s));
        }
        if let Some(action_fn) = on_action.clone() {
            fields = fields.on_action(move |s: &mut S, a| action_fn(s, a));
        }
        let fields = fields.build(dark);

        // The card's scroll area: `min-h-0 flex-1 overflow-y-auto p-5`
        // around the fields, with the success banner below them.
        let mut scroll = col()
            .key("onboarding-import-form-scroll")
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0
            .scroll(ScrollSpec { stick_bottom: false }) // overflow-y-auto
            .px(15.0) // p-5
            .py(15.0)
            .child(fields);
        if written > 0 {
            if let Some(result) = &import_result {
                scroll = scroll.child(success_banner(&k, result));
            }
        }

        // The footer's primary label: the hook's `primaryActionLabel`
        // memo (`!appImport && !selectedFile ? chooseImportFile :
        // importAction`), swapped to the busy entry while importing.
        let primary = if !imp.app_import && imp.selected_file.is_none() {
            t("connShare.chooseImportFile")
        } else {
            t("connShare.importAction")
        };
        let mut submit = Button::new(if imp.busy {
            t("onboarding.s1.importing").to_owned()
        } else {
            primary.to_owned()
        })
        .variant(ButtonVariant::Contrast) // React primary (T018)
        .control_size(ControlSize::ExtraLarge) // React md (h-9)
        .icon(&ICON_FLASK_CONICAL) // <FlaskConical className="h-3.5 w-3.5" />
        .disabled(imp.busy);
        if let Some(on_submit) = on_submit {
            submit = submit.on_press(move |s: &mut S| on_submit(s));
        }

        // The card's footer: `flex shrink-0 items-center gap-3 border-t
        // border-edge bg-surface-alt px-5 py-3`.
        let footer = row()
            .key("onboarding-import-footer")
            .w_full()
            .shrink() // shrink-0
            .center_y() // items-center
            .gap(9.0) // gap-3
            .border_sides([0.75, 0.0, 0.0, 0.0], k.c.border_base) // border-t border-edge
            .fill(k.c.layer_01) // bg-surface-alt (the T141 ruling)
            .px(15.0) // px-5
            .py(9.0) // py-3
            .child(submit.build(k).key("onboarding-import-submit")) // React data-testid, verbatim
            .child(
                // The detecting line: flex-1 text-xs font-mono
                // text-fg-muted — the detecting entry, or the empty
                // string (React renders the empty span, not nothing).
                text(if detecting {
                    t("onboarding.s1.importDetecting").to_owned()
                } else {
                    String::new()
                })
                .key("onboarding-import-detecting")
                .font(TextStyle::new(9.0, 400.0).mono()) // text-xs font-mono
                .grow(1.0) // flex-1
                .color(k.c.text_muted), // text-fg-muted
            );

        // The form card: `flex min-h-0 flex-1 flex-col overflow-hidden
        // rounded-xl border border-edge bg-surface` — the inline form,
        // never a dialog.
        let form_card = col()
            .key("onboarding-import-form") // React data-testid, verbatim
            .w_full()
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0
            .radius(k.radius.lg) // rounded-xl
            .clip_rounded(k.radius.lg) // overflow-hidden
            .border(0.75, k.c.border_base) // border border-edge
            .fill(k.c.surface_base) // bg-surface
            .child(scroll)
            .child(footer);

        // The source picker: `mb-4 flex flex-wrap gap-2`, one button per
        // source in React's order.
        let mut picker = row()
            .key("onboarding-import-sources") // React data-testid, verbatim
            .w_full()
            .flex_wrap() // flex-wrap
            .gap(6.0) // gap-2
            .mb(12.0); // mb-4
        for source in SOURCE_ORDER {
            picker = picker.child(source_button(
                &k,
                source,
                file_label,
                step.source == source,
                &detected_apps,
                &on_step,
            ));
        }

        col()
            .key("onboarding-step-s1-import") // React data-testid, verbatim
            .w_full()
            .grow(1.0) // flex-1
            .min_h(0.0) // min-h-0
            // <StepTag label={t('onboarding.s1.stepLabel')} />
            .child(step_tag(&k, t("onboarding.s1.stepLabel")))
            // The heading: mb-2.5 text-[25px] font-bold tracking-tight
            // leading-tight text-fg (the siblings' metrics).
            .child(
                text(t("onboarding.s1.importTitle").to_owned())
                    .key("onboarding-step-s1-import-title")
                    .font(TextStyle::new(18.75, 700.0).tracking(-0.46875).lh(23.4375))
                    .mb(7.5) // mb-2.5
                    .color(k.c.text_base),
            )
            // The subtitle: mb-5 max-w-[56ch] text-[13.5px] leading-[1.65]
            // text-fg-secondary.
            .child(
                text(t("onboarding.s1.importSubtitle").to_owned())
                    .key("onboarding-step-s1-import-subtitle")
                    .font(TextStyle::new(10.125, 400.0).lh(16.7))
                    .max_w(SUBTITLE_MAX_W)
                    .mb(15.0) // mb-5
                    .color(k.c.text_muted),
            )
            .child(picker)
            .child(form_card)
    }
}

/// One picker button: `inline-flex items-center gap-1.5 rounded-lg border
/// px-3 py-1.5 text-[12.5px] font-medium` — the selected source wears
/// `border-accent/55 bg-accent/10 text-fg`, the others `border-edge
/// bg-surface text-fg-secondary` (with the hover fill standing in for
/// React's border/ink hover — see the deltas).
fn source_button<S: 'static>(
    k: &Kit,
    source: ImportSource,
    file_label: &str,
    selected: bool,
    detected_apps: &[ImportApp],
    on_step: &Option<StepVmFn<S>>,
) -> El {
    let id = source_id(source);
    let mut button = row()
        .key(format!("onboarding-import-source-{id}")) // React data-testid, verbatim
        .center_y() // items-center
        .gap(4.5) // gap-1.5
        .radius(6.0) // rounded-lg (8px → 6pt)
        .px(9.0) // px-3
        .py(4.5) // py-1.5
        .role("button")
        .focusable(true)
        .child(
            text(source_label(source, file_label).to_owned())
                .key(format!("onboarding-import-source-{id}-label"))
                .font(TextStyle::new(9.375, 500.0)) // text-[12.5px] font-medium
                .color(if selected {
                    k.c.text_base // text-fg
                } else {
                    k.c.text_muted // text-fg-secondary
                }),
        );
    if selected {
        button = button
            .border(0.75, k.c.bg_accent.alpha(0.55)) // border-accent/55
            .fill(k.c.bg_accent.alpha(0.10)); // bg-accent/10
    } else {
        button = button
            .border(0.75, k.c.border_base) // border-edge
            .fill(k.c.surface_base) // bg-surface
            // hover:border-edge-hi hover:text-fg — see the deltas.
            .hover(k.c.surface_raised);
    }
    // The detected checkmark: `detectedApps.includes(s)` — the apps list
    // never carries `file`, so the file button shows none either.
    if let ImportSource::App(app) = source {
        if detected_apps.contains(&app) {
            button = button.child(
                icon(&ICON_CIRCLE_CHECK, 6.75) // h-3 w-3
                    .key(format!("onboarding-import-found-{id}")) // React data-testid, verbatim
                    .color(k.c.success.fg), // text-green
            );
        }
    }
    // onClick={() => setSource(s)} — the step-local write.
    if let Some(on_step) = on_step {
        let on_step = on_step.clone();
        button = button.on_press(move |s: &mut S| on_step(s).source = source);
    }
    button
}

/// The success banner: `mt-4 flex items-start gap-2.5 rounded-[10px]
/// border border-green/25 bg-green/10 p-3.5 text-[12.5px] leading-[1.6]
/// text-green` — the imported entry when the run created connections, the
/// updated entry when it only replaced them (React's nested ternary).
fn success_banner(k: &Kit, result: &ImportSummary) -> El {
    let message = if result.imported > 0 {
        t("onboarding.s1.importSuccess")
            .replace("{count}", &result.imported.to_string())
            .replace("{source}", &result.source)
    } else {
        t("onboarding.s1.importUpdated")
            .replace("{count}", &result.overwritten.to_string())
            .replace("{source}", &result.source)
    };
    row()
        .key("onboarding-import-success") // React data-testid, verbatim
        .w_full()
        .mt(12.0) // mt-4
        .items_start() // items-start
        .gap(7.5) // gap-2.5
        .radius(7.5) // rounded-[10px]
        .border(0.75, k.c.success.fg.alpha(0.25)) // border-green/25
        .fill(k.c.success.fg.alpha(0.10)) // bg-green/10
        .px(10.5) // p-3.5
        .py(10.5)
        .child(
            icon(&ICON_CIRCLE_CHECK, 11.25) // h-[15px] w-[15px]
                .key("onboarding-import-success-icon")
                .mt(1.5) // mt-0.5
                .shrink() // shrink-0
                .color(k.c.success.fg), // text-green
        )
        .child(
            text(message)
                .key("onboarding-import-success-text")
                .font(TextStyle::new(9.375, 400.0).lh(15.0)) // text-[12.5px] leading-[1.6]
                .min_w(0.0)
                .wrap()
                .color(k.c.success.fg), // text-green
        )
}
