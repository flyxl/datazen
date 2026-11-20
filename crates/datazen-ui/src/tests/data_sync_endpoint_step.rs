//! T114: the Data Sync endpoint step — real probes against the rendered
//! element tree.
//!
//! The first wizard step: the scope-reconfirmation notice, the
//! saved-profile controls (name input, profile select, save / load /
//! delete), and the endpoints-bar mount slot (T322). Every assertion
//! observes runtime elements and host-state effects, never a value the
//! component reports about itself:
//!
//! * the three blocks and their mounting conditions;
//! * the confirm button reaching the host;
//! * the load / delete gate on a selected profile id;
//! * the select: the placeholder, the toggle, the choice mapping to the
//!   profile *id* (the values mapping, not the shown label);
//! * typing in the name field: the `TextEdit` writes and the
//!   per-keystroke change callback;
//! * the localized labels on every control;
//! * geometry in both themes.

use rune::{Cx, El, ElementInfo, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};

use crate::i18n::{has, t};
use crate::views::{DataSyncEndpointStep, DataSyncEndpointStepVm, SyncProfile};

const DESIGN: (f32, f32) = (1440.0, 816.0);

/// Host state: the props as data plus everything the host owns for this
/// step — the name `TextEdit`, the select's open flag, and one counter per
/// callback.
#[derive(Clone)]
struct Probe {
    vm: DataSyncEndpointStepVm,
    name_edit: TextEdit,
    select_open: bool,
    dark: bool,
    confirmed: usize,
    name_changes: usize,
    selected: Option<String>,
    save_calls: usize,
    load_calls: usize,
    delete_calls: usize,
    toggles: usize,
    closes: usize,
    chosen: Option<String>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: DataSyncEndpointStepVm {
                scope_reconfirmation_required: false,
                profiles: vec![
                    SyncProfile {
                        id: "profile-7".to_owned(),
                        name: "nightly 增量".to_owned(),
                    },
                    SyncProfile {
                        id: "profile-8".to_owned(),
                        name: "周报全量".to_owned(),
                    },
                ],
                selected_profile_id: String::new(),
            },
            name_edit: TextEdit::new(String::new()),
            select_open: false,
            dark: true,
            confirmed: 0,
            name_changes: 0,
            selected: None,
            save_calls: 0,
            load_calls: 0,
            delete_calls: 0,
            toggles: 0,
            closes: 0,
            chosen: None,
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataSyncEndpointStep::<Probe>::new(s.vm.clone())
        .profile_name_edit(&s.name_edit)
        .select_open(s.select_open)
        .on_confirm_scope(|st: &mut Probe| st.confirmed += 1)
        .on_profile_name_edit(|st: &mut Probe| &mut st.name_edit)
        .on_profile_name_change(|st: &mut Probe, _tx: &mut Tx| st.name_changes += 1)
        .on_select_profile(|st: &mut Probe, profile_id: &str| {
            st.chosen = Some(profile_id.to_owned());
            st.vm.selected_profile_id = profile_id.to_owned();
        })
        .on_save_profile(|st: &mut Probe| st.save_calls += 1)
        .on_load_profile(|st: &mut Probe| st.load_calls += 1)
        .on_delete_profile(|st: &mut Probe| st.delete_calls += 1)
        .on_select_toggle(|st: &mut Probe| {
            st.toggles += 1;
            st.select_open = !st.select_open;
        })
        .on_select_close(|st: &mut Probe| {
            st.closes += 1;
            st.select_open = false;
        })
        .build(s.dark)
}

fn runtime(probe: Probe) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(
        probe,
        view,
        DESIGN,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts(rt: &Runtime<Probe>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// -----------------------------------------------------------------------
// Mounting.
// -----------------------------------------------------------------------

#[test]
fn the_step_mounts_the_controls_and_the_endpoints_slot() {
    let rt = runtime(Probe::default());
    assert!(has_key(&rt, "data-sync-endpoint-step"), "the step root");
    assert!(
        !has_key(&rt, "data-sync-recovery-scope-review"),
        "no notice without a required reconfirmation"
    );
    assert!(has_key(&rt, "data-sync-profile-controls"), "the controls row");
    // T322's mount: an empty keyed slot, present and laid out.
    assert!(has_key(&rt, "data-sync-endpoints-bar"), "the endpoints-bar mount");
}

#[test]
fn the_scope_review_notice_mounts_when_reconfirmation_is_required() {
    let mut probe = Probe::default();
    probe.vm.scope_reconfirmation_required = true;
    let rt = runtime(probe);

    let notice = element(&rt, "data-sync-recovery-scope-review");
    assert_eq!(notice.role, "status", "React's role=status on the notice");
    assert_eq!(
        element(&rt, "data-sync-recovery-scope-review-text").text.as_deref(),
        Some(t("migrationHistory.syncReconcileNeedsScope"))
    );
    // A kit button carries its text as the accessible label
    // (`ElementInfo.label`); its visible text lives on a `label` child.
    assert_eq!(
        element(&rt, "data-sync-reconfirm-scope").label.as_deref(),
        Some(t("migrationHistory.syncReconcileConfirmScope"))
    );
}

#[test]
fn the_confirm_button_reaches_the_host() {
    let mut probe = Probe::default();
    probe.vm.scope_reconfirmation_required = true;
    let mut rt = runtime(probe);
    assert_eq!(rt.state.confirmed, 0);
    click(&mut rt, "data-sync-reconfirm-scope");
    assert_eq!(rt.state.confirmed, 1, "onConfirmScope fired exactly once");
}

// -----------------------------------------------------------------------
// The profile controls.
// -----------------------------------------------------------------------

#[test]
fn the_name_field_binds_the_host_edit_and_fires_the_change() {
    let mut rt = runtime(Probe::default());
    assert!(has_key(&rt, "data-sync-profile-name"), "the input");
    assert_eq!(
        element(&rt, "data-sync-profile-name-label").text.as_deref(),
        Some(t("sync.profile.name"))
    );

    // Focus the field, type, and observe both the TextEdit write and the
    // per-keystroke change callback.
    let (x, y) = rt.find("data-sync-profile-name").expect("the input is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent {
        key: Key::Text("观测".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.name_edit.text.contains("观测"),
        "the edit holds the text: {:?}",
        rt.state.name_edit.text
    );
    assert!(
        rt.state.name_changes >= 1,
        "onProfileNameChange fired per keystroke: {}",
        rt.state.name_changes
    );
}

#[test]
fn the_select_shows_the_placeholder_and_lists_the_profiles() {
    let rt = runtime(Probe::default());
    let trigger = element(&rt, "data-sync-profile-select-trigger");
    assert_eq!(
        trigger.label.as_deref(),
        Some(t("sync.profile.load")),
        "the a11y name is the load label"
    );
    // Nothing chosen: the placeholder is the select's shown text.
    assert!(
        texts(&rt).iter().any(|text| text == t("sync.profile.select")),
        "the placeholder shows: {:?}",
        texts(&rt)
    );
    assert_eq!(
        element(&rt, "data-sync-profile-load-label").text.as_deref(),
        Some(t("sync.profile.load"))
    );
}

#[test]
fn choosing_a_profile_hands_the_host_the_profile_id() {
    let mut rt = runtime(Probe::default());

    // Open the listbox through the trigger.
    click(&mut rt, "data-sync-profile-select-trigger");
    assert_eq!(rt.state.toggles, 1, "onSelectProfile's toggle fired");
    assert!(rt.state.select_open, "the host flipped its open flag");

    // The listbox holds one row per profile.
    assert!(has_key(&rt, "option0"), "the first option row");
    assert!(has_key(&rt, "option1"), "the second option row");

    // Choose the first: the host receives the *id*, not the label.
    click(&mut rt, "option0");
    assert_eq!(
        rt.state.chosen.as_deref(),
        Some("profile-7"),
        "the values mapping answers with the profile id"
    );
    assert_eq!(
        rt.state.vm.selected_profile_id, "profile-7",
        "the host recorded the selection"
    );
}

#[test]
fn load_and_delete_are_gated_on_a_selected_profile() {
    let mut rt = runtime(Probe::default());
    // Nothing chosen: both buttons are inert (a disabled button takes no
    // press at all, so the host counters stay at zero).
    assert!(
        !element(&rt, "data-sync-profile-load").focusable,
        "load is disabled without a selection"
    );
    click(&mut rt, "data-sync-profile-load");
    click(&mut rt, "data-sync-profile-delete");
    assert_eq!(
        (rt.state.load_calls, rt.state.delete_calls),
        (0, 0),
        "the disabled gates take no press"
    );

    // Save is never gated in React.
    click(&mut rt, "data-sync-profile-save");
    assert_eq!(rt.state.save_calls, 1, "save fires without a selection");

    // Select a profile, then the gates open.
    rt.state.vm.selected_profile_id = "profile-7".to_owned();
    let probe = rt.state.clone();
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, DESIGN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    click(&mut rt, "data-sync-profile-load");
    assert_eq!(rt.state.load_calls, 1, "load fires with a selection");
    click(&mut rt, "data-sync-profile-delete");
    assert_eq!(rt.state.delete_calls, 1, "delete fires with a selection");
}

#[test]
fn the_buttons_carry_the_localized_labels() {
    let rt = runtime(Probe::default());
    // A kit button carries its text as the accessible label.
    assert_eq!(
        element(&rt, "data-sync-profile-save").label.as_deref(),
        Some(t("sync.profile.save"))
    );
    assert_eq!(
        element(&rt, "data-sync-profile-load").label.as_deref(),
        Some(t("sync.profile.load"))
    );
    assert_eq!(
        element(&rt, "data-sync-profile-delete").label.as_deref(),
        Some(t("sync.profile.delete"))
    );
}

// -----------------------------------------------------------------------
// Geometry and locale coverage.
// -----------------------------------------------------------------------

#[test]
fn the_step_stays_inside_the_window_in_both_themes() {
    for dark in [true, false] {
        let mut probe = Probe::default();
        probe.dark = dark;
        probe.vm.scope_reconfirmation_required = true;
        let rt = runtime(probe);
        let outside: Vec<String> = rt
            .elements()
            .iter()
            .filter(|e| {
                let r = e.rect;
                r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > DESIGN.0 + 0.5 || r[1] + r[3] > DESIGN.1 + 0.5
            })
            .map(|e| e.key.clone().unwrap_or_else(|| e.path.clone()))
            .collect();
        assert!(
            outside.is_empty(),
            "dark={dark}: {} node(s) outside {}x{}: {outside:?}",
            outside.len(),
            DESIGN.0,
            DESIGN.1
        );
    }
}

#[test]
fn every_locale_key_the_step_reads_exists() {
    for key in [
        "migrationHistory.syncReconcileNeedsScope",
        "migrationHistory.syncReconcileConfirmScope",
        "sync.profile.name",
        "sync.profile.namePlaceholder",
        "sync.profile.load",
        "sync.profile.select",
        "sync.profile.save",
        "sync.profile.delete",
    ] {
        assert!(has(key), "missing {key}");
    }
}
