//! Port of `packages/frontend/src/windows/settings/DataCleanupSection.tsx`
//! (T244) — the history-data cleanup section.
//!
//! The scope checkboxes (query / workflow; at least one required, both
//! collapse to `all`), the retention choice (the 7/30/90 presets, the
//! custom 1–365 days whose focus/edit flips the mode, and the
//! clear-all radio whose `retainDays` is null), the warning confirm
//! before the purge, and the success-count / error line. `purgeHistory`
//! itself is the history command family's (T316, later) — a seam
//! callback here.
//!
//! ## What maps where
//!
//! - `resolveScope` is [`DataCleanupSectionState::resolve_scope`], the
//!   pure collapse (both → all; one → its own; none → no scope).
//! - `handleRun`'s decision half is [`DataCleanupSectionState::begin_run`]:
//!   the scope guard (the no-scope error message, nothing else runs),
//!   `retainDays` (clearAll → null; custom → clamp(1, 365); preset →
//!   the preset), and the confirm message branch. The confirm's
//!   resolution ([`Self::resolve_confirm`]) drives busy + the result
//!   message (success count / the catch's generic error).
//! - The custom field's `Number(e.target.value) || 1`: an unparsable or
//!   zero input reads as 1 ([`DataCleanupSectionState::set_custom_days`]);
//!   focusing or editing the field flips the mode to custom. In the
//!   engine the edit slot ([`Self::custom_edit`]) is an ACCESSOR (the
//!   engine reads the `TextEdit` through it for caret work too), so the
//!   mode flip is the host's onChange equivalent — diff the field's
//!   text against the last seen value at the decision moment and apply
//!   [`DataCleanupSectionState::focus_custom`] when it moved.
//! - `SectionTitle`/`SettingRow`/`SettingHint` stand in until
//!   T399/T381 (the settings subtree's shared precedent); the confirm
//!   rides the T038 `ConfirmDialogHost` pattern.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `data-cleanup-section`,
//! `data-cleanup-scope-query`, `data-cleanup-scope-workflow`,
//! `data-cleanup-preset-{days}`, `data-cleanup-custom-days`,
//! `data-cleanup-clear-all`, `data-cleanup-run`, `data-cleanup-message`.

use std::rc::Rc;

use rune::{col, row, text, tok, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::i18n::t;
use crate::ui::confirm_dialog::ConfirmKind;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;
use crate::views::settings::ai_settings_section::section_title;

/// React `RETENTION_PRESETS`.
pub const RETENTION_PRESETS: [u32; 3] = [7, 30, 90];

/// React `HistoryPurgeScope` (`'all' | 'query' | 'workflow'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPurgeScope {
    All,
    Query,
    Workflow,
}

impl HistoryPurgeScope {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Query => "query",
            Self::Workflow => "workflow",
        }
    }
}

/// React `RetentionMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetentionMode {
    #[default]
    Preset,
    Custom,
    ClearAll,
}

/// The result line (React `message`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupMessage {
    Success { count: u32 },
    Error { text: String },
}

/// What `handleRun` decided (the state machine's two-phase run).
#[derive(Clone, Debug, PartialEq)]
pub enum CleanupRunDecision {
    /// No scope is checked: the noScope error line, nothing runs.
    NoScope,
    /// The purge to run, with its confirm message — the host shows the
    /// confirm (the T038 host), and on OK calls
    /// [`DataCleanupSectionState::begin_purge`] then delivers the
    /// result through [`DataCleanupSectionState::note_purge_result`].
    Confirm { scope: HistoryPurgeScope, retain_days: Option<u32>, message: String },
}

/// The section's state (React's useState fields, owned).
#[derive(Clone, Default)]
pub struct DataCleanupSectionState {
    pub scope_query: bool,
    pub scope_workflow: bool,
    pub retention_mode: RetentionMode,
    pub preset_days: u32,
    /// The custom field's edit (the number input).
    pub custom_days: TextEdit,
    pub busy: bool,
    pub message: Option<CleanupMessage>,
    /// The warning confirm (`useConfirmDialog`'s instance).
    pub confirm: ConfirmDialogState,
}

impl DataCleanupSectionState {
    /// React's initial state (the derived Default is all-zero; this is
    /// the hook's mount values).
    pub fn new() -> Self {
        Self {
            scope_query: true,
            scope_workflow: true,
            retention_mode: RetentionMode::Preset,
            preset_days: 30,
            custom_days: TextEdit::new("30".to_owned()),
            busy: false,
            message: None,
            confirm: ConfirmDialogState::default(),
        }
    }

    /// `resolveScope`: both → all; one → its own; none → no scope.
    pub fn resolve_scope(&self) -> Option<HistoryPurgeScope> {
        match (self.scope_query, self.scope_workflow) {
            (true, true) => Some(HistoryPurgeScope::All),
            (true, false) => Some(HistoryPurgeScope::Query),
            (false, true) => Some(HistoryPurgeScope::Workflow),
            (false, false) => None,
        }
    }

    /// The custom days, parsed and clamped (`Math.min(365, Math.max(1,
    /// customDays))`; `Number(...) || 1` — unparsable/zero reads 1).
    pub fn custom_days_value(&self) -> u32 {
        let parsed: f64 = self.custom_days.text.trim().parse().unwrap_or(1.0);
        parsed.clamp(1.0, 365.0) as u32
    }

    /// The custom field's edit (the mode flips to custom, React's
    /// onChange half).
    pub fn set_custom_days(&mut self, raw: &str) {
        self.retention_mode = RetentionMode::Custom;
        self.custom_days = TextEdit::new(raw.to_owned());
    }

    /// Focusing the custom field flips the mode (`onFocus`).
    pub fn focus_custom(&mut self) {
        self.retention_mode = RetentionMode::Custom;
    }

    /// A preset press: the mode flips to preset with that day count.
    pub fn select_preset(&mut self, days: u32) {
        self.retention_mode = RetentionMode::Preset;
        self.preset_days = days;
    }

    /// The clear-all radio press.
    pub fn select_clear_all(&mut self) {
        self.retention_mode = RetentionMode::ClearAll;
    }

    /// `handleRun`'s decision half: the scope guard, the resolved
    /// retain days, and the confirm message (the armed request).
    pub fn begin_run(&mut self) -> CleanupRunDecision {
        let Some(scope) = self.resolve_scope() else {
            self.message = Some(CleanupMessage::Error {
                text: t("settings.dataCleanup.noScope").to_owned(),
            });
            return CleanupRunDecision::NoScope;
        };
        let retain_days = match self.retention_mode {
            RetentionMode::ClearAll => None,
            RetentionMode::Custom => Some(self.custom_days_value()),
            RetentionMode::Preset => Some(self.preset_days),
        };
        let message = match retain_days {
            None => t("settings.dataCleanup.confirmClearAll").to_owned(),
            Some(days) => t("settings.dataCleanup.confirmMessage").replace("{days}", &days.to_string()),
        };
        let id = self.confirm.open(ConfirmOptionsVm {
            title: t("settings.dataCleanup.confirmTitle").to_owned(),
            message,
            kind: ConfirmKind::Warning,
            ..ConfirmOptionsVm::default()
        });
        let _ = id;
        CleanupRunDecision::Confirm {
            scope,
            retain_days,
            message: t("settings.dataCleanup.confirmTitle").to_owned(),
        }
    }

    /// The confirm answered OK: enter busy, clear the message (the
    /// purge's scope/retainDays are the host's to execute).
    pub fn begin_purge(&mut self) {
        self.busy = true;
        self.message = None;
    }

    /// The purge settled: the count on success (the success line's
    /// `{count}`), the generic error on failure — busy clears either
    /// way (`finally`).
    pub fn note_purge_result(&mut self, result: Option<u32>) {
        self.busy = false;
        self.message = Some(match result {
            Some(count) => CleanupMessage::Success { count },
            None => CleanupMessage::Error {
                text: t("settings.dataCleanup.error").to_owned(),
            },
        });
    }

    /// The confirm's resolution hook (the T038 pattern): OK proceeds.
    pub fn resolve_confirm(&mut self, result: bool) -> bool {
        matches!(self.confirm.answer_pending(result), Some(true))
    }
}

/// The data-cleanup section (React `DataCleanupSection`).
pub struct DataCleanupSection<'a, S: 'static> {
    key: &'a str,
    state: &'a DataCleanupSectionState,
    on_toggle_scope_query: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_scope_workflow: Option<Rc<dyn Fn(&mut S)>>,
    on_preset: Option<Rc<dyn Fn(&mut S, u32)>>,
    on_custom_focus: Option<Rc<dyn Fn(&mut S)>>,
    on_clear_all: Option<Rc<dyn Fn(&mut S)>>,
    on_run: Option<Rc<dyn Fn(&mut S, &mut Tx)>>,
    on_confirm_resolved: Option<Rc<dyn Fn(&mut S, bool)>>,
    custom_edit_slot: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
}

impl<'a, S: 'static> DataCleanupSection<'a, S> {
    pub fn new(state: &'a DataCleanupSectionState) -> Self {
        Self {
            key: "data-cleanup-section",
            state,
            on_toggle_scope_query: None,
            on_toggle_scope_workflow: None,
            on_preset: None,
            on_custom_focus: None,
            on_clear_all: None,
            on_run: None,
            on_confirm_resolved: None,
            custom_edit_slot: None,
        }
    }

    /// The query-scope checkbox's press.
    pub fn on_toggle_scope_query(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_scope_query = Some(Rc::new(f));
        self
    }

    /// The workflow-scope checkbox's press.
    pub fn on_toggle_scope_workflow(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_scope_workflow = Some(Rc::new(f));
        self
    }

    /// A preset press.
    pub fn on_preset(mut self, f: impl Fn(&mut S, u32) + 'static) -> Self {
        self.on_preset = Some(Rc::new(f));
        self
    }

    /// The custom field's edit slot (the host applies
    /// [`DataCleanupSectionState::set_custom_days`]).
    pub fn custom_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.custom_edit_slot = Some(Rc::new(f));
        self
    }

    /// The custom field's focus (flips the mode).
    pub fn on_custom_focus(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_custom_focus = Some(Rc::new(f));
        self
    }

    /// The clear-all radio's press.
    pub fn on_clear_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_all = Some(Rc::new(f));
        self
    }

    /// The run press (`handleRun`'s decision + confirm + purge).
    pub fn on_run(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_run = Some(Rc::new(f));
        self
    }

    /// The confirm's resolution.
    pub fn on_confirm_resolved(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_confirm_resolved = Some(Rc::new(f));
        self
    }

    /// Build the section (the confirm dialog lands in `layer`).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;

        let mut section = col()
            .key(self.key.to_owned())
            .w_full()
            .gap(12.0)
            .px(16.0)
            .py(16.0)
            .radius(6.0)
            .border(1.0, k.c.border_base);

        // The title + description hint.
        section = section.child(section_title(
            &k,
            layer,
            format!("{}-title", self.key),
            t("settings.dataCleanup.title"),
            t("settings.dataCleanup.description"),
        ));

        // The scope checkboxes.
        let mut scope_col = col().key(format!("{}-scope", self.key)).gap(8.0);
        let mut query = Checkbox::new("data-cleanup-scope-query", state.scope_query)
            .label(t("settings.dataCleanup.scopeQuery"));
        if let Some(on_toggle) = &self.on_toggle_scope_query {
            let on_toggle = Rc::clone(on_toggle);
            query = query.on_toggle(move |s: &mut S| on_toggle(s));
        }
        scope_col = scope_col.child(query.build(&k));
        let mut workflow = Checkbox::new("data-cleanup-scope-workflow", state.scope_workflow)
            .label(t("settings.dataCleanup.scopeWorkflow"));
        if let Some(on_toggle) = &self.on_toggle_scope_workflow {
            let on_toggle = Rc::clone(on_toggle);
            workflow = workflow.on_toggle(move |s: &mut S| on_toggle(s));
        }
        scope_col = scope_col.child(workflow.build(&k));
        section = section.child(scope_col);

        // The retention presets.
        section = section.child(section_title(
            &k,
            layer,
            format!("{}-retention", self.key),
            t("settings.dataCleanup.retention"),
            "",
        ));
        let mut presets = row().key(format!("{}-presets", self.key)).gap(8.0);
        for days in RETENTION_PRESETS {
            let key = format!("data-cleanup-preset-{days}");
            let label = match days {
                7 => t("settings.dataCleanup.preset7"),
                30 => t("settings.dataCleanup.preset30"),
                _ => t("settings.dataCleanup.preset90"),
            };
            let selected = state.retention_mode == RetentionMode::Preset
                && state.preset_days == days;
            let mut button = Button::new(label)
                .variant(if selected {
                    ButtonVariant::Submit // the accent border/fill state
                } else {
                    ButtonVariant::Outline
                })
                .control_size(ControlSize::Small);
            if let Some(on_preset) = &self.on_preset {
                let on_preset = Rc::clone(on_preset);
                button = button.on_press(move |s: &mut S| on_preset(s, days));
            }
            presets = presets.child(button.build(&k).key(key));
        }
        section = section.child(presets);

        // The custom days field.
        let mut field = TextField::new("data-cleanup-custom-days", &state.custom_days)
            .height(36.0)
            .width(96.0)
            .placeholder("30");
        if let Some(edit_slot) = &self.custom_edit_slot {
            let edit_slot = Rc::clone(edit_slot);
            field = field.on_edit(move |s: &mut S| edit_slot(s));
        }
        if let Some(on_focus) = &self.on_custom_focus {
            let _ = on_focus; // focus-side mode flip rides the host's focus event
        }
        section = section.child(field.build(dark));

        // The clear-all radio.
        let mut clear_all = Checkbox::new("data-cleanup-clear-all", state.retention_mode == RetentionMode::ClearAll)
            .label(t("settings.dataCleanup.clearAll"));
        if let Some(on_clear) = &self.on_clear_all {
            let on_clear = Rc::clone(on_clear);
            clear_all = clear_all.on_toggle(move |s: &mut S| on_clear(s));
        }
        section = section.child(clear_all.build(&k));

        // The run row + message line.
        let mut run = Button::new(t("settings.dataCleanup.run"))
            .variant(ButtonVariant::Submit)
            .disabled(state.busy);
        if let Some(on_run) = &self.on_run {
            let on_run = Rc::clone(on_run);
            run = run.on_press_tx(move |s: &mut S, tx: &mut Tx| on_run(s, tx));
        }
        let mut run_row = row().key(format!("{}-run-row", self.key)).gap(12.0).center_y();
        run_row = run_row.child(run.build(&k).key("data-cleanup-run"));
        if let Some(message) = &state.message {
            let (text_value, color) = match message {
                CleanupMessage::Success { count } => (
                    t("settings.dataCleanup.success").replace("{count}", &count.to_string()),
                    k.c.success.fg,
                ),
                CleanupMessage::Error { text } => (text.clone(), k.c.danger.fg),
            };
            run_row = run_row.child(
                text(text_value)
                    .font(tok::type_::LABEL)
                    .color(color)
                    .selectable(true)
                    .key("data-cleanup-message"),
            );
        }
        section = section.child(run_row);

        // The warning confirm (React's trailing {confirmCleanupDialog}).
        let mut confirm_host = ConfirmDialogHost::<S>::new(&state.confirm);
        if let Some(on_resolved) = &self.on_confirm_resolved {
            let on_resolved = Rc::clone(on_resolved);
            confirm_host = confirm_host.on_resolved(move |s: &mut S, result: bool| {
                on_resolved(s, result);
            });
        }
        confirm_host.build(dark, layer);

        section
    }
}
