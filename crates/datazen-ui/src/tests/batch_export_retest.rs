//! Independent retest for the two T016 audit fixes (D1 format label, D2
//! all-selected membership). Pure VM cases plus an observation-only runtime
//! probe — never reads a value the component reports about itself.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::connection::batch_export_dialog::BatchExportVm;
use crate::components::connection::batch_export_dialog::BatchExportDialog;

const KEY: &str = "batch-export-dialog";
const PARTS_SRC: &str = include_str!("../components/connection/batch_export_parts.rs");

// ---------------------------------------------------------------------------
// D1: runtime observation of the format trigger + listbox selection.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Scene {
    vm: BatchExportVm,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            vm: BatchExportVm {
                open: true,
                ..BatchExportVm::default()
            },
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    BatchExportDialog::<Scene>::new(KEY, &scene.vm)
        .on_format_open(|s: &mut Scene| s.vm.format_open = true)
        .on_format_close(|s: &mut Scene| s.vm.format_open = false)
        .build(true, &mut layer);
    col().key("retest-page").w_full().h_full().children(layer)
}

fn runtime() -> Runtime<Scene> {
    let mut rt = Runtime::new(
        Scene::default(),
        view,
        (1440.0, 816.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn has_text(rt: &Runtime<Scene>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(expected))
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn d1_closed_trigger_shows_label_and_open_listbox_highlights_current_option() {
    let mut rt = runtime();

    // Closed trigger: React shows the option label "CSV", never the raw "csv".
    assert!(has_text(&rt, "CSV"), "closed trigger shows the 'CSV' label");
    assert!(!has_text(&rt, "csv"), "raw 'csv' value must not be displayed");

    // Open listbox: the current option reports itself as selected.
    press(&mut rt, &format!("{KEY}-format-field-trigger"));
    assert!(
        element(&rt, "option0").item_selected,
        "the CSV option is highlighted as the current selection"
    );
}

#[test]
fn d1_product_source_passes_the_label_to_current() {
    assert!(
        PARTS_SRC.contains(".current(Some(vm.data_format.label()))"),
        "the format Select must pass the option label to .current()"
    );
}

// ---------------------------------------------------------------------------
// D2: all_selected() membership semantics.
// ---------------------------------------------------------------------------

#[test]
fn d2_all_selected_membership_cases() {
    // Duplicated selection: two slots filled but one real table missing -> false.
    let dup = BatchExportVm {
        tables: vec!["users".into(), "orders".into()],
        selected: vec!["users".into(), "users".into()],
        ..BatchExportVm::default()
    };
    assert!(!dup.all_selected(), "duplicated selection is not all-selected");

    // All distinct and present -> true.
    let all = BatchExportVm {
        tables: vec!["users".into(), "orders".into()],
        selected: vec!["users".into(), "orders".into()],
        ..BatchExportVm::default()
    };
    assert!(all.all_selected(), "all distinct tables selected -> true");

    // Empty table list -> false (documented React behavior; coder's
    // vm_predicates_match_react already asserts this on BatchExportVm::default()).
    let empty = BatchExportVm::default();
    assert!(!empty.all_selected(), "empty table list is not all-selected");

    // Partial selection -> false.
    let partial = BatchExportVm {
        tables: vec!["users".into(), "orders".into(), "products".into()],
        selected: vec!["users".into()],
        ..BatchExportVm::default()
    };
    assert!(!partial.all_selected(), "partial selection is not all-selected");
}
