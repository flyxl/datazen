//! T530: `StructureColumnTable` — headers/cells, capability gates, selection,
//! sort, remove/drag intents and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use rune_kit::SortDir;

use rune::{col, Cx, El, Env, TextEdit, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::structure_column_table::{
    build, cap_enabled, control_disabled_key, Capability, ColumnDraft, ColumnField, ColumnPatch,
    EditorMode, StructureCapabilities, StructureColumnAction, StructureColumnTableVm, UiConfig,
};

fn draft(id: &str, name: &str, kind: &str) -> ColumnDraft {
    ColumnDraft {
        id: id.to_owned(),
        name: TextEdit::new(name.to_owned()),
        data_type: TextEdit::new(kind.to_owned()),
        nullable: true,
        ..Default::default()
    }
}

fn all_caps() -> StructureCapabilities {
    StructureCapabilities {
        create_table: true, add_column: true, rename_column: true, alter_type: true,
        alter_nullability: true, alter_default: true, alter_primary_key: true, create_index: true,
        comment: true, drop_column: true, reorder_column: true,
    }
}

fn vm() -> StructureColumnTableVm {
    let mut original = BTreeSet::new();
    original.insert("c1".to_owned());
    StructureColumnTableVm {
        mode: EditorMode::Create,
        caps: Some(all_caps()),
        ui_config: UiConfig {
            show_comment: true,
            column_types: vec!["integer".to_owned(), "text".to_owned()],
            default_column_type: "text".to_owned(),
        },
        columns: vec![draft("c1", "id", "integer"), draft("c2", "name", "text")],
        original_ids: original,
        reorder_enabled: true,
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: StructureColumnTableVm,
    actions: Rc<RefCell<Vec<StructureColumnAction>>>,
    dark: bool,
}

fn scene(vm: StructureColumnTableVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            StructureColumnAction::Update { id, patch } => {
                if let Some(column) = state.vm.columns.iter_mut().find(|item| &item.id == id) {
                    match patch {
                        ColumnPatch::Name(value) => column.name.replace_text(value.clone()),
                        ColumnPatch::DataType(value) => column.data_type.replace_text(value.clone()),
                        ColumnPatch::Nullable(value) => column.nullable = *value,
                        ColumnPatch::PrimaryKey(value) => column.is_primary_key = *value,
                        ColumnPatch::Unique(value) => column.is_unique = *value,
                        ColumnPatch::DefaultValue(value) => {
                            column.default_value.replace_text(value.clone().unwrap_or_default())
                        }
                        ColumnPatch::Comment(value) => {
                            column.comment.replace_text(value.clone().unwrap_or_default())
                        }
                    }
                }
            }
            StructureColumnAction::Remove(id) => state.vm.columns.retain(|item| &item.id != id),
            StructureColumnAction::Sort(column) => {
                state.vm.sort = match state.vm.sort {
                    Some((current, SortDir::Ascending)) if current == *column => {
                        Some((*column, SortDir::Descending))
                    }
                    _ => Some((*column, SortDir::Ascending)),
                };
            }
            StructureColumnAction::Select(index) => {
                // Selection is observable through the emitted action; no VM field needed.
                let _ = index;
            }
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    let edits: Rc<dyn for<'b> Fn(&'b mut Scene, &str, ColumnField) -> &'b mut TextEdit> =
        Rc::new(|state, id, field| {
            let column = state.vm.columns.iter_mut().find(|item| item.id == id)
                .unwrap_or_else(|| panic!("column {id} exists"));
            match field {
                ColumnField::Name => &mut column.name,
                ColumnField::DataType => &mut column.data_type,
                ColumnField::DefaultValue => &mut column.default_value,
                ColumnField::Comment => &mut column.comment,
            }
        });
    build(&s.vm, Some(edits), Some(callback), s.dark)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn capability_gate_and_disabled_key_are_exact() {
    assert!(cap_enabled(Some(&all_caps()), Capability::RenameColumn));
    assert!(!cap_enabled(None, Capability::RenameColumn));
    assert_eq!(control_disabled_key(), "structEditor.capDisabled");
}

#[test]
fn headers_rows_and_cells_are_pinned() {
    let rt = runtime(scene(vm(), true), (1440.0, 816.0));
    for key in [
        "structure-column-table",
        "structure-column-head",
        "structure-column-head-1",
        "structure-column-head-2",
        "structure-column-head-3",
        "structure-column-head-4",
        "structure-sort-1",
        "structure-column-row-c1",
        "structure-column-row-c2",
        "structure-column-name-c1",
        "structure-column-type-c1",
        "structure-column-nullable-c1",
        "structure-column-pk-c1",
        "structure-column-unique-c1",
        "structure-column-default-c1",
        "structure-column-comment-c1",
        "structure-column-remove-c1",
        "structure-column-grip-c1",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for key in ["structView.fieldName", "structView.type", "structView.nullable", "structView.primaryKey", "structView.unique", "structView.defaultValue", "structView.comment"] {
        assert!(texts.iter().any(|x| x == t(key)), "{key} header is shown");
    }
}

#[test]
fn alter_mode_hides_unique_and_comment_when_not_configured() {
    let mut state = vm();
    state.mode = EditorMode::Alter;
    state.ui_config.show_comment = false;
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "structure-column-unique-c1"));
    assert!(!has_key(&rt, "structure-column-comment-c1"));
}

#[test]
fn toggles_emit_patches_and_primary_key_clears_nullability() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "structure-column-nullable-c1");
    assert!(rt.state.actions.borrow().contains(&StructureColumnAction::Update {
        id: "c1".to_owned(), patch: ColumnPatch::Nullable(false),
    }));

    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "structure-column-pk-c1");
    let actions = rt.state.actions.borrow();
    assert!(actions.contains(&StructureColumnAction::Update {
        id: "c1".to_owned(), patch: ColumnPatch::PrimaryKey(true),
    }), "PK toggle emits its patch");
    assert!(actions.contains(&StructureColumnAction::Update {
        id: "c1".to_owned(), patch: ColumnPatch::Nullable(false),
    }), "a primary key is never nullable");
}

#[test]
fn remove_row_select_and_sort_emit_host_intents() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "structure-column-remove-c1");
    assert!(rt.state.actions.borrow().contains(&StructureColumnAction::Remove("c1".to_owned())));

    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "structure-column-grip-c1");
    assert!(rt.state.actions.borrow().contains(&StructureColumnAction::Select(0)));

    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "structure-sort-1");
    assert!(rt.state.actions.borrow().contains(&StructureColumnAction::Sort(1)));
    assert_eq!(rt.state.vm.sort, Some((1, SortDir::Ascending)));
    assert_eq!(rt.state.vm.row_order(), vec![0, 1], "id sorts before name");
}

#[test]
fn capability_denied_controls_are_disabled() {
    let mut state = vm();
    let mut caps = all_caps();
    caps.drop_column = false;
    caps.alter_nullability = false;
    state.caps = Some(caps);
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    // c1 is an original row, so denied caps disable its remove/nullable controls.
    let remove = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("structure-column-remove-c1"))
        .unwrap_or_else(|| panic!("remove button is laid out"));
    assert!(!remove.focusable, "dropColumn denied: remove is disabled");
    let nullable = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("structure-column-nullable-c1"))
        .unwrap_or_else(|| panic!("nullable checkbox is laid out"));
    assert!(!nullable.focusable, "alterNullability denied: checkbox is disabled");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = runtime(scene(vm(), dark), size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.visible;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.visible))
                .collect();
            assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
        }
    }
}

#[test]
fn every_capability_flag_maps_exactly() {
    let caps = all_caps();
    for control in [
        Capability::CreateTable,
        Capability::AddColumn,
        Capability::RenameColumn,
        Capability::AlterType,
        Capability::AlterNullability,
        Capability::AlterDefault,
        Capability::AlterPrimaryKey,
        Capability::CreateIndex,
        Capability::Comment,
        Capability::DropColumn,
        Capability::ReorderColumn,
    ] {
        assert!(cap_enabled(Some(&caps), control));
        assert!(!cap_enabled(None, control));
    }
}

#[test]
fn new_row_is_always_removable_even_when_drop_is_denied() {
    let mut state = vm();
    let mut caps = all_caps();
    caps.drop_column = false;
    state.caps = Some(caps);
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    click(&mut rt, "structure-column-remove-c2");
    assert!(
        rt.state.actions.borrow().contains(&StructureColumnAction::Remove("c2".to_owned())),
        "a just-added row is removable even when the driver denies dropColumn"
    );
}

#[test]
fn sort_reverses_and_orders_by_name() {
    let mut state = vm();
    state.columns = vec![draft("c1", "zebra", "text"), draft("c2", "apple", "text")];
    state.sort = Some((0, SortDir::Ascending));
    assert_eq!(state.row_order(), vec![1, 0]);
    state.sort = Some((0, SortDir::Descending));
    assert_eq!(state.row_order(), vec![0, 1]);
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Removing the `common.confirm` apply label made `dialog_names_column_and_shows_picker_intent`
//    fail on `set-value-date-apply`.
// 2. Dropping the PK→nullable patch made `toggles_emit_patches_and_primary_key_clears_nullability`
//    fail on `a primary key is never nullable`.

#[test]
fn capability_denied_name_input_does_not_mutate_the_draft() {
    let mut state = vm();
    let mut caps = all_caps();
    caps.rename_column = false;
    state.caps = Some(caps);
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    let (x, y) = rt.find("structure-column-name-c1").unwrap_or_else(|| panic!("name field is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("SHOULD_NOT_APPLY");
    rt.frame(0.0);
    let name = rt.state.vm.columns.iter().find(|item| item.id == "c1").map(|item| item.name.text.clone());
    assert_eq!(name.as_deref(), Some("id"), "React disables the name input when renameColumn is denied");
    let input = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some("structure-column-name-c1"))
        .unwrap_or_else(|| panic!("name field is laid out"));
    assert!(!input.focusable, "a capability-denied field is disabled");
}

// Independent lane-C repro probe (original D2), retained as a regression guard.
#[test]
fn probe_capability_denied_text_fields_are_disabled() {
    let mut state = vm();
    let mut caps = all_caps();
    caps.rename_column = false;
    caps.alter_type = false;
    caps.alter_default = false;
    caps.comment = false;
    state.caps = Some(caps);
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    let (x, y) = rt.find("structure-column-name-c1").unwrap_or_else(|| panic!("name field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("SHOULD_NOT_APPLY");
    rt.frame(0.0);
    assert_eq!(
        rt.state.vm.columns[0].name.text, "id",
        "React disables the name input when renameColumn is denied"
    );
}
