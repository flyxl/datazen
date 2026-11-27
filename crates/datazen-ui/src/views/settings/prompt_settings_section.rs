//! Port of `packages/frontend/src/windows/settings/PromptSettingsSection.tsx`
//! (T369) — Settings → AI → 提示词.
//!
//! The AI prompt template editor: the driver filter (all drivers plus
//! the DB_REGISTRY members — a host seam, the `driver_settings_section`
//! ruling: the generated artifact stays host-supplied), the scenario
//! cards (the label + the variables hint + the source badge default/
//! driver/user, the 260-char preview, the edit form with its mono
//! template field), the override save, and the user-source reset with
//! its confirm dialog.
//!
//! ## What maps where
//!
//! - `aiCommands.promptList / promptSetOverride / promptRemoveOverride`
//!   are host action slots; the requests are the `commands::ai::wire`
//!   shapers ([`prompt_list_request`] and friends, T278's port).
//! - `useConfirmDialog` is the [`ConfirmDialogState`] /
//!   [`ConfirmDialogHost`] pair (the hook's own port): the reset's
//!   warning confirm gates the removal.
//! - The mount/`[driverType]` effect is
//!   [`PromptSettingsSectionState::load_due`]: the host re-calls
//!   [`Self::begin_load`] when the driver filter changes and delivers
//!   through [`Self::deliver_prompts`] (which closes any open edit —
//!   React's `setEditingIdx(null)` in `loadPrompts`).
//! - `handleEdit` / `handleSave` / `handleReset` are the state's
//!   `begin_edit` / `begin_save` (the entry build + the 2s feedback
//!   window) / `begin_reset` (the confirm gate) plans.
//! - `SCENARIO_VARIABLES` is the table, verbatim (the variables hint's
//!   `{{var}}` list).
//! - The 2s feedback (`setTimeout(() => setFeedback(''), 2000)`) is
//!   the `feedback_until` deadline.
//! - `SectionTitle` / `SettingRow` are the `settingsUi.tsx` port; the
//!   driver filter is the kit [`Select`] (the open state in
//!   [`PromptSettingsSectionState::open_driver_select`], the kit's
//!   one-open rule); React `Button` variants map per T022
//!   (`secondary` → `Outline`, `primary` → `Submit`).
//!
//! ## Keys
//!
//! Derived from the section base: the root `prompt-settings-section`,
//! the driver row `prompt-settings-driver` (the select's trigger
//! `-trigger` per the kit), the feedback line `prompt-settings-feedback`,
//! one card `prompt-card-{scenario}` (label `prompt-card-{scenario}-label`,
//! badge `prompt-card-{scenario}-source`, edit `prompt-card-{scenario}-edit`,
//! reset `prompt-card-{scenario}-reset`, preview `prompt-card-{scenario}-preview`,
//! the form `prompt-card-{scenario}-form` with the field
//! `prompt-edit-system`, save `prompt-edit-save`, cancel
//! `prompt-edit-cancel`), and the confirm dialog the
//! [`ConfirmDialogHost`]'s own keys.
//!
//! Multi-line Rune input preserves the textarea content and selection; the
//! browser's drag resize handle is not part of this port.

#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, text_input, El, ScrollSpec, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, State};

use crate::commands::ai::wire::{
    PromptInfo, PromptOverrideEntry, PromptScenario, PromptSource, prompt_list_request,
    prompt_remove_override_request, prompt_set_override_request,
};
use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::ui::confirm_dialog::ConfirmKind;
use crate::i18n::t;
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::views::settings::setting_hint::SettingHint;
use crate::views::settings::settings_ui::{section_title, setting_row};

/// `SCENARIO_VARIABLES`, verbatim (the `{{var}}` names each scenario's
/// template may reference; the hint lists them).
pub fn scenario_variables(scenario: PromptScenario) -> &'static [&'static str] {
    use PromptScenario as P;
    match scenario {
        P::Nl2sql => &["db_type", "version", "schema", "recent"],
        P::Diagnose => &["db_type", "schema"],
        P::NlFilter => &["db_type", "columns"],
        P::SchemaDocSelectTables => &["db_type", "table_names"],
        P::SchemaDoc => &["db_type", "schema"],
        P::ConnectionDiagnose => &[],
        P::QuerySummary => &[],
        P::ExplainAnalysis => &["db_type"],
        P::Chat => &[],
        P::WorkflowGenerate => &["db_type", "schema", "connections"],
    }
}

/// The preview's cut (`p.system.slice(0, 260)` plus the ellipsis).
pub const PROMPT_PREVIEW_CUT: usize = 260;
/// The feedback window (`setTimeout(…, 2000)`), in seconds.
pub const PROMPT_FEEDBACK_WINDOW: f64 = 2.0;
/// The all-drivers sentinel (the select's `*` value).
pub const PROMPT_ALL_DRIVERS: &str = "*";

/// The section's state.
#[derive(Clone, Debug, Default)]
pub struct PromptSettingsSectionState {
    /// `driverType` (the `*` sentinel reads as "all").
    pub driver_type: String,
    /// `prompts` (the loaded list).
    pub prompts: Vec<PromptInfo>,
    /// Native host identifies exact resettable scopes. The standalone
    /// legacy-shaped component accepts its original source-only default.
    pub(super) resettable: Option<Vec<PromptScenario>>,
    /// `editingIdx`.
    pub editing_idx: Option<usize>,
    /// `editSystem`.
    pub edit_system: TextEdit,
    /// `feedback` — the window's deadline (the saved / resetDone line).
    pub feedback_until: Option<f64>,
    /// The feedback line's KEY (the render translates; a locale switch
    /// mid-window cannot show the wrong language).
    pub feedback_key: Option<&'static str>,
    /// The driver select's open flag (the kit's one-open rule).
    pub open_driver_select: bool,
    /// The reset confirm's dialog state.
    pub confirm: ConfirmDialogState,
    /// The reset awaiting the confirm's answer (the request
    /// `begin_reset` armed).
    pub pending_reset: Option<crate::commands::ai::AiRequest>,
}

impl PromptSettingsSectionState {
    /// React's start: the all-drivers sentinel.
    pub fn new() -> Self {
        Self { driver_type: PROMPT_ALL_DRIVERS.to_owned(), ..Default::default() }
    }

    /// `loadPrompts`'s request: `promptList(dt)` with the `*` sentinel
    /// as `None`.
    pub fn list_request(&self) -> crate::commands::ai::AiRequest {
        let dt = if self.driver_type == PROMPT_ALL_DRIVERS { None } else { Some(self.driver_type.as_str()) };
        prompt_list_request(dt)
    }

    /// The load's answer: the list replaces and any open edit closes
    /// (`setEditingIdx(null)`).
    pub fn deliver_prompts(&mut self, prompts: Vec<PromptInfo>) {
        self.prompts = prompts;
        self.editing_idx = None;
    }

    /// The driver filter changed (`setDriverType`): the reload's
    /// trigger is the host's (React's `useEffect [loadPrompts]`).
    pub fn set_driver_type(&mut self, value: &str) {
        self.driver_type = value.to_owned();
    }

    /// `handleEdit(idx)`: open the form seeded with the row's system.
    pub fn begin_edit(&mut self, idx: usize) {
        if let Some(prompt) = self.prompts.get(idx) {
            self.editing_idx = Some(idx);
            self.edit_system = TextEdit::new(prompt.system.clone());
        }
    }

    /// The edit's cancel.
    pub fn cancel_edit(&mut self) {
        self.editing_idx = None;
    }

    /// `handleSave`'s entry build (`{ driverType, scenario, system }`)
    /// — the CURRENT driver filter rides (the `*` sentinel verbatim,
    /// React sends it as-is) — plus the 2s feedback window.
    pub fn begin_save(&mut self, now: f64) -> Option<(PromptOverrideEntry, crate::commands::ai::AiRequest)> {
        let idx = self.editing_idx?;
        let prompt = self.prompts.get(idx)?;
        let entry = PromptOverrideEntry {
            driver_type: self.driver_type.clone(),
            scenario: prompt.scenario,
            system: self.edit_system.text.clone(),
        };
        let request = prompt_set_override_request(&entry);
        self.feedback_key = Some("settings.prompts.saved");
        self.feedback_until = Some(now + PROMPT_FEEDBACK_WINDOW);
        self.editing_idx = None;
        Some((entry, request))
    }

    pub(super) fn can_reset(&self, scenario: PromptScenario) -> bool {
        self.resettable.as_ref().map_or_else(
            || self.prompts.iter().any(|p| p.scenario == scenario && p.source == PromptSource::User),
            |scoped| scoped.contains(&scenario),
        )
    }

    /// `handleReset(p)`: the warning confirm's gate — `None` refuses
    /// (the dialog opened), `Some(request)` proceeds.
    pub fn begin_reset(&mut self, idx: usize) -> Option<crate::commands::ai::AiRequest> {
        let prompt = self.prompts.get(idx)?;
        // Only a user source may reset (the button's own rule; the
        // guard here keeps the plan total).
        if !self.can_reset(prompt.scenario) {
            return None;
        }
        let request = prompt_remove_override_request(&self.driver_type, prompt.scenario);
        let _ = self.confirm.open(ConfirmOptionsVm {
            title: t("settings.prompts.reset").to_owned(),
            message: t("settings.prompts.resetConfirm").to_owned(),
            confirm_label: None,
            cancel_label: None,
            kind: ConfirmKind::Warning,
            badge: None,
            code_preview: None,
            description: None,
        });
        self.pending_reset = Some(request);
        None
    }

    /// The confirm answered: `true` proceeds with the removal (the
    /// resetDone feedback arms), `false` cancels. Either way the
    /// dialog closes (the `handleClose` both buttons run).
    pub fn resolve_reset(&mut self, ok: bool, now: f64) -> Option<crate::commands::ai::AiRequest> {
        let answered = self.confirm.answer_pending(ok);
        if answered != Some(ok) {
            // A stale answer (a superseded request): nothing.
            return None;
        }
        if !ok {
            self.pending_reset = None;
            return None;
        }
        let request = self.pending_reset.take()?;
        self.feedback_key = Some("settings.prompts.resetDone");
        self.feedback_until = Some(now + PROMPT_FEEDBACK_WINDOW);
        Some(request)
    }

    /// Whether the feedback line shows.
    pub fn feedback(&self, now: f64) -> Option<&'static str> {
        if self.feedback_until.is_some_and(|until| now < until) {
            self.feedback_key
        } else {
            None
        }
    }
}

/// `PromptSettingsSection()`: the section over the state, the host's
/// driver registry, and the action slots.
pub struct PromptSettingsSection<'a, S: 'static> {
    state: &'a PromptSettingsSectionState,
    dark: bool,
    now: f64,
    /// The DB_REGISTRY entries (the host seam: `(value, label)` pairs;
    /// the all-drivers option rides first).
    driver_options: &'a [(String, String)],
    // the edit field's lens
    edit_system_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    // host actions
    on_set_driver_type: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_driver_select: Option<Rc<dyn Fn(&mut S)>>,
    on_close_driver_select: Option<Rc<dyn Fn(&mut S)>>,
    on_begin_edit: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_save: Option<Rc<dyn Fn(&mut S)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
    busy: bool,
    inline_reset_confirmation: bool,
    on_begin_reset: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_reset_resolved: Option<Rc<dyn Fn(&mut S, bool)>>,
}

impl<'a, S: 'static> PromptSettingsSection<'a, S> {
    /// The section over the state, the clock, the driver options and
    /// the edit lens.
    pub fn new(
        state: &'a PromptSettingsSectionState,
        dark: bool,
        now: f64,
        driver_options: &'a [(String, String)],
        edit_system_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            now,
            driver_options,
            edit_system_edit: Rc::new(edit_system_edit),
            on_set_driver_type: None,
            on_toggle_driver_select: None,
            on_close_driver_select: None,
            on_begin_edit: None,
            on_save: None,
            on_cancel: None,
            busy: false,
            inline_reset_confirmation: true,
            on_begin_reset: None,
            on_reset_resolved: None,
        }
    }

    /// The driver filter's choose (`setDriverType`).
    pub fn on_set_driver_type(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_driver_type = Some(Rc::new(f));
        self
    }
    /// The driver select's open toggle.
    pub fn on_toggle_driver_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_driver_select = Some(Rc::new(f));
        self
    }
    /// The driver select's close.
    pub fn on_close_driver_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_driver_select = Some(Rc::new(f));
        self
    }
    /// One card's edit button.
    pub fn on_begin_edit(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_begin_edit = Some(Rc::new(f));
        self
    }
    /// The edit form's save.
    pub fn on_save(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }
    pub(super) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    pub(super) fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// The real Settings page uses the window-level modal route. Keep the
    /// standalone port's inline host for its other callers.
    pub(super) fn without_inline_reset_confirmation(mut self) -> Self {
        self.inline_reset_confirmation = false;
        self
    }

    /// One card's reset button (opens the confirm).
    pub fn on_begin_reset(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_begin_reset = Some(Rc::new(f));
        self
    }
    /// The confirm dialog's answer.
    pub fn on_reset_resolved(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_reset_resolved = Some(Rc::new(f));
        self
    }

    /// The section body (the confirm dialog lands in `layer`).
    pub fn build(self, layer: &mut Vec<El>) -> El {
        let dark = self.dark;
        let k = kit(dark);
        let state = self.state;

        let mut body = col().key("prompt-settings-section").w_full().gap(20.0);

        body = body.child(section_title(
            &k,
            layer,
            "prompt-settings-title".to_owned(),
            t("settings.prompts"),
            Some(&format!("{}\n\n{}", t("settings.prompts.description"), t("settings.prompts.langNotice"))),
        ));

        // The driver filter.
        let mut labels = vec![t("settings.prompts.allDrivers").to_owned()];
        let mut values = vec![PROMPT_ALL_DRIVERS.to_owned()];
        for (value, label) in self.driver_options {
            labels.push(label.clone());
            values.push(value.clone());
        }
        let current = self
            .driver_options
            .iter()
            .find(|(value, _)| *value == state.driver_type)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| t("settings.prompts.allDrivers").to_owned());
        let mut select = Select::new("prompt-settings-driver", &labels)
            .values(&values)
            .current(Some(&current))
            .label(t("settings.prompts.driver"))
            .disabled(self.busy)
            .open(state.open_driver_select);
        if let Some(f) = &self.on_toggle_driver_select {
            let f = Rc::clone(f);
            select = select.on_toggle(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_close_driver_select {
            let f = Rc::clone(f);
            select = select.on_close(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_set_driver_type {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, value.to_owned());
            });
        }
        let select_el = select.build(&k, layer).h(36.0);
        body = body.child(setting_row(
            &k,
            layer,
            "prompt-settings-driver-row".to_owned(),
            t("settings.prompts.driver"),
            None,
            select_el,
        ));

        // The feedback line.
        if let Some(key) = state.feedback(self.now) {
            body = body.child(
                text(t(key)).key("prompt-settings-feedback").font(TextStyle::new(12.0, 400.0)).color(k.c.success.fg),
            );
        }

        // The scenario cards.
        let mut list = col().key("prompt-settings-list").w_full().gap(8.0);
        for (idx, prompt) in state.prompts.iter().enumerate() {
            list = list.child(self.card(&k, layer, dark, idx, prompt));
        }
        body = body.child(list);

        if self.inline_reset_confirmation {
            let mut dialog = ConfirmDialogHost::<S>::new(&state.confirm);
            if let Some(f) = &self.on_reset_resolved {
                let f = Rc::clone(f);
                dialog = dialog.on_resolved(move |s: &mut S, ok| f(s, ok));
            }
            dialog.build(dark, layer);
        }

        body
    }

    /// One scenario card.
    fn card(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        dark: bool,
        idx: usize,
        prompt: &PromptInfo,
    ) -> El {
        let state = self.state;
        let scenario = scenario_key(prompt.scenario);
        let is_editing = state.editing_idx == Some(idx);
        let vars = scenario_variables(prompt.scenario);

        let mut card = col()
            .key(format!("prompt-card-{scenario}"))
            .w_full()
            .padding(12.0)
            .gap(8.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);

        // The head: label + variables hint + the source badge; the
        // actions right.
        let mut head = row().key(format!("prompt-card-{scenario}-head")).w_full().gap(8.0).center_y().justify_between();
        let mut meta = row().gap(8.0).center_y()
            .child(
                row().gap(4.0).center_y()
                    .child(text(prompt.label.clone()).font(TextStyle { weight: 500.0, ..TextStyle::new(14.0, 400.0) }).color(k.c.text_base)),
            );
        if !vars.is_empty() {
            let list = vars.iter().map(|v| format!("{{{{{v}}}}}")).collect::<Vec<_>>().join(", ");
            meta = meta.child(
                SettingHint::new(
                    format!("prompt-card-{scenario}-vars"),
                    prompt.label.clone(),
                    format!("{}: {}", t("settings.prompts.variables"), list),
                )
                .build(k, layer),
            );
        }
        meta = meta.child(
            Badge::new(format!("prompt-card-{scenario}-source"), source_label(prompt.source).to_owned())
                .tone(source_tone(prompt.source))
                .build(dark),
        );
        head = head.child(meta);

        let mut actions = row().gap(4.0).center_y();
        if !is_editing {
            let mut edit = Button::new(t("settings.prompts.edit"))
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::ExtraLarge)
                .disabled(self.busy);
            if let Some(f) = &self.on_begin_edit {
                let f = Rc::clone(f);
                edit = edit.on_press(move |s: &mut S| f(s, idx));
            }
            actions = actions.child(edit.build(k).key(format!("prompt-card-{scenario}-edit")));
            if state.can_reset(prompt.scenario) {
                let mut reset = Button::new(t("settings.prompts.reset"))
                    .variant(ButtonVariant::Outline) // React secondary
                    .control_size(ControlSize::ExtraLarge)
                    .disabled(self.busy);
                if let Some(f) = &self.on_begin_reset {
                    let f = Rc::clone(f);
                    reset = reset.on_press(move |s: &mut S| f(s, idx));
                }
                actions = actions.child(reset.build(k).key(format!("prompt-card-{scenario}-reset")));
            }
        }
        head = head.child(actions);
        card = card.child(head);

        // The body: the edit form, or the 260-char preview.
        if is_editing {
            let mut field = text_input(&state.edit_system, TextStyle { size: 12.0, ..k.ty.mono })
                .key("prompt-edit-system")
                .label(t("settings.prompts.template"))
                .w_full()
                .min_h(120.0)
                .px(12.0)
                .py(8.0)
                .radius(k.radius.md)
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .focus_ring(2.0, 0.0, k.c.border_focus)
                .color(k.c.text_base);
            if !self.busy {
                let lens = Rc::clone(&self.edit_system_edit);
                let line_break_lens = Rc::clone(&self.edit_system_edit);
                field = field
                    .on_edit::<S>(move |s: &mut S| lens(s))
                    .on_submit::<S>(move |s: &mut S, _| { line_break_lens(s).insert("\n"); });
            }
            let field = rune::auto_grow(field, 8, 8).key("prompt-edit-system-scroll");
            let mut save = Button::new(t("common.save"))
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::ExtraLarge)
                .disabled(self.busy);
            if let Some(f) = &self.on_save {
                let f = Rc::clone(f);
                save = save.on_press(move |s: &mut S| f(s));
            }
            let mut cancel = Button::new(t("common.cancel"))
                .variant(ButtonVariant::Outline)
                .control_size(ControlSize::ExtraLarge)
                .disabled(self.busy);
            if let Some(f) = &self.on_cancel {
                let f = Rc::clone(f);
                cancel = cancel.on_press(move |s: &mut S| f(s));
            }
            card = card.child(
                col().key(format!("prompt-card-{scenario}-form")).w_full().gap(8.0)
                    .child(
                        col().gap(4.0)
                            .child(text(t("settings.prompts.template")).font(TextStyle::new(12.0, 400.0)).color(k.c.text_muted))
                            .child(field),
                    )
                    .child(
                        row().gap(8.0)
                            .child(save.build(k).key("prompt-edit-save"))
                            .child(cancel.build(k).key("prompt-edit-cancel")),
                    ),
            );
        } else {
            let mut preview: String = prompt.system.chars().take(PROMPT_PREVIEW_CUT).collect();
            if prompt.system.chars().count() > PROMPT_PREVIEW_CUT {
                preview.push('…');
            }
            card = card.child(
                col()
                    .key(format!("prompt-card-{scenario}-preview-box"))
                    .w_full()
                    .max_h(96.0)
                    .padding(8.0)
                    .radius(k.radius.sm)
                    .fill(k.c.surface_weak)
                    .scroll(ScrollSpec { stick_bottom: false })
                    .child(
                        text(preview)
                            .key(format!("prompt-card-{scenario}-preview"))
                            .w_full()
                            .font(TextStyle { size: 11.0, line_height: 16.5, ..k.ty.mono })
                            .color(k.c.text_faint)
                            .wrap()
                            .selectable(true),
                    ),
            );
        }

        card
    }
}

/// The scenario's wire spelling (the card keys).
fn scenario_key(scenario: PromptScenario) -> &'static str {
    use PromptScenario as P;
    match scenario {
        P::Nl2sql => "nl2sql",
        P::Diagnose => "diagnose",
        P::NlFilter => "nl_filter",
        P::SchemaDocSelectTables => "schema_doc_select_tables",
        P::SchemaDoc => "schema_doc",
        P::ConnectionDiagnose => "connection_diagnose",
        P::QuerySummary => "query_summary",
        P::ExplainAnalysis => "explain_analysis",
        P::Chat => "chat",
        P::WorkflowGenerate => "workflow_generate",
    }
}

/// `sourceLabel`'s key.
fn source_label(source: PromptSource) -> &'static str {
    match source {
        PromptSource::Default => "settings.prompts.source.default",
        PromptSource::Driver => "settings.prompts.source.driver",
        PromptSource::User => "settings.prompts.source.user",
    }
}

/// `SOURCE_BADGE_CLASSES`'s tint trio.
fn source_tone(source: PromptSource) -> BadgeTone {
    match source {
        PromptSource::Default => BadgeTone::Neutral,
        PromptSource::Driver => BadgeTone::Accent,
        PromptSource::User => BadgeTone::Success,
    }
}
