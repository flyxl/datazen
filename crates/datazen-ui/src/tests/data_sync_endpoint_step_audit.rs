//! T114 independent audit: fresh vectors over the endpoint step.
//!
//! Written against the ported behavior, not the coder's fixtures: an
//! empty profile list, a pre-selected profile, a name edit that already
//! holds text, the select's dismissal path (the close callback, not just
//! the toggle), and the reconfirmation notice geometry at the narrow
//! window. Mutation evidence lives in the audit commit message
//! (red-then-green, restore verified).

use rune::{Cx, El, ElementInfo, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};

use crate::i18n::t;
use crate::views::{DataSyncEndpointStep, DataSyncEndpointStepVm, SyncProfile};

const NARROW: (f32, f32) = (960.0, 640.0);

/// Fresh host state: no profiles at all (a first-run wizard), a name that
/// already holds text, and one counter per callback.
#[derive(Clone)]
struct Audit {
    vm: DataSyncEndpointStepVm,
    name_edit: TextEdit,
    select_open: bool,
    confirmed: usize,
    name_changes: usize,
    selected: Option<String>,
    save_calls: usize,
    load_calls: usize,
    delete_calls: usize,
    toggles: usize,
    closes: usize,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            vm: DataSyncEndpointStepVm {
                scope_reconfirmation_required: false,
                profiles: Vec::new(),
                selected_profile_id: String::new(),
            },
            name_edit: TextEdit::new("夜间同步".to_owned()),
            select_open: false,
            confirmed: 0,
            name_changes: 0,
            selected: None,
            save_calls: 0,
            load_calls: 0,
            delete_calls: 0,
            toggles: 0,
            closes: 0,
        }
    }
}

fn view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    DataSyncEndpointStep::<Audit>::new(s.vm.clone())
        .profile_name_edit(&s.name_edit)
        .select_open(s.select_open)
        .on_confirm_scope(|st: &mut Audit| st.confirmed += 1)
        .on_profile_name_edit(|st: &mut Audit| &mut st.name_edit)
        .on_profile_name_change(|st: &mut Audit, _tx: &mut Tx| st.name_changes += 1)
        .on_select_profile(|st: &mut Audit, profile_id: &str| {
            st.selected = Some(profile_id.to_owned());
            st.vm.selected_profile_id = profile_id.to_owned();
        })
        .on_save_profile(|st: &mut Audit| st.save_calls += 1)
        .on_load_profile(|st: &mut Audit| st.load_calls += 1)
        .on_delete_profile(|st: &mut Audit| st.delete_calls += 1)
        .on_select_toggle(|st: &mut Audit| {
            st.toggles += 1;
            st.select_open = !st.select_open;
        })
        .on_select_close(|st: &mut Audit| {
            st.closes += 1;
            st.select_open = false;
        })
        .build(true)
}

fn runtime(audit: Audit) -> Runtime<Audit> {
    let mut rt = Runtime::new(audit, view, NARROW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Audit>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Audit>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click(rt: &mut Runtime<Audit>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// -----------------------------------------------------------------------
// Fresh vectors.
// -----------------------------------------------------------------------

#[test]
fn an_empty_profile_list_still_mounts_the_controls_with_all_gates_closed() {
    let rt = runtime(Audit::default());
    assert!(has_key(&rt, "data-sync-profile-controls"));
    // No profiles: the select lists nothing and shows the placeholder;
    // load and delete stay gated (no selected id).
    assert!(has_key(&rt, "data-sync-profile-select-trigger"));
    assert!(
        !element(&rt, "data-sync-profile-load").focusable,
        "load gated with no profiles at all"
    );
    assert!(
        !element(&rt, "data-sync-profile-delete").focusable,
        "delete gated with no profiles at all"
    );
    // Save stays ungated, exactly as React's.
    assert!(
        element(&rt, "data-sync-profile-save").focusable,
        "save is never gated"
    );
}

#[test]
fn the_name_field_renders_the_host_text_it_already_holds() {
    let rt = runtime(Audit::default());
    // The controlled value the host already holds shows in the tree.
    let rendered = rt
        .elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        rendered.contains("夜间同步"),
        "the pre-held name renders: {rendered:?}"
    );
}

#[test]
fn editing_the_name_appends_to_the_same_edit() {
    let mut rt = runtime(Audit::default());
    let (x, y) = rt.find("data-sync-profile-name").expect("the input is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent {
        key: Key::Text("!".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.name_edit.text.contains("夜间同步!"),
        "the append landed in the host edit: {:?}",
        rt.state.name_edit.text
    );
    assert!(rt.state.name_changes >= 1, "the change fired");
}

#[test]
fn a_pre_selected_profile_opens_the_gates_without_a_toggle() {
    let mut audit = Audit::default();
    audit.vm.profiles = vec![
        SyncProfile { id: "p1".to_owned(), name: "一个".to_owned() },
        SyncProfile { id: "p2".to_owned(), name: "两个".to_owned() },
    ];
    audit.vm.selected_profile_id = "p2".to_owned();
    let rt = runtime(audit);
    assert!(
        element(&rt, "data-sync-profile-load").focusable,
        "load ungated with a pre-selected id"
    );
    assert!(
        element(&rt, "data-sync-profile-delete").focusable,
        "delete ungated with a pre-selected id"
    );
}

#[test]
fn pressing_outside_the_open_listbox_closes_it_through_the_host() {
    let mut audit = Audit::default();
    audit.vm.profiles = vec![SyncProfile { id: "p1".to_owned(), name: "一个".to_owned() }];
    let mut rt = runtime(audit);

    // Open the listbox.
    click(&mut rt, "data-sync-profile-select-trigger");
    assert_eq!(rt.state.toggles, 1);
    assert!(rt.state.select_open);
    assert!(has_key(&rt, "option0"), "the listbox is up");

    // Press far outside the listbox: the dismissal layer closes it
    // through the host's on_select_close.
    rt.pointer_down(8.0, 8.0);
    rt.pointer_up(8.0, 8.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.closes, 1, "the close callback fired");
    assert!(!rt.state.select_open, "the host closed its flag");
}

#[test]
fn the_notice_and_controls_fit_the_narrow_window_with_the_notice_up() {
    let mut audit = Audit::default();
    audit.vm.scope_reconfirmation_required = true;
    let rt = runtime(audit);
    assert_eq!(
        element(&rt, "data-sync-recovery-scope-review").role,
        "status"
    );
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > NARROW.0 + 0.5 || r[1] + r[3] > NARROW.1 + 0.5
        })
        .map(|e| e.key.clone().unwrap_or_else(|| e.path.clone()))
        .collect();
    assert!(
        outside.is_empty(),
        "{} node(s) outside {}x{}: {outside:?}",
        outside.len(),
        NARROW.0,
        NARROW.1
    );
}

#[test]
fn the_endpoints_bar_slot_stays_a_laid_out_empty_mount() {
    let rt = runtime(Audit::default());
    let slot = element(&rt, "data-sync-endpoints-bar");
    // The mount is present with zero size until T322 fills it.
    let [_, _, w, h] = slot.rect;
    assert!(
        w >= 0.0 && h >= 0.0,
        "the slot is laid out (T322 fills it): {w}x{h}"
    );
    assert!(
        !rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("data-sync-endpoints-bar-"))),
        "no children under T322's mount yet"
    );
}
