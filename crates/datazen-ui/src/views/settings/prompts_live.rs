//! Real settings-route owner of the prompt editor. The host returns an
//! epoch-scoped snapshot only after a local operation completes.
use datazen_ui_api::{
    native_prompts::{NativePromptChange, NativePromptRequest, NativePromptView, PromptScenario},
    AdapterJob,
};
use rune::{col, text, El, TextEdit};

use super::prompt_settings_section::{PromptSettingsSection, PromptSettingsSectionState};
use crate::{hooks::confirm_dialog::ConfirmDialogHost, i18n::t, state_def::AppState, types::Section, ui::kit::kit};

const UNAVAILABLE: &str = "Local prompt settings are unavailable.";

#[derive(Default)]
pub(crate) struct LivePromptSettings {
    pub(super) editor: PromptSettingsSectionState,
    mounted: bool,
    epoch: u64,
    pending: Option<Pending>,
    clock: f64,
    error: Option<String>,
    reset_plan: Option<PromptScenario>,
}

enum Pending {
    List,
    Save,
    Reset,
}

impl AppState {
    pub(crate) fn tick_native_prompts(&mut self, now: f64) -> bool {
        let active = self.active == Section::Settings && self.settings_section() == "prompts";
        self.prompt_settings.clock = now;
        if active != self.prompt_settings.mounted {
            self.prompt_settings.mounted = active;
            // PromptSettingsSection remounts with the global filter and closes
            // stale dialogs/edits. Its serial never resets.
            self.prompt_settings.editor = PromptSettingsSectionState::new();
            self.prompt_settings.error = None;
            self.prompt_settings.reset_plan = None;
            self.prompt_settings.pending = None;
            if active {
                self.start_prompt_change(NativePromptChange::List);
            } else {
                self.prompt_settings.epoch = self.prompt_settings.epoch.saturating_add(1);
            }
            return true;
        }
        if self.prompt_settings.editor.feedback_until.is_some_and(|until| now >= until) {
            self.prompt_settings.editor.feedback_until = None;
            return true;
        }
        false
    }

    fn choose_prompt_driver(&mut self, driver: &str) {
        let ui = &mut self.prompt_settings;
        ui.editor.open_driver_select = false;
        if ui.pending.is_some() || ui.editor.driver_type == driver || ui.editor.confirm.is_open() {
            return;
        }
        ui.editor = PromptSettingsSectionState::new();
        ui.editor.set_driver_type(driver);
        self.start_prompt_change(NativePromptChange::List);
    }

    fn start_prompt_change(&mut self, change: NativePromptChange) {
        let ui = &mut self.prompt_settings;
        if !ui.mounted || ui.pending.is_some() {
            return;
        }
        let Some(epoch) = ui.epoch.checked_add(1) else {
            ui.error = Some(UNAVAILABLE.into());
            return;
        };
        ui.epoch = epoch;
        let pending = match &change {
            NativePromptChange::List => Pending::List,
            NativePromptChange::Save { .. } => Pending::Save,
            NativePromptChange::Reset { .. } => Pending::Reset,
        };
        let request = NativePromptRequest { driver: ui.editor.driver_type.clone(), change };
        ui.editor.feedback_until = None;
        ui.error = None;
        if self.desktop.begin_job(AdapterJob::NativePrompts { epoch, request }) {
            ui.pending = Some(pending);
        } else {
            ui.error = Some(UNAVAILABLE.into());
        }
    }

    pub(crate) fn receive_native_prompts(&mut self, epoch: u64, result: Result<NativePromptView, String>) {
        let ui = &mut self.prompt_settings;
        if !ui.mounted || ui.epoch != epoch {
            return;
        }
        let Some(pending) = ui.pending.take() else { return };
        match result {
            Ok(answer) => {
                ui.editor.deliver_prompts(answer.prompts);
                ui.editor.resettable = Some(answer.resettable);
                ui.editor.edit_system = TextEdit::new("");
                let key = match pending {
                    Pending::List => None,
                    Pending::Save => Some("settings.prompts.saved"),
                    Pending::Reset => Some("settings.prompts.resetDone"),
                };
                ui.editor.feedback_key = key;
                ui.editor.feedback_until = key.map(|_| ui.clock + super::prompt_settings_section::PROMPT_FEEDBACK_WINDOW);
            }
            Err(_) => ui.error = Some(UNAVAILABLE.into()),
        }
    }

    fn save_native_prompt(&mut self) {
        let editor = &self.prompt_settings.editor;
        let Some(prompt) = editor.editing_idx.and_then(|idx| editor.prompts.get(idx)) else { return };
        let change = NativePromptChange::Save { scenario: prompt.scenario, system: editor.edit_system.text.clone() };
        self.start_prompt_change(change);
    }

    fn ask_native_prompt_reset(&mut self, idx: usize) {
        let ui = &mut self.prompt_settings;
        if ui.pending.is_some() || ui.editor.confirm.is_open() {
            return;
        }
        let Some(scenario) = ui.editor.prompts.get(idx).map(|prompt| prompt.scenario) else { return };
        if !ui.editor.can_reset(scenario) {
            return;
        }
        ui.editor.begin_reset(idx);
        ui.reset_plan = Some(scenario);
    }

    pub(crate) fn retire_native_prompt_confirmation(&mut self) {
        let ui = &mut self.prompt_settings;
        ui.editor.confirm.pending = None;
        ui.editor.pending_reset = None;
        ui.reset_plan = None;
    }

    fn answer_native_prompt_reset(&mut self, epoch: u64, dialog: u64, ok: bool) {
        if self.active != Section::Settings || self.settings_section() != "prompts" {
            return;
        }
        let ui = &mut self.prompt_settings;
        if !ui.mounted || ui.epoch != epoch || ui.pending.is_some() || ui.reset_plan.is_none() {
            return;
        }
        if ui.editor.confirm.answer(dialog, ok) != Some(ok) {
            return;
        }
        ui.editor.pending_reset = None;
        let plan = ui.reset_plan.take();
        if let Some(scenario) = plan.filter(|_| ok) {
            self.start_prompt_change(NativePromptChange::Reset { scenario });
        }
    }
}

/// The window modal layer owns reset geometry and native Escape/focus handling.
pub(crate) fn mount_reset_confirmation(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let ui = &state.prompt_settings;
    if state.active != Section::Settings || state.settings_section() != "prompts"
        || !ui.mounted || ui.pending.is_some() || ui.reset_plan.is_none()
        || !ui.editor.confirm.is_open()
    {
        return;
    }
    let epoch = ui.epoch;
    let request = ui.editor.confirm.pending_id;
    ConfirmDialogHost::<AppState>::new(&ui.editor.confirm)
        .on_resolved(move |state, ok| state.answer_native_prompt_reset(epoch, request, ok))
        .build(dark, layer);
}

pub(super) fn view(state: &AppState, dark: bool) -> El {
    let ui = &state.prompt_settings;
    let mut options: Vec<_> = state.desktop.available_engines().into_iter()
        .map(|key| {
            let label = crate::views::engine_label(&key);
            (key, label)
        }).collect();
    let mut label_counts = std::collections::BTreeMap::new();
    for (_, label) in &options {
        *label_counts.entry(label.clone()).or_insert(0usize) += 1;
    }
    for (driver, label) in &mut options {
        // The kit returns the displayed label; keep any aliased driver names
        // unambiguous rather than silently choosing the first registry key.
        if label_counts.get(label).copied().unwrap_or(0) > 1
            || label.as_str() == t("settings.prompts.allDrivers")
        {
            *label = format!("{label} ({driver})");
        }
    }
    let expected = ui.epoch;
    let ready = move |state: &AppState| {
        state.active == Section::Settings && state.settings_section() == "prompts"
            && state.prompt_settings.epoch == expected && state.prompt_settings.pending.is_none()
    };
    let mut layer = Vec::new();
    let section = PromptSettingsSection::<AppState>::new(
        &ui.editor, dark, ui.clock, &options,
        |state| &mut state.prompt_settings.editor.edit_system,
    )
    .busy(ui.pending.is_some())
    .without_inline_reset_confirmation()
    .on_set_driver_type(move |state, value| {
        if ready(state) { state.choose_prompt_driver(&value); }
    })
    .on_toggle_driver_select(move |state| {
        if ready(state) {
            state.prompt_settings.editor.open_driver_select = !state.prompt_settings.editor.open_driver_select;
        }
    })
    .on_close_driver_select(move |state| {
        if ready(state) { state.prompt_settings.editor.open_driver_select = false; }
    })
    .on_begin_edit(move |state, idx| {
        if ready(state) {
            state.prompt_settings.error = None;
            state.prompt_settings.editor.begin_edit(idx);
        }
    })
    .on_cancel(move |state| {
        if ready(state) {
            state.prompt_settings.error = None;
            state.prompt_settings.editor.cancel_edit();
            state.prompt_settings.editor.edit_system = TextEdit::new("");
        }
    })
    .on_resize_draft(move |state, idx, height| {
        if ready(state) { state.prompt_settings.editor.resize_draft(idx, height); }
    })
    .on_save(move |state| {
        if ready(state) { state.save_native_prompt(); }
    })
    .on_begin_reset(move |state, idx| {
        if ready(state) { state.ask_native_prompt_reset(idx); }
    })
    .build(&mut layer);
    let k = kit(dark);
    let mut content = col().key("settings-section-prompts").w_full().gap(8.0);
    if let Some(error) = ui.error.as_ref() {
        content = content.child(text(error.clone()).key("native-prompts-error").color(k.c.danger.fg).w_full().wrap());
    }
    content = content.child(section);
    if matches!(ui.pending, Some(Pending::List)) {
        content = content.child(text(t("common.loading")).key("native-prompts-loading").color(k.c.text_faint));
    }
    content.children(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::native_prompts::{PromptInfo, PromptSource};

    fn reply(system: &str) -> NativePromptView {
        NativePromptView {
            prompts: vec![PromptInfo {
                scenario: PromptScenario::Nl2sql,
                label: "NL → SQL".into(),
                default_system: "baseline".into(),
                system: system.into(),
                source: PromptSource::User,
            }],
            resettable: vec![PromptScenario::Nl2sql],
        }
    }

    #[test]
    fn only_current_success_displays_feedback_and_unmount_drops_pending_delivery() {
        let mut state = AppState::default();
        state.active = Section::Settings;
        state.set_settings_section("prompts");
        let ui = &mut state.prompt_settings;
        ui.mounted = true;
        ui.pending = Some(Pending::Save);
        ui.epoch = 7;
        ui.clock = 2.0;
        ui.editor.edit_system = TextEdit::new("my draft");
        state.receive_native_prompts(6, Ok(reply("stale")));
        assert!(state.prompt_settings.editor.prompts.is_empty());
        state.receive_native_prompts(7, Err("private raw error".into()));
        assert_eq!(state.prompt_settings.error.as_deref(), Some(UNAVAILABLE));
        assert_eq!(state.prompt_settings.editor.edit_system.text, "my draft");
        assert_eq!(state.prompt_settings.editor.feedback(2.0), None);
        state.prompt_settings.pending = Some(Pending::Save);
        state.prompt_settings.epoch = 8;
        state.receive_native_prompts(8, Ok(reply("chosen")));
        assert_eq!(state.prompt_settings.editor.prompts[0].system, "chosen");
        assert_eq!(state.prompt_settings.editor.feedback(2.0), Some("settings.prompts.saved"));
        assert!(state.tick_native_prompts(4.1));
        assert_eq!(state.prompt_settings.editor.feedback_until, None);

        state.prompt_settings.pending = Some(Pending::List);
        state.set_settings_section("general");
        assert!(state.tick_native_prompts(5.0));
        state.receive_native_prompts(8, Ok(reply("stale after leaving")));
        assert!(state.prompt_settings.editor.prompts.is_empty());
    }
    #[test]
    fn window_prompt_reset_fits_and_only_clicked_ok_commits() {
        use datazen_ui_api::{ColumnFilter, ColumnSort, DesktopAdapter, TablePage};
        use rune::{Env, Key, KeyEvent, Mods, Runtime};
        use std::{cell::RefCell, rc::Rc};

        struct RecordingPort(Rc<RefCell<Vec<NativePromptRequest>>>);
        impl DesktopAdapter for RecordingPort {
            fn list_connections(&mut self) -> Result<Vec<datazen_ui_api::ConnectionProfile>, String> {
                Ok(Vec::new())
            }

            fn read_table_page(
                &mut self, _: &str, _: u32, _: u32, _: Option<&ColumnSort>, _: Option<&ColumnFilter>,
            ) -> Result<TablePage, String> {
                Err("not a table fixture".into())
            }

            fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
                Err("not a table fixture".into())
            }

            fn begin_job(&mut self, job: AdapterJob) -> bool {
                match job {
                    AdapterJob::NativePrompts { request, .. } => {
                        self.0.borrow_mut().push(request);
                        true
                    }
                    _ => false,
                }
            }
        }

        const ASK: &str = "prompt-card-nl2sql-reset";
        const CANCEL: &str = "confirm-dialog-cancel";
        const OK: &str = "confirm-dialog-ok";
        let mut state = AppState::default();
        state.active = Section::Settings;
        state.set_settings_section("prompts");
        let requests = Rc::new(RefCell::new(Vec::new()));
        state.desktop = Box::new(RecordingPort(requests.clone()));
        let ui = &mut state.prompt_settings;
        ui.mounted = true;
        ui.epoch = 12;
        let mut answer = reply("first fixture preview");
        for scenario in [
            PromptScenario::Diagnose, PromptScenario::NlFilter, PromptScenario::SchemaDoc,
            PromptScenario::SchemaDocSelectTables, PromptScenario::ConnectionDiagnose,
            PromptScenario::QuerySummary, PromptScenario::ExplainAnalysis, PromptScenario::Chat,
            PromptScenario::WorkflowGenerate,
        ] {
            answer.prompts.push(PromptInfo {
                scenario,
                label: "Additional fixture scenario".into(),
                source: PromptSource::Default,
                system: "one\ntwo\nthree\nfour\nfive\nsix\nseven".into(),
                default_system: String::new(),
            });
        }
        ui.editor = PromptSettingsSectionState::new();
        ui.editor.deliver_prompts(answer.prompts);
        ui.editor.resettable = Some(answer.resettable);
        let mut rt = Runtime::new(state, crate::app::view, (1440.0, 900.0), Env {
            system_dark: false, scale: 1.0,
        });
        let settle = |rt: &mut Runtime<AppState>| {
            for _ in 0..24 { rt.frame(1.0 / 60.0); }
        };
        let press = |rt: &mut Runtime<AppState>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
            assert!((0.0..1440.0).contains(&x) && (0.0..900.0).contains(&y), "{key} outside viewport");
            rt.pointer_move(x, y);
            rt.frame(1.0 / 60.0);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            settle(rt);
        };
        settle(&mut rt);
        press(&mut rt, ASK);
        let nodes = rt.elements();
        for key in [CANCEL, OK] {
            let node = nodes.iter().find(|node| node.key.as_deref() == Some(key) && !node.exiting).unwrap();
            assert!(!node.path.contains("settings-content"), "dialog must not ride the settings scroll");
            let [x, y, w, h] = node.visible;
            assert!(w > 0.0 && h > 0.0 && x >= 0.0 && y >= 0.0 && x + w <= 1440.0 && y + h <= 900.0);
        }
        rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        settle(&mut rt);
        assert!(requests.borrow().is_empty(), "no default Enter may reset");
        if rt.find(OK).is_none() {
            press(&mut rt, ASK);
        }
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(rt.find(OK).is_none() && requests.borrow().is_empty());
        press(&mut rt, ASK);
        press(&mut rt, CANCEL);
        assert!(rt.find(OK).is_none() && requests.borrow().is_empty());

        press(&mut rt, ASK);
        let retired = rt.state.prompt_settings.editor.confirm.pending_id;
        rt.state.set_settings_section("general");
        rt.state.answer_native_prompt_reset(12, retired, true);
        rt.state.set_settings_section("prompts");
        settle(&mut rt);
        assert!(rt.find(OK).is_none() && requests.borrow().is_empty());
        press(&mut rt, ASK);
        let current = rt.state.prompt_settings.editor.confirm.pending_id;
        rt.state.answer_native_prompt_reset(12, retired, true);
        rt.state.answer_native_prompt_reset(11, current, true);
        assert!(requests.borrow().is_empty());
        press(&mut rt, OK);
        rt.state.answer_native_prompt_reset(12, current, true);
        assert_eq!(requests.borrow().as_slice(), &[NativePromptRequest {
            driver: "*".into(),
            change: NativePromptChange::Reset { scenario: PromptScenario::Nl2sql },
        }]);
    }

}
