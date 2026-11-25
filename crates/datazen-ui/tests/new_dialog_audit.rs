//! T239 independent audit: `NewConnectionDialog` vs React.
//!
//! Probes branches the coder's suite does not: availability-driven filtering,
//! trimmed/case-insensitive driver search, the store-group → form group-select
//! path, its label formatting, edit-mode AI notes, and the injected-host-port
//! contract. All observations come from the rendered tree or the VM.

use rune::{col, Cx, El, Env, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::connection::new_connection_dialog::{NewConnectionDialog, NewConnectionDialogVm};
use datazen_ui::i18n::t;

struct Probe {
    dark: bool,
    vm: NewConnectionDialogVm,
    actions: Vec<String>,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let dialog = NewConnectionDialog::<Probe>::new(&s.vm)
        .on_vm(|st: &mut Probe| &mut st.vm)
        .on_action(|st: &mut Probe, a| st.actions.push(format!("{a:?}")))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(dialog), layer)
}

fn runtime(mut vm: NewConnectionDialogVm, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    if !vm.open {
        vm.open(None, None);
    }
    let mut p = Probe {
        dark,
        vm,
        actions: Vec::new(),
    };
    p.dark = dark;
    let mut rt = Runtime::new(
        p,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
}

/// React `availableDrivers` narrows the picker: only drivers the host reports
/// are offered. The port holds the answer in `available_drivers`.
#[test]
fn available_drivers_narrow_the_picker() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(None, None);
    vm.available_drivers = Some(vec!["postgresql".to_owned(), "sqlite".to_owned()]);
    let rt = runtime(vm, true, (1440.0, 816.0));
    assert!(has(&rt, "new-conn-driver-postgresql"));
    assert!(has(&rt, "new-conn-driver-sqlite"));
    assert!(!has(&rt, "new-conn-driver-mysql"), "unavailable driver hidden");
}

/// React `filterDbTypesByQuery` trims and lowercases the query; the port's
/// `db_types` must do the same.
#[test]
fn driver_search_trims_and_is_case_insensitive() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(None, None);
    vm.driver_query = TextEdit::new("  MY  ".to_owned());
    let rt = runtime(vm, true, (1440.0, 816.0));
    assert!(has(&rt, "new-conn-driver-mysql"), "trimmed lowercase query matches");
    assert!(!has(&rt, "new-conn-driver-postgresql"));
}

/// React passes `groupOptions` (no-group + store groups) to `ConnectionFormBody`,
/// which forwards them to the advanced settings group select. The dialog's
/// `group_options()` must actually reach that select in the built tree.
#[test]
fn store_groups_reach_the_form_group_select() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(None, None);
    vm.groups = vec!["preset:development".to_owned()];
    vm.form.advanced.show_advanced = true;
    let rt = runtime(vm, true, (1440.0, 816.0));
    assert!(
        has(&rt, "new-conn-group-field"),
        "advanced settings group select must be fed by the dialog's group options"
    );
    assert!(
        texts(&rt).iter().any(|x| x == &t("newConn.groupDev")),
        "preset group label must be the localized form, not a raw key"
    );
}

/// Stronger D1 guard: the REAL advanced-settings group select (kit trigger
/// `<key>-trigger`) must be laid out — not just the observability marker that
/// carries the bare `new-conn-group-field` key.
#[test]
fn group_wiring_renders_the_real_select_trigger() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(None, None);
    vm.groups = vec!["preset:development".to_owned()];
    vm.form.advanced.show_advanced = true;
    let rt = runtime(vm, true, (1440.0, 816.0));
    assert!(
        has(&rt, "new-conn-group-field-trigger"),
        "the real advanced group select trigger must exist, not only the marker"
    );
}

/// The group option labels must go through React's `formatGroupLabel`, which
/// maps preset keys to their translated labels.
#[test]
fn group_option_labels_follow_format_group_label() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(None, None);
    vm.groups = vec!["preset:development".to_owned()];
    let options = vm.group_options();
    assert_eq!(options[0].0, "", "no-group option first");
    assert_eq!(options[0].1, t("newConn.noGroup"));
    assert_eq!(
        options[1].1,
        t("newConn.groupDev"),
        "preset key must be localized, got {:?}",
        options[1].1
    );
}

/// React renders `ConnectionAiNotes` inside the body when editing an existing
/// connection (`editId`). The port should expose the notes panel in edit mode.
#[test]
fn edit_mode_renders_ai_notes() {
    let mut vm = NewConnectionDialogVm::default();
    vm.open(Some("cfg-1".to_owned()), None);
    let rt = runtime(vm, true, (1440.0, 816.0));
    assert!(has(&rt, "connection-notes-title"), "edit mode shows the AI notes panel");
}

/// The dialog must not invent IPC: its source must not mention connection
/// commands, and all three footer actions go through the injected callback.
#[test]
fn host_port_is_the_only_command_path() {
    let src = include_str!("../src/components/connection/new_connection_dialog.rs");
    for forbidden in [
        "connectionCommands.",
        "saveConnection(",
        "testConnection(",
        "getAvailableDrivers(",
    ] {
        assert!(!src.contains(forbidden), "no fabricated IPC: {forbidden}");
    }
    assert!(src.contains("NewConnectionAction::Save"));
    assert!(src.contains("NewConnectionAction::Test"));
}

/// Geometry with store groups, advanced section open and a long validation
/// error, at both required windows in both palettes, via runtime rects.
#[test]
fn dialog_with_groups_and_errors_fits_both_windows_both_palettes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut vm = NewConnectionDialogVm::default();
            vm.open(None, None);
            vm.groups = vec!["preset:development".to_owned(), "a-very-long-custom-group-name".to_owned()];
            vm.form.host_error = t("newConn.required").to_owned();
            vm.form.test_err = "a rather long connection failure message that must not overflow".to_owned();
            let mut rt = runtime(vm, dark, (w, h));
            rt.state.vm.form.advanced.show_advanced = true;
            refresh(&mut rt);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                let key = e.key.unwrap_or(e.path);
                // The dialog body scrolls, so children below the fold may
                // extend vertically; pin horizontal containment for all nodes
                // and full containment for the panel root only.
                let horizontal = r[0] < -0.5 || r[0] + r[2] > w + 0.5;
                let panel = key == "new-connection-dialog";
                if horizontal || (panel && (r[1] < -0.5 || r[1] + r[3] > h + 0.5)) {
                    outside.push(format!("{key} {r:?}"));
                }
            }
            assert!(
                outside.is_empty(),
                "dialog dark={dark} {w}x{h}: {} node(s) outside: {outside:?}",
                outside.len()
            );
        }
    }
}
