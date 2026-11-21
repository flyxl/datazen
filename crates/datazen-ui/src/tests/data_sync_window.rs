//! T268: the DataSyncWindow port — the Vm's derivations (step index,
//! narrow layout, show-next, executing, the status fallback), the
//! window frame (the attrs mirror, the chrome with the history
//! trigger, the stepper, the nav, the status bar), each step's own
//! body under its step, the execute-bar seam's preview-only mount and
//! its five inputs, the dialogs from the seams, and the forwarded
//! callbacks through real presses.

use rune::{Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_compare_step::SyncState;
use crate::views::data_sync::use_data_sync_wizard_state::WizardStep;
use crate::views::{DataSyncWindow, DataSyncWindowEdits, DataSyncWindowVm};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    vm: DataSyncWindowVm,
    edits: DataSyncWindowEdits,
    dark: bool,
    log: Vec<String>,
}

fn probe(vm: DataSyncWindowVm) -> Probe {
    Probe {
        vm,
        edits: DataSyncWindowEdits::default(),
        dark: true,
        log: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataSyncWindow::<Probe>::new(s.vm.clone(), &s.edits)
        .dark(s.dark)
        .on_select_toggle(|st| st.log.push("select_toggle".to_owned()))
        .on_select_close(|st| st.log.push("select_close".to_owned()))
        .on_toggle_policy(|st| st.log.push("policy_toggle".to_owned()))
        .on_close_policy(|st| st.log.push("policy_close".to_owned()))
        .on_history_open(|st| st.log.push("history_open".to_owned()))
        .on_history_close(|st| st.log.push("history_close".to_owned()))
        .on_reconcile_unknown_run(|st, run| st.log.push(format!("reconcile:{run}")))
        .on_go_back(|st| st.log.push("back".to_owned()))
        .on_go_next(|st| st.log.push("next".to_owned()))
        .on_close_error(|st| st.log.push("close_error".to_owned()))
        .on_confirm_execute(|st| st.log.push("confirm_execute".to_owned()))
        .build(s.dark)
}

fn mount(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = state.dark;
    let mut rt = Runtime::new(state, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    keyed(rt, key).is_some()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    keyed(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = keyed(rt, key)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

// -- the Vm's derivations (React 155/161/302/313/319) ----------------------

#[test]
fn the_vm_derivations_pin_the_react_rules() {
    let mut vm = DataSyncWindowVm::default();
    // The seeds: endpoints, idle, nothing selected.
    assert_eq!(vm.step_index(), 0);
    assert!(vm.narrow_layout(), "endpoints is a narrow step");
    assert!(vm.show_next(), "the next shows before the preview");
    assert!(!vm.executing());

    vm.wizard.step = WizardStep::Objects;
    assert_eq!(vm.step_index(), 2);
    assert!(!vm.narrow_layout(), "objects takes the wide column");
    assert!(vm.show_next());

    vm.wizard.step = WizardStep::Preview;
    assert_eq!(vm.step_index(), 4);
    assert!(!vm.show_next(), "the preview navigates with the execute bar");

    vm.wizard.step = WizardStep::Result;
    assert_eq!(vm.step_index(), 5);
    assert!(vm.narrow_layout(), "result is a narrow step");
    assert!(!vm.show_next());

    vm.wizard.sync_state = SyncState::Executing;
    assert!(vm.executing(), "syncState === 'executing'");

    // The status bar's fallback (JS `||`): the empty string falls back.
    assert_eq!(vm.status_text(), crate::i18n::t("common.dataSync"));
    vm.feedback.status_msg = "同步完成".to_owned();
    assert_eq!(vm.status_text(), "同步完成");
}

// -- the window frame -------------------------------------------------------

#[test]
fn the_window_frame_mounts_chrome_stepper_nav_and_status_bar() {
    let rt = mount(probe(DataSyncWindowVm::default()), (1440.0, 816.0));
    assert!(has_key(&rt, "data-sync-window"));
    // The three root attributes ride the hidden mirror.
    assert_eq!(
        label_of(&rt, "data-sync-window-attrs"),
        "sync_state=idle|step=endpoints|write_outcome_uncertain=false"
    );
    // The chrome: the pack title and the run-history trigger.
    assert!(has_key(&rt, "data-sync-window-chrome"));
    assert_eq!(
        keyed(&rt, "data-sync-window-title").and_then(|e| e.text),
        Some(crate::i18n::t("common.dataSyncTitle").to_owned())
    );
    assert!(has_key(&rt, "migration-history-open"), "the trigger");
    assert!(has_key(&rt, "data-sync-wizard-stepper"));
    assert!(has_key(&rt, "data-sync-wizard-nav"));
    assert!(has_key(&rt, "status-bar"));
    assert_eq!(
        keyed(&rt, "data-sync-status-message").and_then(|e| e.text),
        Some(crate::i18n::t("common.dataSync").to_owned()),
        "the empty status message falls back to the pack word"
    );
}

#[test]
fn each_step_mounts_its_own_body() {
    let steps = [
        (WizardStep::Endpoints, "data-sync-endpoint-step"),
        (WizardStep::Setup, "data-sync-options-step"),
        (WizardStep::Objects, "data-sync-objects-step"),
        (WizardStep::Compare, "data-sync-compare-step"),
        (WizardStep::Preview, "data-sync-preview-step"),
        (WizardStep::Result, "data-sync-result"),
    ];
    for (step, key) in steps {
        let mut vm = DataSyncWindowVm::default();
        vm.wizard.step = step;
        let rt = mount(probe(vm), (1440.0, 816.0));
        assert!(has_key(&rt, key), "{step:?} mounts {key}");
    }
}

#[test]
fn the_execute_bar_seam_mounts_only_under_the_preview_step() {
    // Objects: no seam.
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Objects;
    let rt = mount(probe(vm), (1440.0, 816.0));
    assert!(!has_key(&rt, "data-sync-execute-bar-seam"));

    // Preview: the seam with its five inputs on the label.
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Preview;
    vm.row_selection.total_selected_rows = 2;
    vm.row_selection.has_selected_deletes = true;
    vm.endpoints.target_read_only = false;
    vm.wizard.sync_state = SyncState::Executing;
    vm.execution.can_execute = true;
    let rt = mount(probe(vm), (1440.0, 816.0));
    assert!(has_key(&rt, "data-sync-execute-bar-seam"));
    assert_eq!(
        label_of(&rt, "data-sync-execute-bar-seam"),
        "T322|rows=2|deletes=true|readonly=false|executing=true|can=true"
    );

    // Result: no seam again (React 297's gate).
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Result;
    let rt = mount(probe(vm), (1440.0, 816.0));
    assert!(!has_key(&rt, "data-sync-execute-bar-seam"));
}

#[test]
fn the_narrow_layout_caps_the_step_column() {
    // Endpoints (narrow): max-w-2xl = 672pt.
    let rt = mount(probe(DataSyncWindowVm::default()), (1440.0, 816.0));
    let column = keyed(&rt, "data-sync-step-column").expect("the column is laid out");
    assert!(
        (column.rect[2] - 672.0).abs() < 1.0,
        "the narrow column is 672pt, not {:?}",
        column.rect
    );

    // Objects (wide): max-w-6xl = 1152pt.
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Objects;
    let rt = mount(probe(vm), (1440.0, 816.0));
    let column = keyed(&rt, "data-sync-step-column").expect("the column is laid out");
    assert!(
        (column.rect[2] - 1152.0).abs() < 1.0,
        "the wide column is 1152pt, not {:?}",
        column.rect
    );
}

#[test]
fn the_dialogs_mount_from_the_seams() {
    let mut vm = DataSyncWindowVm::default();
    vm.feedback.error_open = true;
    vm.feedback.error_msg = "对比失败".to_owned();
    let rt = mount(probe(vm), (1440.0, 816.0));
    // The dialogs' fragment root mounts last.
    assert!(has_key(&rt, "data-sync-dialogs"));
    // The open error dialog rides the window's layer.
    assert!(has_key(&rt, "data-sync-error-dialog"));
    assert!(has_key(&rt, "data-sync-error"));

    // Closed: the fragment root stays, the dialog does not.
    let rt = mount(probe(DataSyncWindowVm::default()), (1440.0, 816.0));
    assert!(has_key(&rt, "data-sync-dialogs"));
    assert!(!has_key(&rt, "data-sync-error-dialog"));
}

#[test]
fn presses_reach_the_forwarded_callbacks() {
    // The endpoints step is the first step: the back button is inert
    // (the nav's own disabled rule) and the next button is live once
    // the navigation seam allows it.
    let mut vm = DataSyncWindowVm::default();
    vm.navigation.can_next = true;
    let mut rt = mount(probe(vm), (1440.0, 816.0));
    click(&mut rt, "data-sync-next");
    assert_eq!(rt.state.log, vec!["next".to_owned()]);

    // The run-history dialog's trigger opens the history.
    let mut rt = mount(probe(DataSyncWindowVm::default()), (1440.0, 816.0));
    click(&mut rt, "migration-history-open");
    assert_eq!(rt.state.log, vec!["history_open".to_owned()]);
}
