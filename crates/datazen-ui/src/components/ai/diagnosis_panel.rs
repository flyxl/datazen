//! Rune port of `packages/frontend/src/components/ai/DiagnosisPanel.tsx`
//! (177 lines, T239): the AI diagnosis strip a failed query mounts — the
//! unconfigured one-line notice, the stethoscope header with the manual
//! re-diagnose button and the close control, and the four body states
//! (diagnosing spinner / error card / the three-section result / the
//! idle prompt).
//!
//! ## React → Rune item map
//!
//! | React source | Rune item |
//! |---|---|
//! | `handleDiagnose`'s `if (!diagnosisContext.ok) return` + spread | [`diagnose_request`] |
//! | the mount `useEffect`'s `!diagnosis && !isDiagnosing && !diagnosisError` gate | [`DiagnosisPanelState::should_auto_diagnose`] |
//! | the `aiStore` fields the component reads | [`DiagnosisPanelState`] |
//! | `handleApply` (`onApplySql` + `clearDiagnosis` + `onClose`) | the apply button's compound press |
//! | `handleClose` (`clearDiagnosis` + `onClose`) | both close controls' compound press |
//! | `!isConfigured` early return (54–81) | [`DiagnosisPanel::unconfigured`] |
//! | the configured panel (83–176) | [`DiagnosisPanel::build`] |
//!
//! ## Deliberate boundaries
//!
//! * **`diagnoseError` / `clearDiagnosis` are host actions, not store
//!   reads.** The component takes them as callbacks (`on_diagnose`,
//!   `clear_diagnosis`) exactly as React's `useAiStore((s) => s.diagnoseError)`
//!   hands it the action; the store port (T088's [`crate::ai_store::AiStore`]
//!   slice) already mirrors `begin_diagnose` / `complete_diagnose` /
//!   `fail_diagnose` / `clear_diagnose`.
//! * **`openSettingsWindow('ai')` is a pass-through** (`on_open_settings`),
//!   the same boundary the AiChatPanel shell draws for its unconfigured
//!   button: opening a window is the host's job.
//! * **The mount effect stays host-side.** Rune rebuilds the view every
//!   frame, so React's `useEffect(() => { … }, [])` — run once on mount —
//!   becomes [`DiagnosisPanelState::should_auto_diagnose`], the exact
//!   condition React evaluates; the host that mounts the panel calls it
//!   once and fires `on_diagnose` through [`diagnose_request`].
//! * **Icons** come from the registry: `Stethoscope` (T100's entry, the
//!   accent-coloured header glyph and the diagnose/apply button glyphs at
//!   React's 14 / 12 point sizes), `Settings`, `X`, and `ArrowDownToLine`
//!   (new in this task, verified against lucide-static 0.487.0).
//! * **`text-green-400` on the suggested-SQL `<pre>` is `k.c.success.fg`**,
//!   the mapping `ai_code_block.rs` (T219) already established for AI-side
//!   green ink; `bg-surface-alt` is `k.c.layer_01` and `bg-surface` is
//!   `k.c.bg_base` (the same two precedents).
//! * **`text-fg-secondary` is `k.c.text_muted`**, the mapping the
//!   `ai_provider_step_audit.rs` vectors assert for secondary copy.
//! * **No `select-text` port surface.** Selection is the runtime's text
//!   model, not a style (the error card wraps, matching the CSS default).
//!
//! ## Key convention
//!
//! React gives none of these a `data-testid`, so the keys are new and
//! documented here for the wiring task that mounts the panel:
//!
//! | part | key |
//! |---|---|
//! | unconfigured root | `diagnosis-panel-unconfigured` |
//! | unconfigured notice row | `diagnosis-panel-unconfigured-row` |
//! | unconfigured text | `diagnosis-panel-unconfigured-text` |
//! | unconfigured settings button | `diagnosis-panel-unconfigured-settings` |
//! | unconfigured close | `diagnosis-panel-unconfigured-close` |
//! | configured root | `diagnosis-panel` |
//! | header | `diagnosis-panel-header` |
//! | title | `diagnosis-panel-title` |
//! | manual diagnose button | `diagnosis-panel-diagnose` |
//! | close | `diagnosis-panel-close` |
//! | body | `diagnosis-panel-body` |
//! | diagnosing row | `diagnosis-panel-diagnosing` |
//! | error card | `diagnosis-panel-error` |
//! | explanation label / text | `diagnosis-panel-explanation-label` / `-text` |
//! | changes label / list | `diagnosis-panel-changes-label` / `-list` |
//! | suggested label / pre / apply | `diagnosis-panel-suggested-label` / `-sql` / `-apply` |
//! | idle prompt | `diagnosis-panel-idle` |
//!
//! No production consumer yet: the query workspace wiring task mounts it
//! under a failed execution, next to `QueryErrorPanel` (T100).

// The production consumer is the query-workspace wiring task; until then
// the component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use datazen_ui_api::{DiagnosisResult, QueryActionBuildResult, QueryDiagnosisContext};
use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

use crate::i18n::t;
use crate::icons::{ICON_ARROW_DOWN_TO_LINE, ICON_SETTINGS, ICON_STETHOSCOPE, ICON_X};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// React's `max-h-[200px]` on the body (`DiagnosisPanel.tsx:112`): the
/// scrolling window the four states mount inside.
pub(crate) const BODY_MAX_HEIGHT: f32 = 200.0;

/// React `handleDiagnose`'s spread object: `QueryDiagnosisParams` with
/// `sql` / `errorMessage` overwritten by the redacted copies
/// (`{ ...diagnosisParams, sql: safeSql, errorMessage: safeErrorMessage }`).
/// This is the `diagnoseError` request the host receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiagnoseRequest {
    pub db_session_id: String,
    pub database: String,
    /// `safeSql` — the redacted copy, not the editor's `sql`.
    pub sql: String,
    /// `safeErrorMessage` — the redacted copy.
    pub error_message: String,
}

/// `handleDiagnose`: nothing when the context failed to build, the spread
/// request when it is ok. The manual button and the host's mount effect
/// both go through here, so an invalid context diagnoses nothing —
/// React's "does not diagnose when the context is missing or invalid"
/// contract.
pub(crate) fn diagnose_request(
    context: &QueryActionBuildResult<QueryDiagnosisContext>,
) -> Option<DiagnoseRequest> {
    let inner = match context {
        QueryActionBuildResult::Success(success) => &success.context,
        QueryActionBuildResult::Failure(_) => return None,
    };
    Some(DiagnoseRequest {
        db_session_id: inner.diagnosis_params.db_session_id.clone(),
        database: inner.diagnosis_params.database.clone(),
        sql: inner.safe_sql.clone(),
        error_message: inner.safe_error_message.clone(),
    })
}

/// The `aiStore` slice the component reads (React's five selectors minus
/// the two actions, which arrive as callbacks): the diagnosis result, the
/// in-flight flag, the failure text and the configured gate.
pub(crate) struct DiagnosisPanelState {
    pub diagnosis: Option<DiagnosisResult>,
    pub is_diagnosing: bool,
    pub diagnosis_error: Option<String>,
    pub is_configured: bool,
}

impl DiagnosisPanelState {
    pub fn new() -> Self {
        Self {
            diagnosis: None,
            is_diagnosing: false,
            diagnosis_error: None,
            is_configured: true,
        }
    }

    /// The mount `useEffect`'s condition verbatim
    /// (`!diagnosis && !isDiagnosing && !diagnosisError`): the host that
    /// mounts the panel asks once and fires `on_diagnose` when it holds.
    pub fn should_auto_diagnose(&self) -> bool {
        self.diagnosis.is_none() && !self.is_diagnosing && self.diagnosis_error.is_none()
    }
}

impl Default for DiagnosisPanelState {
    fn default() -> Self {
        Self::new()
    }
}

/// The panel. Build it with the caller's `aiStore` slice and the built
/// diagnosis context, then attach the five host callbacks.
pub(crate) struct DiagnosisPanel<'a, S: 'static> {
    state: &'a DiagnosisPanelState,
    context: &'a QueryActionBuildResult<QueryDiagnosisContext>,
    on_diagnose: Option<Rc<dyn Fn(&mut S, DiagnoseRequest)>>,
    on_apply_sql: Option<Rc<dyn Fn(&mut S, &str)>>,
    clear_diagnosis: Option<Rc<dyn Fn(&mut S)>>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    on_open_settings: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> DiagnosisPanel<'a, S> {
    pub(crate) fn new(
        state: &'a DiagnosisPanelState,
        context: &'a QueryActionBuildResult<QueryDiagnosisContext>,
    ) -> Self {
        Self {
            state,
            context,
            on_diagnose: None,
            on_apply_sql: None,
            clear_diagnosis: None,
            on_close: None,
            on_open_settings: None,
        }
    }

    /// React's `diagnoseError` — the host action behind the manual button
    /// and the mount effect.
    pub(crate) fn on_diagnose(
        mut self,
        f: impl Fn(&mut S, DiagnoseRequest) + 'static,
    ) -> Self {
        self.on_diagnose = Some(Rc::new(f));
        self
    }

    /// React's `onApplySql` prop.
    pub(crate) fn on_apply_sql(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_apply_sql = Some(Rc::new(f));
        self
    }

    /// React's `clearDiagnosis` store action — `handleApply` and
    /// `handleClose` both invoke it before closing.
    pub(crate) fn clear_diagnosis(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.clear_diagnosis = Some(Rc::new(f));
        self
    }

    /// React's `onClose` prop.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// `openSettingsWindow('ai')` — the unconfigured notice's configure
    /// button. A window open is the host's job, so the wiring task passes
    /// the opener exactly as the AiChatPanel shell does for its own
    /// unconfigured button.
    pub(crate) fn on_open_settings(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_settings = Some(Rc::new(f));
        self
    }

    /// The panel. React's early return picks the branch: the unconfigured
    /// notice (`DiagnosisPanel.tsx:54–81`) or the configured panel
    /// (83–176).
    pub(crate) fn build(self, dark: bool) -> El {
        if !self.state.is_configured {
            return self.unconfigured(dark);
        }
        self.configured(dark)
    }

    /// The `!isConfigured` early return: `border-t border-edge
    /// bg-surface-alt px-4 py-3`, one row — the muted stethoscope notice
    /// with the inline configure button, and the close control.
    fn unconfigured(self, dark: bool) -> El {
        let k = kit(dark);

        // The configure button: React's `h-5 gap-1 px-1.5 text-[10px]`
        // primary button with the 10pt settings glyph. The kit has no
        // 10pt row, so the button renders at its smallest control size —
        // the mapping the shell's `h-7` → small precedent follows.
        let mut settings = Button::new(t("settings.ai.goToConfigure").to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_SETTINGS)
            .help(t("settings.ai.goToConfigure"))
            .build(&k)
            .key("diagnosis-panel-unconfigured-settings");
        if let Some(open) = &self.on_open_settings {
            let open = Rc::clone(open);
            settings = settings.on_press(move |s: &mut S| open(s));
        }

        // React's notice row: `flex items-center gap-2 text-xs
        // text-fg-muted` — stethoscope, the line, the button.
        let notice = row()
            .key("diagnosis-panel-unconfigured-row")
            .items_center()
            .gap(8.0)
            .child(
                rune::icon(&ICON_STETHOSCOPE, 14.0)
                    .key("diagnosis-panel-unconfigured-icon")
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .child(
                text(t("common.aiNotConfigured"))
                    .key("diagnosis-panel-unconfigured-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(settings);

        col()
            .key("diagnosis-panel-unconfigured")
            .w_full()
            .items_stretch()
            // React: `border-t border-edge bg-surface-alt px-4 py-3`.
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .px(16.0)
            .py(12.0)
            .child(
                row()
                    .key("diagnosis-panel-unconfigured-bar")
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(notice)
                    .child(self.close_control(&k, "diagnosis-panel-unconfigured-close")),
            )
    }

    /// The configured panel: the accent-tinted root, the stethoscope
    /// header with the manual diagnose button and the close control, and
    /// the scrolling body with the four states.
    fn configured(self, dark: bool) -> El {
        let k = kit(dark);

        // The header's left: `flex items-center gap-2` — the accent
        // stethoscope and the medium title.
        let title = row()
            .key("diagnosis-panel-title")
            .items_center()
            .gap(8.0)
            .child(
                rune::icon(&ICON_STETHOSCOPE, 14.0)
                    .key("diagnosis-panel-title-icon")
                    .color(k.c.text_accent)
                    .a11y_hidden(),
            )
            .child(
                text(t("diagnosis.title"))
                    .key("diagnosis-panel-title-text")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base),
            );

        // The header's right: `flex items-center gap-1.5` — the manual
        // diagnose button (only when idle: `!diagnosis && !isDiagnosing`)
        // and the close control.
        let mut actions = row()
            .key("diagnosis-panel-header-actions")
            .items_center()
            .gap(6.0);
        if self.state.diagnosis.is_none() && !self.state.is_diagnosing {
            actions = actions.child(self.diagnose_button(&k));
        }
        actions = actions.child(self.close_control(&k, "diagnosis-panel-close"));

        let header = row()
            .key("diagnosis-panel-header")
            .w_full()
            .items_center()
            .justify_between()
            // React: `border-b border-edge px-3 py-1.5`.
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(12.0)
            .py(6.0)
            .child(title)
            .child(actions);

        col()
            .key("diagnosis-panel")
            .w_full()
            .items_stretch()
            // React: `border-t border-accent/20 bg-accent/5`.
            .border_sides([1.0, 0.0, 0.0, 0.0], k.alpha(k.c.bg_accent, 0.2))
            .fill(k.alpha(k.c.bg_accent, 0.05))
            .child(header)
            .child(self.body(&k))
    }

    /// The manual diagnose button: React's `h-6 gap-1 px-2 text-[11px]`
    /// primary button with the 12pt stethoscope. The press is
    /// `handleDiagnose` — an invalid context renders it inert (the gate
    /// refuses before the host action runs).
    fn diagnose_button(&self, k: &rune_kit::Kit) -> El {
        let mut button = Button::new(t("diagnosis.diagnose").to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_STETHOSCOPE)
            .help(t("diagnosis.diagnose"))
            .build(k)
            .key("diagnosis-panel-diagnose");
        if let (Some(on_diagnose), Some(request)) =
            (&self.on_diagnose, diagnose_request(self.context))
        {
            let on_diagnose = Rc::clone(on_diagnose);
            button = button.on_press(move |s: &mut S| on_diagnose(s, request.clone()));
        }
        button
    }

    /// The close control: React's bare `X` button (`text-fg-muted
    /// hover:text-fg`). The press is `handleClose` — `clearDiagnosis` then
    /// `onClose`, both branches' identical compound.
    fn close_control(&self, k: &rune_kit::Kit, key: &'static str) -> El {
        let mut button = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(t("common.close"))
            .build(k)
            .key(key);
        let clear = self.clear_diagnosis.clone();
        let close = self.on_close.clone();
        button = button.on_press(move |s: &mut S| {
            if let Some(clear) = &clear {
                clear(s);
            }
            if let Some(close) = &close {
                close(s);
            }
        });
        button
    }

    /// The body: React's `max-h-[200px] overflow-auto px-3 py-2` with the
    /// four mutually exclusive states — diagnosing, the error card, the
    /// result, and the idle prompt (the fourth renders only when the
    /// other three are all clear).
    fn body(&self, k: &rune_kit::Kit) -> El {
        let mut body = col()
            .key("diagnosis-panel-body")
            .w_full()
            .max_h(BODY_MAX_HEIGHT)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .px(12.0)
            .py(8.0)
            .items_stretch();

        // `isDiagnosing`: the spinner row.
        if self.state.is_diagnosing {
            body = body.child(
                row()
                    .key("diagnosis-panel-diagnosing")
                    .items_center()
                    .gap(8.0)
                    .py(12.0)
                    .child(
                        Spinner::new("diagnosis-panel-diagnosing-spinner")
                            .size(SpinnerSize::Lg)
                            .tone(SpinnerTone::Muted)
                            .label(t("diagnosis.diagnosing"))
                            .build(k.dark),
                    )
                    .child(
                        text(t("diagnosis.diagnosing"))
                            .key("diagnosis-panel-diagnosing-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
        }

        // `diagnosisError`: the red card.
        if let Some(error) = &self.state.diagnosis_error {
            body = body.child(
                text(error.clone())
                    .key("diagnosis-panel-error")
                    .w_full()
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap()
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.sm)
                    .border(1.0, k.alpha(k.c.danger.fg, 0.2))
                    .fill(k.alpha(k.c.danger.fg, 0.1)),
            );
        }

        // `diagnosis`: the three sections.
        if let Some(diagnosis) = &self.state.diagnosis {
            body = body.child(self.result(k, diagnosis));
        }

        // The idle prompt: `!diagnosis && !isDiagnosing && !diagnosisError`.
        if self.state.diagnosis.is_none()
            && !self.state.is_diagnosing
            && self.state.diagnosis_error.is_none()
        {
            body = body.child(
                text(t("diagnosis.diagnose"))
                    .key("diagnosis-panel-idle")
                    .py(8.0)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }
        body
    }

    /// The result's three sections (`space-y-2`): the explanation, the
    /// changes list (only when non-empty), and the suggested SQL with the
    /// apply button (only when a suggestion exists).
    fn result(&self, k: &rune_kit::Kit, diagnosis: &DiagnosisResult) -> El {
        let mut sections = col()
            .key("diagnosis-panel-result")
            .w_full()
            .items_stretch()
            .gap(8.0);

        // The explanation.
        sections = sections
            .child(section_label(
                k,
                "diagnosis-panel-explanation-label",
                t("diagnosis.explanation"),
            ))
            .child(
                text(diagnosis.explanation.clone())
                    .key("diagnosis-panel-explanation-text")
                    .w_full()
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            );

        // The changes (React: `diagnosis.changes.length > 0`).
        if !diagnosis.changes.is_empty() {
            let mut list = col()
                .key("diagnosis-panel-changes-list")
                .w_full()
                .items_stretch()
                .pl(12.0);
            for (index, change) in diagnosis.changes.iter().enumerate() {
                list = list.child(
                    text(change.clone())
                        .key(format!("diagnosis-panel-change-{index}"))
                        .w_full()
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .wrap(),
                );
            }
            sections = sections
                .child(section_label(
                    k,
                    "diagnosis-panel-changes-label",
                    t("diagnosis.changes"),
                ))
                .child(list);
        }

        // The suggested SQL and the apply button.
        if let Some(sql) = &diagnosis.suggested_sql {
            sections = sections
                .child(section_label(
                    k,
                    "diagnosis-panel-suggested-label",
                    t("diagnosis.suggestedSql"),
                ))
                .child(
                    text(sql.clone())
                        .key("diagnosis-panel-suggested-sql")
                        .w_full()
                        .px(8.0)
                        .py(8.0)
                        .radius(k.radius.sm)
                        .font(k.ty.mono)
                        .color(k.c.success.fg)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.bg_base)
                        .wrap(),
                )
                .child(
                    row()
                        .key("diagnosis-panel-apply-row")
                        .mt(6.0)
                        .child(self.apply_button(k, sql)),
                );
        }
        sections
    }

    /// The apply button: React's `h-6 gap-1 px-2 text-[11px]` primary
    /// button with the 12pt arrow glyph. The press is `handleApply` —
    /// `onApplySql(suggestedSql)`, `clearDiagnosis()`, `onClose()`, the
    /// compound React always runs when the suggestion exists.
    fn apply_button(&self, k: &rune_kit::Kit, sql: &str) -> El {
        let sql = sql.to_string();
        let mut button = Button::new(t("diagnosis.applySuggested").to_string())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_ARROW_DOWN_TO_LINE)
            .help(t("diagnosis.applySuggested"))
            .build(k)
            .key("diagnosis-panel-apply");
        let apply = self.on_apply_sql.clone();
        let clear = self.clear_diagnosis.clone();
        let close = self.on_close.clone();
        button = button.on_press(move |s: &mut S| {
            if let Some(apply) = &apply {
                apply(s, &sql);
            }
            if let Some(clear) = &clear {
                clear(s);
            }
            if let Some(close) = &close {
                close(s);
            }
        });
        button
    }
}

/// One section label: React's `mb-1 text-[11px] font-medium uppercase
/// tracking-wider text-fg-muted` heading line. Rune text has no
/// text-transform, so the uppercase is baked in (`display_label`'s
/// precedent in `ai_code_block_state.rs`).
fn section_label(k: &rune_kit::Kit, key: &'static str, label: &str) -> El {
    text(label.to_uppercase())
        .key(key)
        .w_full()
        .mb(4.0)
        .font(k.ty.small_medium)
        .color(k.c.text_muted)
}
