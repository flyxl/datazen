//! Independent audit for T550: `TableStructureEditor` port.
//!
//! Cover the `enabled` capability gate, export preconditions, disabled-control
//! tooltips and toolbar states the coder does not drive.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::structure_editor::{StructureCapabilities, StructureEditorUiConfig};
use crate::views::table_structure_editor::{
    build, EditorMode, StructureEditorAction, StructureEditorActionFn, StructureEditorVm,
    ADD_COLUMN_KEY, BACK_KEY, EXECUTE_KEY, LOADING_KEY, UNSUPPORTED_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Probe {
    vm: StructureEditorVm,
    actions: Rc<RefCell<Vec<StructureEditorAction>>>,
    dark: bool,
}

fn column(name: &str) -> crate::structure_editor::StructureColumnDraft {
    crate::structure_editor::StructureColumnDraft {
        id: name.to_owned(),
        name: name.to_owned(),
        data_type: "int".to_owned(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
        is_unique: false,
    }
}

fn caps(all: bool) -> StructureCapabilities {
    StructureCapabilities {
        create_table: all,
        add_column: all,
        create_index: all,
        ..Default::default()
    }
}

fn vm() -> StructureEditorVm {
    StructureEditorVm {
        mode: EditorMode::Alter,
        table_name: "users".to_owned(),
        initial_table_name: Some("users".to_owned()),
        database: Some("app".to_owned()),
        ui_config: Some(StructureEditorUiConfig { enabled: Some(true), default_column_type: "int".to_owned() }),
        caps: Some(caps(true)),
        export_supported: true,
        columns: vec![column("id")],
        table_name_edit: TextEdit::new("users".to_owned()),
        ..Default::default()
    }
}

fn view(p: &Probe, _cx: &mut Cx<Probe>) -> El {
    let actions = p.actions.clone();
    let on_action: Option<StructureEditorActionFn<Probe>> =
        Some(Rc::new(move |_s, action| actions.borrow_mut().push(action)));
    let mut layer = Vec::new();
    build(&p.vm, &on_action, p.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(vm: StructureEditorVm, dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe { vm, actions: Rc::new(RefCell::new(Vec::new())), dark },
        view,
        DESIGN,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("audit: {key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn validity_edges_follow_react() {
    let mut s = vm();
    assert!(s.is_valid());
    s.table_name = "   ".to_owned();
    assert!(!s.is_valid(), "audit: a whitespace table name is invalid");
    s.table_name = "users".to_owned();
    s.columns = vec![column("   "), column("id")];
    assert_eq!(s.valid_columns().len(), 1, "audit: whitespace column names are excluded");
    let mut create = vm();
    create.mode = EditorMode::Create;
    assert_eq!(create.title(), t("common.newTable"));
    assert_eq!(create.execute_label(), t("structEditor.createTable"));
}

#[test]
fn cancel_and_preview_toolbar_states() {
    let mut s = vm();
    s.show_back_button = false;
    let mut rt = runtime(s, true);
    assert!(!has_key(&rt, BACK_KEY), "audit: no Back without showBackButton");
    assert!(has_key(&rt, "struct-editor-cancel"), "audit: Cancel replaces Back");
    click(&mut rt, "struct-editor-cancel");
    assert!(rt.state.actions.borrow().contains(&StructureEditorAction::Cancel));

    let mut invalid = vm();
    invalid.table_name.clear();
    let rt = runtime(invalid, false);
    assert!(!element(&rt, "struct-editor-preview").focusable, "audit: Preview disabled when invalid");
    assert!(!element(&rt, EXECUTE_KEY).focusable, "audit: Execute disabled when invalid");
}

#[test]
fn unsupported_and_loading_geometry_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            for s in [
                { let mut s = vm(); s.ui_config = None; s },
                { let mut s = vm(); s.loading = true; s },
            ] {
                let mut rt = Runtime::new(
                    Probe { vm: s.clone(), actions: Rc::new(RefCell::new(Vec::new())), dark },
                    view, (w, h), Env { system_dark: dark, scale: 1.0 },
                );
                rt.frame(0.0);
                let expected = if s.ui_config.is_none() { UNSUPPORTED_KEY } else { LOADING_KEY };
                assert!(has_key(&rt, expected), "audit: {expected} laid out at {w}x{h} dark={dark}");
                for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, ew, eh] = el.rect;
                    assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                    assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                }
            }
        }
    }
}

// ----- React parity assertions -----

#[test]
fn react_enabled_false_means_unsupported() {
    let mut s = vm();
    s.ui_config = Some(StructureEditorUiConfig { enabled: Some(false), default_column_type: "int".to_owned() });
    assert!(
        !s.supported(),
        "DEFECT: React resolveUiConfig returns null when enabled === false (TableStructureEditor.tsx:63)"
    );
}

#[test]
fn react_export_requires_a_nonempty_initial_table_name() {
    let mut s = vm();
    s.initial_table_name = Some(String::new());
    assert!(
        !s.can_export(),
        "DEFECT: React gates export on a truthy initialTableName (TableStructureEditor.tsx:500)"
    );
}

#[test]
fn react_disabled_add_column_shows_capability_tooltip() {
    let mut s = vm();
    s.caps = Some(caps(false));
    let rt = runtime(s, true);
    assert_eq!(
        element(&rt, ADD_COLUMN_KEY).help.as_deref(),
        Some(t("structEditor.capDisabled")),
        "DEFECT: React sets title=capDisabled when add-column is gated (TableStructureEditor.tsx:581)"
    );
}
