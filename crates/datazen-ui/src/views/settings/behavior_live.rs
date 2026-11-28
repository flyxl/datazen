//! Behavior's one live action: finite retention of local workflow run history.
use chrono::{Duration, SecondsFormat, Utc};
use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::settings_ui::section_title;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::types::Section;
use crate::ui::{dialog::Dialog, kit::kit};

const PRESETS: [(u32, &str); 3] = [
    (7, "settings.dataCleanup.preset7"),
    (30, "settings.dataCleanup.preset30"),
    (90, "settings.dataCleanup.preset90"),
];
const LABEL: TextStyle = TextStyle::new(14.0, 400.0).lh(20.0);
const CAPTION: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);

#[derive(Clone, PartialEq)]
pub(crate) struct WorkflowRetention {
    days: u32,
    epoch: u64,
    pending: Option<(u64, String)>,
    feedback: Option<Result<usize, ()>>,
}

impl Default for WorkflowRetention {
    fn default() -> Self {
        Self { days: 30, epoch: 0, pending: None, feedback: None }
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
    if form.pending.is_some() || !PRESETS.iter().any(|(days, _)| *days == form.days) {
        return;
    }
    // Freeze the actual UTC boundary before user consent. Refreshing the
    // screen or waiting on the prompt never changes the set being requested.
    let cutoff = (Utc::now() - Duration::days(i64::from(form.days)))
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    form.epoch = form.epoch.wrapping_add(1);
    form.pending = Some((form.epoch, cutoff));
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
    let Some((_, cutoff)) = form.pending.take() else { return };
    if !accepted {
        return;
    }
    let result = state.desktop.retain_native_workflow_history_before(&cutoff).map_err(|_| ());
    if result.is_ok() {
        state.load_workflow_history();
    }
    state.settings_content.workflow_retention.feedback = Some(result);
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
        let chosen = form.days == days;
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
            if is_current(s) && s.settings_content.workflow_retention.pending.is_none() {
                s.settings_content.workflow_retention.days = days;
                s.settings_content.workflow_retention.feedback = None;
            }
        }));
    }
    let run = t("settings.dataCleanup.run");
    let action = row().key("data-cleanup-workflow-run").role("button").label(run)
        .focusable(true).h(36.0).px(18.0).center().radius(k.radius.md)
        .fill(k.c.bg_accent).hover(k.c.submit_hover)
        .child(text(run).font(LABEL).color(k.c.submit_fg))
        .on_press(ask);
    let mut card = col().key("data-cleanup-section").w_full().gap(16.0).padding(17.0)
        .radius(k.radius.md).border(1.0, k.c.border_base).fill(k.c.surface_weak)
        .child(text(t("settings.dataCleanup.title")).font(TextStyle::new(14.0, 500.0).lh(20.0)).color(k.c.text_base))
        .child(text(t("settings.nativeRetention.scope")).key("data-cleanup-workflow-scope").font(LABEL).color(k.c.text_muted))
        .child(text(t("settings.dataCleanup.retention")).font(CAPTION).color(k.c.text_faint))
        .child(choices)
        .child(row().child(action));
    if let Some(feedback) = form.feedback {
        let (key, message, ink) = match feedback {
            Ok(count) => ("data-cleanup-workflow-success",
                t("settings.dataCleanup.success").replace("{count}", &count.to_string()), k.c.success.fg),
            Err(()) => ("data-cleanup-workflow-error",
                t("settings.dataCleanup.error").to_owned(), k.c.danger.fg),
        };
        card = card.child(text(message).key(key).font(CAPTION).color(ink).w_full().wrap());
    }
    body.child(card).children(hints)
}

/// Mounted beside the window, not inside the scrolling Settings content.
pub(crate) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    if !is_current(state) {
        return;
    }
    let Some((epoch, cutoff)) = &state.settings_content.workflow_retention.pending else { return };
    let epoch = *epoch;
    let k = kit(dark);
    let message = t("settings.nativeRetention.confirm").replace("{cutoff}", cutoff);
    Dialog::new("data-cleanup-workflow-confirm", t("settings.dataCleanup.confirmTitle"))
        .open(true).open_epoch(epoch).width(496.0).fit().body_max_height(180.0)
        .on_close(move |s: &mut AppState| answer(s, epoch, false))
        .child(text(message).key("data-cleanup-workflow-confirm-message")
            .font(LABEL).color(k.c.text_muted).w_full().wrap())
        .footer(control(k, "data-cleanup-workflow-cancel", t("common.cancel"), ButtonVariant::Ghost,
            move |s: &mut AppState| answer(s, epoch, false)))
        .footer(control(k, "data-cleanup-workflow-accept", t("settings.nativeRetention.delete"), ButtonVariant::Danger,
            move |s: &mut AppState| answer(s, epoch, true)))
        .build(k, layer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::{ColumnFilter, ColumnSort, ConnectionProfile, DesktopAdapter, TablePage};
    use rune::{Env, Key, KeyEvent, Mods, Runtime};
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
        assert_eq!(recorded.borrow().as_slice(), &[current.1]);
        answer(&mut rt.state, current.0, true);
        assert_eq!(recorded.borrow().len(), 1);
    }
}
