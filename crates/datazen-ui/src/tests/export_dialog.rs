//! T391: ExportDialog port — pinned option tables, scope correction, column
//! selection, result states, slot composition and two-size/two-palette geometry.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::ControlSize;

use crate::components::connection::batch_export_dialog::{DataExportCapability, DataFormat};
use crate::i18n::t;
use crate::views::content_view_dialogs::{
    build_with_export, DialogAction, DialogConnectionCtx, DialogsVm,
};
use crate::views::export_dialog::{
    build, ExportAction, ExportColumn, ExportResult, ExportScope, ExportStatus, ExportVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const ROOT: &str = "content-view-export-dialog";

fn column(name: &str, pk: bool) -> ExportColumn {
    ExportColumn { name: name.to_owned(), data_type: "int".to_owned(), is_primary_key: pk }
}

fn base_vm() -> ExportVm {
    ExportVm {
        table_name: "users".to_owned(),
        columns: vec![column("id", true)],
        rows: vec![vec!["1".to_owned()], vec!["2".to_owned()]],
        db_session_id: Some("c1".to_owned()),
        selected_cols: vec!["id".to_owned()],
        scope: ExportScope::CurrentPage,
        format: DataFormat::Csv,
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: ExportVm,
    actions: Rc<RefCell<Vec<ExportAction>>>,
    dark: bool,
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, ExportAction)> =
        Rc::new(move |state: &mut Scene, action: ExportAction| {
            match action {
                ExportAction::FormatSelectOpen(open) => state.vm.format_select_open = open,
                ExportAction::ScopeSelectOpen(open) => state.vm.scope_select_open = open,
                other => state.actions.borrow_mut().push(other),
            }
            let _ = &actions;
        });
    build::<Scene>(&s.vm, Some(callback), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn scene(vm: ExportVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {size:?}: {outside:?}", outside.len());
}

// --- 1. options and capability ----------------------------------------------

#[test]
fn format_options_keep_their_order_and_web_reasons() {
    let mut vm = base_vm();
    let options = vm.format_options();
    assert_eq!(
        options.iter().map(|o| o.format.value()).collect::<Vec<_>>(),
        vec!["csv", "json", "markdown", "xlsx", "parquet", "sql_insert", "sql_update"]
    );
    assert_eq!(vm.disabled_format_reasons(), vec!["export.formatDisabledParquetRustOnly"]);

    vm.scope = ExportScope::EntireTable;
    assert_eq!(
        vm.disabled_format_reasons(),
        vec!["export.formatDisabledWebSqlScope", "export.formatDisabledParquetRustOnly"]
    );

    vm.columns[0].is_primary_key = false;
    assert_eq!(
        vm.disabled_format_reasons(),
        vec![
            "export.formatDisabledWebSqlScope",
            "export.formatDisabledParquetRustOnly",
            "export.formatDisabledNoKey",
        ]
    );
}

#[test]
fn desktop_entire_table_never_disables_xlsx_and_keeps_parquet() {
    let mut vm = base_vm();
    vm.is_desktop = true;
    vm.scope = ExportScope::EntireTable;
    let disabled: Vec<&str> = vm
        .format_options()
        .into_iter()
        .filter(|option| option.disabled)
        .map(|option| option.format.value())
        .collect();
    assert!(disabled.is_empty(), "desktop SQL entire-table serves every format");
}

#[test]
fn entire_table_scope_requires_a_session_and_capability() {
    let mut vm = base_vm();
    assert!(vm.allow_entire());
    let labels = vm.scope_labels();
    assert!(labels.iter().any(|(scope, _, _)| *scope == ExportScope::EntireTable));

    vm.db_session_id = None;
    assert!(!vm.allow_entire());
    vm.default_scope = Some(ExportScope::EntireTable);
    assert_eq!(vm.effective_scope(), ExportScope::CurrentPage, "a missing session falls back");

    let mut vm = base_vm();
    vm.capability = DataExportCapability::LoadedOnly;
    assert!(!vm.allow_entire());
    assert!(!vm.scope_labels().iter().any(|(scope, _, _)| *scope == ExportScope::EntireTable));
    vm.default_scope = Some(ExportScope::EntireTable);
    assert_eq!(vm.effective_scope(), ExportScope::CurrentPage);

    let mut vm = base_vm();
    vm.capability = DataExportCapability::None;
    assert!(vm.export_locked() && vm.export_disabled());
}

// --- 2. row count and columns -----------------------------------------------

#[test]
fn row_count_follows_the_scope_and_selected_rows() {
    let mut vm = base_vm();
    assert_eq!(vm.row_count(), 2);
    vm.selected_rows = vec![1];
    vm.scope = ExportScope::Selected;
    assert_eq!(vm.row_count(), 1);
    vm.scope = ExportScope::EntireTable;
    vm.total_rows = Some(900);
    assert_eq!(vm.row_count(), 900, "entire-table uses the backend total");
}

#[test]
fn column_selection_toggles_and_reset_rules() {
    let mut vm = base_vm();
    vm.columns = vec![column("id", true), column("name", false)];
    vm.select_all();
    assert_eq!(vm.selected_cols, vec!["id", "name"]);
    vm.toggle_column("name");
    assert_eq!(vm.selected_cols, vec!["id"]);
    assert!(!vm.export_disabled(), "one selected column is enough");
    vm.clear_all();
    assert!(vm.export_disabled(), "no selected columns disables export");
    vm.reset_default();
    assert_eq!(vm.selected_cols, vec!["id", "name"]);
}

// --- 3. result states --------------------------------------------------------

#[test]
fn export_results_drive_the_four_states() {
    let mut vm = base_vm();
    vm.begin_export();
    assert_eq!(vm.status, ExportStatus::Exporting);
    vm.apply_result(ExportResult::Cancelled);
    assert_eq!(vm.status, ExportStatus::Form);
    vm.begin_export();
    vm.apply_result(ExportResult::Aborted { partial_deleted: true });
    assert_eq!(vm.status, ExportStatus::Aborted);
    assert!(vm.partial_deleted && !vm.cancelling);
    vm.begin_export();
    vm.apply_result(ExportResult::Failed("boom".to_owned()));
    assert_eq!(vm.status, ExportStatus::Form);
    assert_eq!(vm.error, "boom");
    vm.begin_export();
    vm.apply_result(ExportResult::Success);
    assert_eq!(vm.status, ExportStatus::Success);
}

#[test]
fn success_and_aborted_footer_and_keys() {
    let mut vm = base_vm();
    vm.status = ExportStatus::Success;
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, &format!("{ROOT}-success")));
    assert!(has_key(&rt, &format!("{ROOT}-confirm")));
    assert!(!has_key(&rt, &format!("{ROOT}-format")));

    let mut vm = base_vm();
    vm.status = ExportStatus::Aborted;
    vm.partial_deleted = true;
    let rt = runtime(scene(vm, true), DESIGN);
    assert_eq!(text_of(&rt, &format!("{ROOT}-aborted-text")), t("export.cancelledPartialDeleted"));
}

#[test]
fn form_pins_keys_and_buttons_emit_actions() {
    let mut rt = runtime(scene(base_vm(), true), DESIGN);
    for key in [
        ROOT,
        &format!("{ROOT}-body"),
        &format!("{ROOT}-format-trigger"),
        &format!("{ROOT}-scope-trigger"),
        &format!("{ROOT}-columns"),
        &format!("{ROOT}-column-id"),
        &format!("{ROOT}-summary"),
        &format!("{ROOT}-confirm"),
        &format!("{ROOT}-cancel"),
        &format!("{ROOT}-close"),
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    click(&mut rt, &format!("{ROOT}-confirm"));
    click(&mut rt, &format!("{ROOT}-cancel"));
    let actions = rt.state.actions.borrow().clone();
    assert_eq!(actions, vec![ExportAction::Export, ExportAction::Close]);
}

// --- 4. slot composition + geometry -----------------------------------------

#[test]
fn real_export_dialog_is_composed_into_the_dialogs_slot() {
    use crate::views::content_view_dialogs::DialogConnectionCtx;

    #[derive(Clone)]
    struct SlotProbe {
        actions: Rc<RefCell<Vec<DialogAction>>>,
    }
    fn slot_view(s: &SlotProbe, _cx: &mut Cx<SlotProbe>) -> El {
        let mut layer = Vec::new();
        let actions = s.actions.clone();
        let callback: Rc<dyn Fn(&mut SlotProbe, DialogAction)> =
            Rc::new(move |state: &mut SlotProbe, action: DialogAction| {
                state.actions.borrow_mut().push(action);
                let _ = &actions;
            });
        let vm = DialogsVm {
            conn_ctx: Some(DialogConnectionCtx::default()),
            export_open: true,
            export_table_name: Some("users".to_owned()),
            ..Default::default()
        };
        let export_vm = base_vm();
        build_with_export::<SlotProbe>(&vm, true, &mut layer, &callback, Some(&export_vm));
        col().w_full().h_full().children(layer)
    }
    let mut rt = Runtime::new(
        SlotProbe { actions: Rc::new(RefCell::new(Vec::new())) },
        slot_view,
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, ROOT), "the real export dialog mounts in the slot");
    assert!(has_key(&rt, &format!("{ROOT}-format-trigger")), "the real format select mounts");
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(rt.state.actions.borrow().contains(&DialogAction::CloseExport));
    let _ = ControlSize::ExtraLarge;
}

#[test]
fn form_geometry_fits_both_sizes_and_both_palettes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(scene(base_vm(), dark), size);
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}

#[test]
fn format_select_shows_the_option_label_not_the_wire_value() {
    let mut vm = base_vm();
    vm.format_select_open = true;
    let rt = runtime(scene(vm, true), DESIGN);
    let option0 = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("option0"));
    assert_eq!(option0.as_ref().and_then(|e| e.label.as_deref()), Some("CSV"), "option-label CSV");
    assert!(
        option0.is_some_and(|e| e.item_selected),
        "option-label CSV is the current selection, not the wire value"
    );
}

#[test]
fn scope_select_shows_the_localized_current_label() {
    let mut vm = base_vm();
    vm.scope_select_open = true;
    let rt = runtime(scene(vm, true), DESIGN);
    let expected = format!("{} (2 {})", t("export.currentPage"), t("common.rows"));
    assert_eq!(
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some("option0")).and_then(|e| e.label).as_deref(),
        Some(expected.as_str())
    );
    assert!(
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some("option0")).is_some_and(|e| e.item_selected),
        "the scope select's current value is the localized label"
    );
}

#[test]
fn selects_open_their_option_lists_and_write_back_a_choice() {
    let mut rt = runtime(scene(base_vm(), true), DESIGN);
    assert!(!has_key(&rt, "option0"), "closed selects mount no option list");
    click(&mut rt, &format!("{ROOT}-format-trigger"));
    assert!(has_key(&rt, "option0"), "the format select must open its option list");
    assert!(rt.state.vm.format_select_open);

    click(&mut rt, "option1"); // JSON
    assert!(
        rt.state.actions.borrow().contains(&ExportAction::SetFormat(DataFormat::Json)),
        "choosing an option emits the wire-value format back to the host"
    );
    assert!(!rt.state.vm.format_select_open, "a choice closes the format list");

    click(&mut rt, &format!("{ROOT}-scope-trigger"));
    assert!(has_key(&rt, "option0"), "the scope select must open its option list");
    assert!(rt.state.vm.scope_select_open);
    click(&mut rt, "option2"); // entire_table (session present)
    assert!(
        rt.state.actions.borrow().contains(&ExportAction::SetScope(ExportScope::EntireTable))
    );
    assert!(!rt.state.vm.scope_select_open);
}
