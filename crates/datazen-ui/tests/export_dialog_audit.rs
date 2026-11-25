//! T391 independent audit (tester, not coder).
//!
//! Precise probes of the export dialog's Select wiring: React shows the option
//! LABEL for the current format/scope and both selects are interactive. The
//! port passes the raw wire value as `current` and wires no open state. Also
//! checks the disabled guards and a non-vacuous `visible`-rect geometry path.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::connection::batch_export_dialog::{DataExportCapability, DataFormat};
use datazen_ui::views::export_dialog::{
    build, ExportAction, ExportColumn, ExportScope, ExportVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const ROOT: &str = "content-view-export-dialog";

#[derive(Clone)]
struct AuditScene {
    vm: ExportVm,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self { vm: AuditScene::base(), dark: true }
    }
}

impl AuditScene {
    fn base() -> ExportVm {
        ExportVm {
            table_name: "users".to_owned(),
            columns: vec![ExportColumn {
                name: "id".to_owned(),
                data_type: "int".to_owned(),
                is_primary_key: true,
            }],
            rows: vec![vec!["1".to_owned()], vec!["2".to_owned()]],
            db_session_id: Some("sess".to_owned()),
            selected_cols: vec!["id".to_owned()],
            scope: ExportScope::CurrentPage,
            format: DataFormat::Csv,
            ..Default::default()
        }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let actions: Rc<dyn Fn(&mut AuditScene, ExportAction)> = Rc::new(|_s, _a| {});
    build::<AuditScene>(&s.vm, Some(actions), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn export_disabled_tracks_all_three_react_guards() {
    let mut vm = AuditScene::base();
    assert!(!vm.export_disabled());
    vm.exporting = true;
    assert!(vm.export_disabled());
    vm.exporting = false;
    vm.selected_cols.clear();
    assert!(vm.export_disabled(), "no selected columns disables export");
    vm.selected_cols.push("id".to_owned());
    vm.capability = DataExportCapability::None;
    assert!(vm.export_disabled(), "a driver lock disables export");
}

#[test]
fn entire_table_scope_needs_a_session_and_capability() {
    let mut vm = AuditScene::base();
    vm.default_scope = Some(ExportScope::EntireTable);
    assert_eq!(vm.effective_scope(), ExportScope::EntireTable);
    vm.db_session_id = None;
    assert_eq!(vm.effective_scope(), ExportScope::CurrentPage, "no session falls back to page");
    vm.db_session_id = Some("sess".to_owned());
    vm.capability = DataExportCapability::LoadedOnly;
    assert_eq!(vm.effective_scope(), ExportScope::CurrentPage, "loaded-only cannot stream");
}

#[test]
fn form_stays_inside_via_visible_rects() {
    let mut vm = AuditScene::base();
    vm.table_name = "x".repeat(60);
    vm.error = "a long error that wraps".to_owned();
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(AuditScene { vm: vm.clone(), dark }, size);
            assert!(has_key(&rt, ROOT), "{size:?} dark={dark}: root mounted");
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}

