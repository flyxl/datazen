//! T550: `TableStructureEditor` — mode titles, validity/capability gates,
//! toolbar states/actions, unsupported/loading/error states, plan preview and
//! rect geometry at both sizes in both palettes. No fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::structure_editor::{
    StructureCapabilities, StructureChangePlan, StructureColumnDraft, StructureEditorUiConfig,
};
use crate::views::table_structure_editor::{
    build, EditorMode, StructureEditorAction, StructureEditorActionFn, StructureEditorVm,
    ADD_COLUMN_KEY, ADD_INDEX_KEY, BACK_KEY, COLUMN_TABLE_KEY, ERROR_KEY, EXECUTE_KEY, EXPORT_KEY,
    LOADING_KEY, PLAN_CLOSE_KEY, PLAN_KEY, ROOT_KEY, TARGET_KEY, TITLE_KEY, UNSUPPORTED_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Probe {
    vm: StructureEditorVm,
    actions: Rc<RefCell<Vec<StructureEditorAction>>>,
    dark: bool,
}

fn column(name: &str) -> StructureColumnDraft {
    StructureColumnDraft {
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
        reorder_column: all,
        ..Default::default()
    }
}

fn vm(mode: EditorMode) -> StructureEditorVm {
    StructureEditorVm {
        mode,
        table_name: "users".to_owned(),
        initial_table_name: Some("users".to_owned()),
        database: Some("app".to_owned()),
        ui_config: Some(StructureEditorUiConfig {
            enabled: Some(true),
            default_column_type: "int".to_owned(),
        }),
        caps: Some(caps(true)),
        export_supported: true,
        columns: vec![column("id")],
        table_name_edit: TextEdit::new("users".to_owned()),
        ..Default::default()
    }
}

fn scene(vm: StructureEditorVm, dark: bool) -> Probe {
    Probe { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(p: &Probe, _cx: &mut Cx<Probe>) -> El {
    let actions = p.actions.clone();
    let on_action: Option<StructureEditorActionFn<Probe>> =
        Some(Rc::new(move |_s, action| actions.borrow_mut().push(action)));
    let mut layer = Vec::new();
    build(&p.vm, &on_action, p.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text(rt: &Runtime<Probe>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn titles_and_capability_validity_gates() {
    let create = vm(EditorMode::Create);
    assert_eq!(create.title(), t("common.newTable"));
    assert_eq!(create.execute_label(), t("structEditor.createTable"));
    assert!(create.is_valid());
    assert!(create.can_add_column(), "create mode uses createTable capability");
    assert!(create.can_add_index());
    assert!(!create.can_export(), "create mode has no export control");

    let alter = vm(EditorMode::Alter);
    assert_eq!(alter.title(), format!("{} · users", t("common.editTableStructure")));
    assert_eq!(alter.execute_label(), t("structEditor.saveChanges"));
    assert!(alter.can_export());

    let mut gated = vm(EditorMode::Create);
    gated.caps = Some(caps(false));
    assert!(!gated.can_add_column());
    assert!(!gated.can_add_index());
    let mut blank = vm(EditorMode::Create);
    blank.table_name.clear();
    assert!(!blank.is_valid());
    assert!(!blank.can_execute());
    assert!(!blank.can_preview());

    let mut executing = vm(EditorMode::Create);
    executing.executing = true;
    assert_eq!(executing.execute_label(), t("structEditor.executing"));
    assert!(!executing.can_execute());
}

#[test]
fn shell_keys_slots_and_toolbar_actions() {
    let mut state = vm(EditorMode::Alter);
    state.show_back_button = true;
    let mut rt = runtime(scene(state, true), DESIGN);
    for key in [ROOT_KEY, TITLE_KEY, BACK_KEY, COLUMN_TABLE_KEY, ADD_COLUMN_KEY, ADD_INDEX_KEY, EXECUTE_KEY] {
        assert!(has_key(&rt, key), "{key} is composed");
    }
    assert_eq!(text(&rt, TITLE_KEY), format!("{} · users", t("common.editTableStructure")));
    assert!(has_key(&rt, TARGET_KEY));
    click(&mut rt, BACK_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![StructureEditorAction::Back]);

    click(&mut rt, ADD_COLUMN_KEY);
    click(&mut rt, ADD_INDEX_KEY);
    click(&mut rt, EXECUTE_KEY);
    let actions = rt.state.actions.borrow().clone();
    assert!(actions.contains(&StructureEditorAction::AddColumn));
    assert!(actions.contains(&StructureEditorAction::AddIndex));
    assert!(actions.contains(&StructureEditorAction::Execute));
}

#[test]
fn unsupported_loading_error_and_plan_preview_states() {
    let mut unsupported = vm(EditorMode::Create);
    unsupported.ui_config = None;
    let rt = runtime(scene(unsupported, false), MIN);
    assert!(has_key(&rt, UNSUPPORTED_KEY));
    assert!(!has_key(&rt, ROOT_KEY));

    let mut loading = vm(EditorMode::Create);
    loading.loading = true;
    let rt = runtime(scene(loading, true), MIN);
    assert!(has_key(&rt, LOADING_KEY));

    let mut failed = vm(EditorMode::Create);
    failed.error = "planner exploded".to_owned();
    let rt = runtime(scene(failed, false), MIN);
    assert!(has_key(&rt, ERROR_KEY));
    assert_eq!(text(&rt, "struct-editor-error-text"), "planner exploded");

    let mut preview = vm(EditorMode::Create);
    preview.preview_plan = Some(StructureChangePlan { statements: Vec::new(), warnings: None });
    let mut rt = runtime(scene(preview, true), DESIGN);
    assert!(has_key(&rt, PLAN_KEY));
    assert!(has_key(&rt, PLAN_CLOSE_KEY));
    click(&mut rt, PLAN_CLOSE_KEY);
    assert!(rt.state.actions.borrow().contains(&StructureEditorAction::ClosePreview));
}

#[test]
fn geometry_both_sizes_both_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut state = vm(EditorMode::Alter);
            state.show_back_button = true;
            state.preview_plan = Some(StructureChangePlan { statements: Vec::new(), warnings: None });
            state.error = "a fairly long failure message".to_owned();
            let rt = runtime(scene(state, dark), (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
            }
        }
    }
}
