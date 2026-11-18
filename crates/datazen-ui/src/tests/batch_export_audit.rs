//! T016 independent audit of the BatchExportDialog port.
//!
//! This is a *tester* file: it does not fix product code. It adds rect-based
//! geometry, key-uniqueness, i18n, modal/focus, callback-boundary and static
//! cohesion coverage that the coder's own tests leave open, and it pins two
//! fidelity defects that are reported to the requester (see `TEST_FAILED`
//! findings in the handoff). All observations come from the runtime's own
//! element list / a headless probe scene — never from a value the component
//! reports about itself.

use rune::{col, row, text, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::connection::batch_export_dialog::{
    BatchExportDialog, BatchExportMode, BatchExportStatus, BatchExportVm, DataExportCapability,
    DataFormat, ExportProgress, OutputMode,
};
use crate::i18n::{has as has_key, t};

const KEY: &str = "batch-export-dialog";
const MAIN_SRC: &str = include_str!("../components/connection/batch_export_dialog.rs");
const PARTS_SRC: &str = include_str!("../components/connection/batch_export_parts.rs");

/// Every `t()` key the component asks for; used to prove none is echoed and all
/// resolve to a real catalogue entry.
const USED_KEYS: &[&str] = &[
    "batchExport.title",
    "batchExport.selectTables",
    "batchExport.clearAll",
    "batchExport.noTablesSelected",
    "batchExport.mode",
    "batchExport.modeDataAndStructure",
    "batchExport.format",
    "batchExport.output",
    "batchExport.outputSingle",
    "batchExport.outputZip",
    "batchExport.exporting",
    "batchExport.export",
    "batchExport.success",
    "batchExport.disabledByDriver",
    "export.cancelling",
    "export.cancelExport",
    "export.cancelledPartialDeleted",
    "export.cancelledPartialKept",
    "common.close",
    "common.cancel",
    "common.selectAll",
    "common.structureOnly",
    "common.dataOnly",
];

// ---------------------------------------------------------------------------
// Probe scene (mirrors how a real host owns the VM in app state `S`).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Scene {
    vm: BatchExportVm,
    dark: bool,
    closes: usize,
    toggles: Vec<String>,
    select_all: usize,
    clear_all: usize,
    modes: Vec<BatchExportMode>,
    formats: Vec<DataFormat>,
    format_opens: usize,
    format_closes: usize,
    outputs: Vec<OutputMode>,
    exports: Vec<ExportRequestAlias>,
    cancels: usize,
}

/// A lightweight stand-in so the test does not need to import the private
/// `ExportRequest` type; fields mirror it exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExportRequestAlias {
    selected_tables: Vec<String>,
    mode: BatchExportMode,
    data_format: DataFormat,
    output_mode: OutputMode,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            vm: BatchExportVm {
                open: true,
                tables: vec!["users".into(), "orders".into(), "products".into()],
                selected: vec!["users".into()],
                ..BatchExportVm::default()
            },
            dark: true,
            closes: 0,
            toggles: Vec::new(),
            select_all: 0,
            clear_all: 0,
            modes: Vec::new(),
            formats: Vec::new(),
            format_opens: 0,
            format_closes: 0,
            outputs: Vec::new(),
            exports: Vec::new(),
            cancels: 0,
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    BatchExportDialog::<Scene>::new(KEY, &scene.vm)
        .on_close(|s: &mut Scene| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_toggle_table(|s: &mut Scene, name| s.toggles.push(name))
        .on_select_all(|s: &mut Scene| s.select_all += 1)
        .on_clear_all(|s: &mut Scene| s.clear_all += 1)
        .on_mode(|s: &mut Scene, mode| s.modes.push(mode))
        .on_data_format(|s: &mut Scene, format| s.formats.push(format))
        .on_format_open(|s: &mut Scene| {
            s.format_opens += 1;
            s.vm.format_open = true;
        })
        .on_format_close(|s: &mut Scene| {
            s.format_closes += 1;
            s.vm.format_open = false;
        })
        .on_output_mode(|s: &mut Scene, output| s.outputs.push(output))
        .on_export(|s: &mut Scene, req| {
            s.exports.push(ExportRequestAlias {
                selected_tables: req.selected_tables,
                mode: req.mode,
                data_format: req.data_format,
                output_mode: req.output_mode,
            });
        })
        .on_cancel_export(|s: &mut Scene| s.cancels += 1)
        .build(scene.dark, &mut layer);
    col().key("audit-page").w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is mounted"))
}

fn has_text<S>(rt: &Runtime<S>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(expected))
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

fn status_scene(status: BatchExportStatus) -> Scene {
    let mut s = Scene::default();
    s.vm.status = status;
    s
}

fn assert_in_window(rt: &Runtime<Scene>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            !e.virtual_extent
                && (r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5)
        })
        .map(|e| format!("{} {:?}", e.key.clone().unwrap_or(e.path.clone()), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn assert_unique_keys(rt: &Runtime<Scene>, label: &str) {
    let mut seen = std::collections::HashSet::new();
    let mut dupes = Vec::new();
    for e in rt.elements() {
        if let Some(k) = e.key {
            if k.starts_with(KEY) && !seen.insert(k.clone()) {
                dupes.push(k);
            }
        }
    }
    assert!(dupes.is_empty(), "{label}: duplicate keys {dupes:?}");
}

// ---------------------------------------------------------------------------
// Modal shell + focus return (task asks for Esc / focus-return coverage).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FocusScene {
    vm: BatchExportVm,
    closes: usize,
}

impl Default for FocusScene {
    fn default() -> Self {
        Self {
            vm: BatchExportVm::default(),
            closes: 0,
        }
    }
}

fn focus_view(scene: &FocusScene, _cx: &mut Cx<FocusScene>) -> El {
    let mut layer = Vec::new();
    let trigger = row()
        .key("audit-open")
        .role("button")
        .focusable(true)
        .on_press(|s: &mut FocusScene| {
            s.vm.open = true;
            s.vm.open_epoch += 1;
        })
        .child(text("Open").key("audit-open-text"));
    BatchExportDialog::<FocusScene>::new(KEY, &scene.vm)
        .on_close(|s: &mut FocusScene| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_format_open(|_s: &mut FocusScene| {})
        .on_format_close(|_s: &mut FocusScene| {})
        .build(true, &mut layer);
    col()
        .key("focus-page")
        .w_full()
        .h_full()
        .child(trigger)
        .children(layer)
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key)
}

fn tab_to_focus(rt: &mut Runtime<FocusScene>, key: &str) {
    for _ in 0..64 {
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
    }
    panic!("{key} is reachable from the keyboard");
}

#[test]
fn escape_returns_focus_to_the_control_that_opened_the_dialog() {
    let mut rt = Runtime::new(
        FocusScene::default(),
        focus_view,
        (1440.0, 816.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    tab_to_focus(&mut rt, "audit-open");
    assert_eq!(focused_key(&rt).as_deref(), Some("audit-open"));

    // Space presses the focused trigger, opening the modal.
    rt.key(KeyEvent {
        key: Key::Text(" ".into()),
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(has(&rt, KEY), "the dialog is open");
    assert_ne!(
        focused_key(&rt).as_deref(),
        Some("audit-open"),
        "focus moved into the modal while it is open"
    );

    escape_focus(&mut rt);
    assert_eq!(rt.state.closes, 1, "Escape ran the close handler");
    assert!(!has(&rt, KEY), "the panel unmounted");
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("audit-open"),
        "focus returned to the trigger after the modal closed"
    );
}

fn escape_focus(rt: &mut Runtime<FocusScene>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

// ---------------------------------------------------------------------------
// Callback boundary honesty: no fabricated exporter state, exact-once firing.
// ---------------------------------------------------------------------------

#[test]
fn pressing_export_emits_one_request_and_fabricates_no_success() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-footer-export"));
    assert_eq!(rt.state.exports.len(), 1, "the export callback fires exactly once");
    assert_eq!(rt.state.exports[0].selected_tables, vec!["users".to_string()]);
    // The dialog is a pure view: it never pretends the export finished. Status
    // must come back from the host; with no host write the form stays put.
    assert!(has(&rt, &format!("{KEY}-form")), "no fake success/exporting state");
    assert!(!has(&rt, &format!("{KEY}-success")));
    assert!(!has(&rt, &format!("{KEY}-spinner")));
}

#[test]
fn cancel_export_is_gated_by_desktop_and_export_override() {
    // Default (desktop, no override): the control exists but is disabled while not exporting.
    let rt = runtime(Scene::default(), (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-footer-cancel-export")));
    assert!(
        !element(&rt, &format!("{KEY}-footer-cancel-export")).focusable,
        "cancel-export is disabled until an export is running"
    );

    // Web hides it.
    let mut web = Scene::default();
    web.vm.is_desktop = false;
    let rt = runtime(web, (1440.0, 816.0));
    assert!(!has(&rt, &format!("{KEY}-footer-cancel-export")));

    // An onExport override hides it too (React: `IS_DESKTOP_BUILD && !onExport`).
    let mut overridden = Scene::default();
    overridden.vm.has_export_override = true;
    let rt = runtime(overridden, (1440.0, 816.0));
    assert!(!has(&rt, &format!("{KEY}-footer-cancel-export")));
}

// ---------------------------------------------------------------------------
// i18n: exact keys, all resolve, no key echo in any state.
// ---------------------------------------------------------------------------

#[test]
fn every_key_the_component_asks_for_resolves_to_a_translation() {
    for key in USED_KEYS {
        assert!(has_key(key), "missing catalogue entry: {key}");
        assert_ne!(t(key), *key, "key echoed for {key}");
    }
}

#[test]
fn no_rendered_state_echoes_an_i18n_key_literal() {
    for scene in [
        Scene::default(),
        status_scene(BatchExportStatus::Exporting),
        status_scene(BatchExportStatus::Success),
        status_scene(BatchExportStatus::Aborted),
    ] {
        let rt = runtime(scene, (1440.0, 816.0));
        for e in rt.elements() {
            if let Some(txt) = e.text.as_deref() {
                assert!(
                    !USED_KEYS.contains(&txt),
                    "i18n key echoed as visible text: {txt}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unique / stable keys across rows and every dialog state.
// ---------------------------------------------------------------------------

#[test]
fn table_rows_have_unique_stable_keys_and_no_state_duplicates_keys() {
    let rt = runtime(Scene::default(), (1440.0, 816.0));
    let rows = ["users", "orders", "products"];
    let mut seen = std::collections::HashSet::new();
    for name in rows {
        let k = format!("{KEY}-table-{name}");
        assert!(has(&rt, &k), "row key mounted: {k}");
        assert!(seen.insert(k.clone()), "row key duplicated: {k}");
    }
    // The same VM re-rendered must produce the same row keys (stability).
    let rt2 = runtime(Scene::default(), (1440.0, 816.0));
    for name in rows {
        assert!(has(&rt2, &format!("{KEY}-table-{name}")), "row key stable: {name}");
    }

    // Cover every state, including the ones the coder test omitted (aborted,
    // locked, error, format-open).
    let mut locked = Scene::default();
    locked.vm.capability = DataExportCapability::LoadedOnly;
    let mut errored = Scene::default();
    errored.vm.error = Some("disk full".to_string());
    let mut open_select = Scene::default();
    open_select.vm.format_open = true;
    for (label, scene) in [
        ("form", Scene::default()),
        ("exporting", status_scene(BatchExportStatus::Exporting)),
        ("success", status_scene(BatchExportStatus::Success)),
        ("aborted", status_scene(BatchExportStatus::Aborted)),
        ("locked", locked),
        ("error", errored),
        ("format-open", open_select),
    ] {
        let rt = runtime(scene, (1440.0, 816.0));
        assert_unique_keys(&rt, label);
    }
}

// ---------------------------------------------------------------------------
// Geometry: rect-based, at both sizes and themes, across every state.
// ---------------------------------------------------------------------------

#[test]
fn no_state_overflows_either_window_in_either_theme() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut locked = Scene::default();
            locked.vm.capability = DataExportCapability::LoadedOnly;
            let mut errored = Scene::default();
            errored.vm.error = Some("disk full".to_string());
            let mut many = Scene::default();
            many.vm.tables = (0..12).map(|i| format!("a_very_long_table_name_{i}")).collect();
            many.vm.selected = many.vm.tables.clone();
            let mut open_select = Scene::default();
            open_select.vm.format_open = true;
            for (label, scene) in [
                ("form", Scene::default()),
                ("exporting", status_scene(BatchExportStatus::Exporting)),
                ("success", status_scene(BatchExportStatus::Success)),
                ("aborted", status_scene(BatchExportStatus::Aborted)),
                ("locked", locked),
                ("error", errored),
                ("many-tables", many),
                ("format-open", open_select),
            ] {
                let mut scene = scene;
                scene.dark = dark;
                let rt = runtime(scene, (w, h));
                let context = format!("{label} dark={dark} {w}x{h}");
                assert_in_window(&rt, w, h, &context);
                let panel = element(&rt, KEY).rect;
                assert!(
                    panel[0] >= -0.5 && panel[0] + panel[2] <= w + 0.5,
                    "{context}: panel outside window: {panel:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Static structure: genuine cohesion split, token/icon discipline.
// ---------------------------------------------------------------------------

#[test]
fn the_split_is_a_genuine_cohesion_split_within_the_line_budget() {
    assert!(MAIN_SRC.lines().count() <= 800, "main dialog exceeds 800 lines");
    assert!(PARTS_SRC.lines().count() <= 800, "parts module exceeds 800 lines");
    // Each sub-structure declared in `parts` must be *constructed by* the main
    // dialog's form() — otherwise the split is dead code dodging review.
    for name in ["TableSelection", "ModeOptions", "FormatSelector", "OutputOptions"] {
        assert!(
            PARTS_SRC.contains(&format!("pub(crate) struct {name}")),
            "{name} is not declared in the parts module"
        );
        assert!(
            MAIN_SRC.contains(&format!("{name} {{")) || MAIN_SRC.contains(&format!("{name}::")),
            "{name} is declared but never used by the main dialog"
        );
    }
    assert!(
        PARTS_SRC.contains("use super::batch_export_dialog::"),
        "parts must share the VM/callback model owned by the dialog"
    );
}

#[test]
fn the_two_component_files_use_kit_tokens_and_registry_icons_only() {
    for (label, src) in [("dialog", MAIN_SRC), ("parts", PARTS_SRC)] {
        assert!(!src.contains("oklch("), "{label}: raw oklch colour literal");
        assert!(!src.contains("#[allow(dead_code)]\nmod "), "{label}: module-wide dead_code blanket");
    }
    assert!(
        MAIN_SRC.contains("use crate::icons::ICON_CHECK;"),
        "the success icon must come from the icon registry"
    );
}

// ---------------------------------------------------------------------------
// VM helper behaviour (React predicates) and honest states.
// ---------------------------------------------------------------------------

#[test]
fn reset_on_open_restores_react_defaults() {
    let mut vm = BatchExportVm {
        selected: vec!["stale".into()],
        mode: BatchExportMode::StructureOnly,
        data_format: DataFormat::SqlUpdate,
        output_mode: OutputMode::Single,
        status: BatchExportStatus::Aborted,
        progress: Some(ExportProgress {
            current: 2,
            total: 3,
            table_name: Some("x".into()),
            rows_written: Some(9),
        }),
        error: Some("old".into()),
        initial_selected: vec!["users".into(), "orders".into()],
        is_desktop: true,
        ..BatchExportVm::default()
    };
    vm.reset_on_open();
    assert_eq!(vm.selected, vec!["users".to_string(), "orders".to_string()]);
    assert_eq!(vm.mode, BatchExportMode::DataAndStructure);
    assert_eq!(vm.data_format, DataFormat::Csv);
    assert_eq!(vm.output_mode, OutputMode::Zip, "desktop default is zip");
    assert_eq!(vm.status, BatchExportStatus::Form);
    assert!(vm.progress.is_none() && vm.error.is_none());
    assert!(!vm.cancelling && !vm.partial_deleted && !vm.format_open);

    let mut web = BatchExportVm {
        is_desktop: false,
        ..BatchExportVm::default()
    };
    web.reset_on_open();
    assert_eq!(web.output_mode, OutputMode::Single, "web default is single");
}

#[test]
fn vm_predicates_match_react() {
    let vm = BatchExportVm {
        tables: vec!["a".into(), "b".into()],
        selected: vec!["a".into(), "b".into()],
        capability: DataExportCapability::FullTable,
        ..BatchExportVm::default()
    };
    assert!(vm.all_selected());
    assert!(!vm.format_disabled());
    assert!(!vm.locked());

    let vm2 = BatchExportVm {
        mode: BatchExportMode::StructureOnly,
        capability: DataExportCapability::LoadedOnly,
        ..vm.clone()
    };
    assert!(vm2.format_disabled());
    assert!(vm2.locked());

    let empty = BatchExportVm::default();
    assert!(!empty.all_selected(), "an empty table list is not 'all selected'");
}

// ---------------------------------------------------------------------------
// FIDELITY DEFECT PROBES (these are expected to FAIL on the audited commit;
// they are the reproduction evidence handed back to the requester).
// ---------------------------------------------------------------------------

/// D1: React's Select shows `selectedOption.label` ("CSV") for the current
/// value "csv". The port passes the raw value as `current`, so the trigger
/// shows "csv" and no option is highlighted as selected when the list opens.
#[test]
fn d1_format_trigger_shows_the_selected_label_not_the_raw_value() {
    let mut rt = runtime(Scene::default(), (1440.0, 816.0));
    assert!(
        has_text(&rt, "CSV"),
        "the default format trigger shows React's label 'CSV', not the raw value 'csv'"
    );
    assert!(
        !has_text(&rt, "csv"),
        "the raw stored value must not be displayed"
    );

    press(&mut rt, &format!("{KEY}-format-field-trigger"));
    assert!(
        element(&rt, "option0").item_selected,
        "the CSV option is highlighted as the current selection"
    );
}

/// D2: `all_selected()` promises "every table selected" but only compares
/// lengths. A duplicated Vec entry can make it claim all-selected while a real
/// table is missing, wrongly disabling the select-all control.
#[test]
fn d2_all_selected_requires_every_table_to_be_selected() {
    let vm = BatchExportVm {
        tables: vec!["users".into(), "orders".into()],
        selected: vec!["users".into(), "users".into()],
        ..BatchExportVm::default()
    };
    assert!(
        !vm.all_selected(),
        "a duplicated selection must not count as every table selected"
    );
}
