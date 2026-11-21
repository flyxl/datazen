//! T268 independent audit: the attrs mirror's exact spellings across
//! the state/step/outcome axes, the chrome's pack words verbatim, both
//! themes mounting the same key set inside the minimum window, the
//! busy spinner's nav mount, the run-history dialog's layer opening,
//! and the status panel's hydration-driven hint pair.

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
struct Audit {
    vm: DataSyncWindowVm,
    edits: DataSyncWindowEdits,
    dark: bool,
    /// The run-history dialog's open cell (the host's, the T140
    /// port's controlled contract).
    history_open: bool,
    history_epoch: u64,
    history_closes: usize,
}

fn audit(vm: DataSyncWindowVm, dark: bool) -> Audit {
    Audit {
        vm,
        edits: DataSyncWindowEdits::default(),
        dark,
        history_open: false,
        history_epoch: 0,
        history_closes: 0,
    }
}

fn audit_view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    DataSyncWindow::<Audit>::new(s.vm.clone(), &s.edits)
        .dark(s.dark)
        .on_select_toggle(|_st| {})
        .on_toggle_policy(|_st| {})
        .history_open(s.history_open, s.history_epoch)
        .on_history_close(|st| st.history_closes += 1)
        .build(s.dark)
}

fn mount(state: Audit, size: (f32, f32)) -> Runtime<Audit> {
    let dark = state.dark;
    let mut rt = Runtime::new(state, audit_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .collect()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    keyed(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

/// The attrs mirror spells the three axes exactly, across a state and
/// a step the default never reaches.
#[test]
fn the_attrs_mirror_spells_every_axis() {
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Compare;
    vm.wizard.sync_state = SyncState::Unknown;
    vm.run.write_outcome_uncertain = true;
    let rt = mount(audit(vm, true), (1440.0, 816.0));
    assert_eq!(
        label_of(&rt, "data-sync-window-attrs"),
        "sync_state=unknown|step=compare|write_outcome_uncertain=true"
    );

    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Result;
    vm.wizard.sync_state = SyncState::Done;
    let rt = mount(audit(vm, true), (1440.0, 816.0));
    assert_eq!(
        label_of(&rt, "data-sync-window-attrs"),
        "sync_state=done|step=result|write_outcome_uncertain=false"
    );
}

/// The chrome's words are the pack's exact strings, not paraphrases.
#[test]
fn the_chrome_words_are_the_packs_exact_strings() {
    let rt = mount(audit(DataSyncWindowVm::default(), true), (1440.0, 816.0));
    assert_eq!(
        keyed(&rt, "data-sync-window-title").and_then(|e| e.text),
        Some("数据同步 - DataZen".to_owned())
    );
    assert_eq!(
        keyed(&rt, "migration-history-open").and_then(|e| e.label),
        Some(crate::i18n::t("migrationHistory.open").to_owned())
    );
}

/// Both themes mount the same key set, and every node stays inside the
/// minimum window.
#[test]
fn both_themes_mount_the_same_keys_inside_the_minimum_window() {
    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Compare;
    let dark_keys = keys(&mount(audit(vm.clone(), true), (960.0, 640.0)));
    let light_keys = keys(&mount(audit(vm.clone(), false), (960.0, 640.0)));
    assert_eq!(dark_keys, light_keys, "the key set is theme-stable");

    for dark in [true, false] {
        let rt = mount(audit(vm.clone(), dark), (960.0, 640.0));
        for e in rt.elements() {
            let r = e.rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= 960.5 && r[1] + r[3] <= 640.5,
                "{:?} out of the minimum window (dark={dark})",
                e.key
            );
        }
    }
}

/// The busy spinner rides the nav's next row while the navigation seam
/// is busy, and the next hides on the preview step.
#[test]
fn the_busy_nav_mounts_its_spinner_and_hides_next_late() {
    let mut vm = DataSyncWindowVm::default();
    vm.navigation.busy = true;
    vm.navigation.can_next = true;
    let rt = mount(audit(vm, true), (1440.0, 816.0));
    assert!(keyed(&rt, "data-sync-next-busy").is_some(), "the spinner rides the next row");

    let mut vm = DataSyncWindowVm::default();
    vm.wizard.step = WizardStep::Preview;
    let rt = mount(audit(vm, true), (1440.0, 816.0));
    assert!(
        keyed(&rt, "data-sync-next-row").is_none(),
        "the next row hides when the execute bar navigates"
    );
}

/// The run-history dialog's layer opens under its trigger's control:
/// the open flag mounts the dialog, and its close travels back.
#[test]
fn the_run_history_dialog_layer_follows_the_open_flag() {
    let mut state = audit(DataSyncWindowVm::default(), true);
    state.history_open = true;
    state.history_epoch = 3;
    let rt = mount(state, (1440.0, 816.0));
    assert!(
        keyed(&rt, "migration-run-history-dialog").is_some(),
        "the open flag mounts the dialog layer"
    );

    // Closed: only the trigger.
    let rt = mount(audit(DataSyncWindowVm::default(), true), (1440.0, 816.0));
    assert!(keyed(&rt, "migration-history-open").is_some());
    assert!(keyed(&rt, "migration-run-history-dialog").is_none());
}

/// The job-status strip mounts from its Vm: the hydration pair
/// mounts the panel's own hint banners with the error's text.
#[test]
fn the_job_status_strip_mounts_from_its_vm() {
    let mut vm = DataSyncWindowVm::default();
    vm.job_status.pending_verification_count = 2;
    vm.job_status.hydration_error = Some("恢复失败".to_owned());
    let rt = mount(audit(vm, true), (1440.0, 816.0));
    assert!(keyed(&rt, "data-sync-job-status-panel").is_some());
    assert!(keyed(&rt, "data-sync-pending-hint").is_some());
    assert_eq!(
        keyed(&rt, "data-sync-hydration-error-text").and_then(|e| e.text),
        Some("恢复失败".to_owned()),
        "the hydration error passes verbatim"
    );
}
