//! Behavior's confirmed cleanup of local workflow run history.
use chrono::{Duration, SecondsFormat, Utc};
use rune::{col, row, text, El, Key, KeyEvent, Mods, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::settings_ui::{section_title, setting_row};
use super::setting_hint::SettingHint;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::types::Section;
use crate::ui::{dialog::Dialog, field::TextField, kit::kit, toggle::Checkbox};

const PRESETS: [(u32, &str); 3] = [
    (7, "settings.dataCleanup.preset7"),
    (30, "settings.dataCleanup.preset30"),
    (90, "settings.dataCleanup.preset90"),
];
const LABEL: TextStyle = TextStyle::new(14.0, 400.0).lh(20.0);
const CAPTION: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);

mod choices;
use choices::{selection, step_custom_days, Invalid, Mode, Selection};

#[derive(Clone, Debug, PartialEq)]
enum Plan { Before(String), ClearAll }

#[derive(Clone, Copy, PartialEq)]
enum Feedback { Removed(usize), Cleared, NoScope, InvalidDays, Error }

#[derive(Clone, PartialEq)]
pub(crate) struct WorkflowRetention {
    selected: bool,
    mode: Mode,
    custom_days: TextEdit,
    epoch: u64,
    pending: Option<(u64, Plan)>,
    feedback: Option<Feedback>,
}

impl Default for WorkflowRetention {
    fn default() -> Self {
        Self {
            selected: true, mode: Mode::Preset(30), custom_days: TextEdit::new("30".to_owned()),
            epoch: 0, pending: None, feedback: None,
        }
    }
}

impl WorkflowRetention {
    pub(crate) fn retire(&mut self) {
        *self = Self { epoch: self.epoch, ..Self::default() };
    }
}

fn is_current(state: &AppState) -> bool {
    state.active == Section::Settings && state.settings_section() == "behavior"
        && state.desktop.supports_native_workflow_retention()
}

fn ask(state: &mut AppState) {
    if !is_current(state) {
        return;
    }
    let form = &mut state.settings_content.workflow_retention;
    if form.pending.is_some() { return; }
    let plan = match selection(form.selected, form.mode, &form.custom_days.text) {
        Ok(Selection::Finite(days)) => Plan::Before((Utc::now() - Duration::days(i64::from(days)))
            .to_rfc3339_opts(SecondsFormat::Secs, true)),
        Ok(Selection::ClearAll) => Plan::ClearAll,
        Err(error) => {
            form.feedback = Some(match error {
                Invalid::NoScope => Feedback::NoScope,
                Invalid::Days => Feedback::InvalidDays,
            });
            return;
        }
    };
    form.epoch = form.epoch.wrapping_add(1);
    form.pending = Some((form.epoch, plan));
    form.feedback = None;
}

fn answer(state: &mut AppState, epoch: u64, accepted: bool) {
    if !is_current(state) {
        return;
    }
    let form = &mut state.settings_content.workflow_retention;
    if form.pending.as_ref().map(|(id, _)| *id) != Some(epoch) {
        return;
    }
    let Some((_, plan)) = form.pending.take() else { return };
    if !accepted {
        return;
    }
    let result = match plan {
        Plan::Before(cutoff) => state.desktop.retain_native_workflow_history_before(&cutoff)
            .map(Feedback::Removed),
        // Same atomic, workflow-only host operation as the Workflow history
        // page. It reports completion but not a count; never invent one.
        Plan::ClearAll => state.desktop.clear_native_workflow_history().map(|()| Feedback::Cleared),
    };
    if result.is_ok() {
        state.load_workflow_history();
    }
    state.settings_content.workflow_retention.feedback = Some(result.unwrap_or(Feedback::Error));
}

impl AppState {
    /// Rune publishes textbox focus on the bound TextEdit. Follow it in the
    /// existing app poll so click and Tab both select this choice even when the
    /// user keeps the existing text; leaving the field does not revert the choice.
    pub(crate) fn tick_workflow_retention_focus(&mut self) -> bool {
        if !is_current(self) { return false; }
        let form = &mut self.settings_content.workflow_retention;
        if !form.custom_days.focused || form.mode == Mode::Custom || form.pending.is_some() {
            return false;
        }
        form.mode = Mode::Custom;
        form.feedback = None;
        true
    }
}

fn change_form(state: &mut AppState, update: impl FnOnce(&mut WorkflowRetention)) {
    if !is_current(state) { return; }
    let form = &mut state.settings_content.workflow_retention;
    if form.pending.is_none() {
        update(form);
        form.feedback = None;
    }
}

fn step_custom_on_key(state: &mut AppState, event: &KeyEvent) -> bool {
    if event.mods != Mods::default() || !is_current(state) {
        return false;
    }
    let increment = match event.key {
        Key::Up => true,
        Key::Down => false,
        _ => return false,
    };
    let form = &mut state.settings_content.workflow_retention;
    if form.pending.is_some() || form.custom_days.is_composing() {
        return false;
    }
    let Some(days) = step_custom_days(&form.custom_days.text, increment) else { return false };
    let draft = days.to_string();
    if form.custom_days.text != draft {
        // Use a normal undoable editor change, not a reset of the focused
        // TextEdit and its history. Inserting a value never runs cleanup.
        let len = form.custom_days.text.len();
        form.custom_days.set_selection(0, len);
        form.custom_days.insert(&draft);
    }
    form.mode = Mode::Custom;
    form.feedback = None;
    true
}

fn control(
    k: &Kit, key: &str, label: &str, variant: ButtonVariant, press: impl Fn(&mut AppState) + 'static,
) -> El {
    Button::new(label)
        .variant(variant)
        .control_size(ControlSize::Small)
        .on_press(press)
        .build(k)
        .key(key)
        .h(32.0)
}

pub(super) fn view(state: &AppState, dark: bool) -> El {
    let k = kit(dark);
    let mut hints = Vec::new();
    let title = section_title(k, &mut hints, "settings-behavior-title".into(), t("settings.behavior"), None);
    let body = col().key("settings-section-behavior").w_full().gap(20.0).child(title);
    if !state.desktop.supports_native_workflow_retention() {
        return body.child(
            text(t("settings.nativeRetention.unavailable")).font(LABEL).color(k.c.text_muted),
        );
    }

    let form = &state.settings_content.workflow_retention;
    let mut choices = row().w_full().gap(8.0).flex_wrap();
    for (days, key) in PRESETS {
        let label = t(key);
        let chosen = form.mode == Mode::Preset(days);
        let mut choice = row().key(format!("data-cleanup-preset-{days}"))
            .role("radio").label(label).checked(chosen).focusable(true)
            .h(30.0).px(12.0).center().radius(k.radius.md)
            .border(1.0, if chosen { k.c.border_focus } else { k.c.border_base })
            .child(text(label).font(CAPTION).color(if chosen { k.c.text_accent } else { k.c.text_muted }));
        if chosen {
            choice = choice.fill(k.c.bg_accent.alpha(0.15));
        } else {
            choice = choice.hover(k.c.surface_hover);
        }
        choices = choices.child(choice.on_press(move |s: &mut AppState| {
            change_form(s, |f| f.mode = Mode::Preset(days));
        }));
    }
    let run = t("settings.dataCleanup.run");
    let action = row().key("data-cleanup-workflow-run").role("button").label(run)
        .focusable(true).h(36.0).px(18.0).center().radius(k.radius.md)
        .fill(k.c.bg_accent).hover(k.c.submit_hover)
        .child(text(run).font(LABEL).color(k.c.submit_fg))
        .on_press(ask);
    let title = t("settings.dataCleanup.title");
    let scope = Checkbox::new("data-cleanup-scope-workflow", form.selected)
        .label(t("settings.dataCleanup.scopeWorkflow"))
        .on_toggle(|s: &mut AppState| change_form(s, |f| f.selected = !f.selected))
        .build(k);
    let scope = setting_row(k, &mut hints, "data-cleanup-workflow-scope".into(),
        t("settings.dataCleanup.scope"), Some(t("settings.nativeRetention.scope")), scope);
    let retention = section_title(k, &mut hints, "data-cleanup-retention".into(),
        t("settings.dataCleanup.retention"), None);
    let custom_label = t("settings.dataCleanup.customDays");
    let mut custom = TextField::new("data-cleanup-custom-days", &form.custom_days)
        .height(36.0).width(96.0).pad_x(12.0).radius(k.radius.md)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .text_style(LABEL).label(custom_label)
        .on_edit(|s: &mut AppState| &mut s.settings_content.workflow_retention.custom_days)
        .on_change(|s: &mut AppState, _| change_form(s, |f| f.mode = Mode::Custom));
    if !dark {
        // The original number field steps with unmodified arrows. Dark
        // adjustments are deferred; do not replace the shared text control.
        custom = custom.on_key(|s: &mut AppState, event, _| step_custom_on_key(s, event));
    }
    let custom = custom.build(dark).focus_within_ring(1.0, -1.0, k.c.border_focus);
    let custom_row = setting_row(k, &mut hints, "data-cleanup-custom-row".into(),
        custom_label, Some(t("settings.nativeRetention.dayRange")), custom);
    let clear = rune_kit::RadioItem::new(t("settings.dataCleanup.clearAll"), form.mode == Mode::ClearAll)
        .on_select(|s: &mut AppState| change_form(s, |f| f.mode = Mode::ClearAll))
        .build(k).key("data-cleanup-clear-all");
    let mut result_row = row().w_full().pt(4.0).gap(12.0).center_y().child(action);
    if let Some(feedback) = form.feedback {
        let (key, message, ink) = match feedback {
            Feedback::Removed(count) => ("data-cleanup-workflow-success",
                t("settings.dataCleanup.success").replace("{count}", &count.to_string()), k.c.success.fg),
            Feedback::Cleared => ("data-cleanup-workflow-success",
                t("settings.nativeRetention.cleared").to_owned(), k.c.success.fg),
            other => ("data-cleanup-workflow-error", t(match other {
                Feedback::NoScope => "settings.dataCleanup.noScope",
                Feedback::InvalidDays => "settings.nativeRetention.dayRange",
                _ => "settings.dataCleanup.error",
            }).to_owned(), k.c.danger.fg),
        };
        result_row = result_row.child(text(message).key(key).role("status").font(CAPTION)
            .color(ink).min_w(0.0).shrink().wrap());
    }
    let card = col().key("data-cleanup-section").w_full().gap(16.0).padding(17.0)
        .radius(k.radius.md).border(1.0, k.c.border_base).fill(k.c.surface_weak)
        .child(row().center_y().gap(4.0)
            .child(text(title).font(TextStyle::new(14.0, 500.0).lh(20.0)).color(k.c.text_base))
            .child(SettingHint::new("data-cleanup-title-hint", title, t("settings.nativeRetention.scope"))
                .build(k, &mut hints)))
        .child(scope).child(retention).child(choices).child(custom_row).child(clear).child(result_row);
    body.child(card).children(hints)
}

/// Mounted beside the window, not inside the scrolling Settings content.
pub(crate) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    if !is_current(state) {
        return;
    }
    let Some((epoch, plan)) = &state.settings_content.workflow_retention.pending else { return };
    let epoch = *epoch;
    let k = kit(dark);
    let (message, accept_label) = match plan {
        Plan::Before(cutoff) => (t("settings.nativeRetention.confirm").replace("{cutoff}", cutoff),
            t("settings.nativeRetention.delete")),
        Plan::ClearAll => (t("settings.nativeRetention.confirmAll").to_owned(),
            t("settings.dataCleanup.clearAll")),
    };
    Dialog::new("data-cleanup-workflow-confirm", t("settings.dataCleanup.confirmTitle"))
        .open(true).open_epoch(epoch).width(496.0).fit().body_max_height(180.0)
        .on_close(move |s: &mut AppState| answer(s, epoch, false))
        .child(text(message).key("data-cleanup-workflow-confirm-message")
            .font(LABEL).color(k.c.text_muted).w_full().wrap())
        .footer(control(k, "data-cleanup-workflow-cancel", t("common.cancel"), ButtonVariant::Ghost,
            move |s: &mut AppState| answer(s, epoch, false)))
        .footer(control(k, "data-cleanup-workflow-accept", accept_label, ButtonVariant::Danger,
            move |s: &mut AppState| answer(s, epoch, true)))
        .build(k, layer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::{ColumnFilter, ColumnSort, ConnectionProfile, DesktopAdapter, TablePage};
    use rune::{Env, Runtime};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Capture(Rc<RefCell<Vec<String>>>);

    impl DesktopAdapter for Capture {
        fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
            Ok(Vec::new())
        }

        fn read_table_page(
            &mut self, _: &str, _: u32, _: u32, _: Option<&ColumnSort>, _: Option<&ColumnFilter>,
        ) -> Result<TablePage, String> {
            Err("not used by retention".into())
        }

        fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
            Err("not used by retention".into())
        }

        fn supports_native_workflow_retention(&self) -> bool {
            true
        }

        fn retain_native_workflow_history_before(&mut self, cutoff: &str) -> Result<usize, String> {
            self.0.borrow_mut().push(cutoff.to_owned());
            Ok(2)
        }
    }

    #[test]
    fn custom_focus_selects_the_existing_draft_and_survives_blur() {
        let mut state = AppState::default();
        state.active = Section::Settings;
        state.set_settings_section("behavior");
        state.desktop = Box::new(Capture::default());
        state.settings_content.workflow_retention.mode = Mode::Preset(7);
        let mut rt = Runtime::new(state, crate::app::view, (1440.0, 900.0), Env {
            system_dark: false, scale: 1.0,
        });
        rt.poll = Some(crate::app::app_poll);
        rt.frame(0.0);
        rt.settle_all();
        let tap = |rt: &mut Runtime<AppState>, key| {
            let (x, y) = rt.find(key).expect("visible Behavior control");
            rt.pointer_down(x, y);
            rt.frame(1.0 / 60.0);
            rt.pointer_up(x, y);
            rt.frame(1.0 / 60.0);
            rt.settle_all();
        };
        tap(&mut rt, "data-cleanup-custom-days");
        let form = &rt.state.settings_content.workflow_retention;
        assert!(form.custom_days.focused && form.mode == Mode::Custom);
        assert_eq!(form.custom_days.text, "30");
        let earliest = Utc::now() - Duration::days(30) - Duration::seconds(1);
        tap(&mut rt, "data-cleanup-workflow-run");
        let Some((_, Plan::Before(cutoff))) = &rt.state.settings_content.workflow_retention.pending
        else { panic!("expected a finite custom cutoff") };
        let latest = Utc::now() - Duration::days(30);
        let actual = chrono::DateTime::parse_from_rfc3339(cutoff).unwrap().with_timezone(&Utc);
        assert!(actual >= earliest && actual <= latest);
    }

    #[test]
    fn finite_retention_uses_the_window_modal_and_retires_stale_consent() {
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let mut state = AppState::default();
        state.active = Section::Settings;
        state.set_settings_section("behavior");
        state.desktop = Box::new(Capture(recorded.clone()));
        let mut rt = Runtime::new(state, crate::app::view, (1440.0, 900.0), Env {
            system_dark: false, scale: 1.0,
        });
        let settle = |rt: &mut Runtime<AppState>| {
            rt.settle_all();
            rt.frame(0.0);
        };
        let tap = |rt: &mut Runtime<AppState>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            settle(rt);
        };
        let run = "data-cleanup-workflow-run";
        let accept = "data-cleanup-workflow-accept";
        settle(&mut rt);
        tap(&mut rt, run);
        {
            let scrim = rt.elements().into_iter().find(|e| e.key.as_deref()
                == Some("data-cleanup-workflow-confirm-scrim") && !e.exiting).unwrap();
            assert_eq!(scrim.visible, [0.0, 0.0, 1440.0, 900.0]);
            assert!(!scrim.path.contains("settings-content"));
        }
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(rt.find(accept).is_none() && recorded.borrow().is_empty());

        tap(&mut rt, run);
        let retired = rt.state.settings_content.workflow_retention.pending.as_ref().unwrap().0;
        rt.state.set_settings_section("general");
        rt.state.set_settings_section("behavior");
        settle(&mut rt);
        tap(&mut rt, run);
        let current = rt.state.settings_content.workflow_retention.pending.clone().unwrap();
        assert_ne!(current.0, retired);
        answer(&mut rt.state, retired, true);
        assert!(recorded.borrow().is_empty());
        tap(&mut rt, accept);
        let Plan::Before(cutoff) = current.1 else { panic!("expected a finite cutoff") };
        assert_eq!(recorded.borrow().as_slice(), &[cutoff]);
        answer(&mut rt.state, current.0, true);
        assert_eq!(recorded.borrow().len(), 1);
    }
}
