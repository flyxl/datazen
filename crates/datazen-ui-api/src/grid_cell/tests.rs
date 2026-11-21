use super::*;
use serde_json::{json, Value};

fn input() -> GridCellInput {
    GridCellInput {
        row_index: 2,
        column_name: "email".to_owned(),
        column_type: "text".to_owned(),
        value: Some(json!("a@b.com")),
        width: 120,
        sticky: None,
        selected: false,
        error: false,
        active: false,
        editing: false,
        cell_type: None,
        column_info: None,
        editor: None,
        id: Some("cell-2".to_owned()),
        grid_semantics: false,
        dirty: false,
        dirty_tooltip: None,
        dirty_aria_label: None,
        fk: None,
        has_fk_activate: false,
        has_quick_look: false,
        presentation_plan: None,
    }
}

fn text(kind: FkTextKind, target: &str) -> String {
    match kind {
        FkTextKind::Jump => format!("jump:{target}"),
        FkTextKind::Resolving => "resolving".to_owned(),
        FkTextKind::Selector => format!("pick:{target}"),
    }
}

fn view(input: &GridCellInput) -> GridCellView {
    build_grid_cell_view(input, &text)
}

#[test]
fn the_base_contract_is_always_present() {
    let cell = view(&input());
    assert_eq!(cell.attributes.get("data-testid").map(String::as_str), Some(CELL_TEST_ID));
    assert_eq!(cell.attributes.get("data-dt-row").map(String::as_str), Some("2"));
    assert_eq!(cell.attributes.get("data-dt-col").map(String::as_str), Some("email"));
    assert_eq!(cell.attributes.len(), 3);
    assert_eq!(cell.classes, vec![CLASS_BASE]);
    assert_eq!(cell.width, 120);
    assert_eq!(cell.role, None);
    assert_eq!(cell.aria_selected, None);
    assert_eq!(cell.sticky_style, None);
    assert_eq!(cell.children.len(), 1);
    assert_eq!(cell.write_target, CellWriteTarget::Legacy);
    match &cell.children[0] {
        CellChild::Renderer(renderer) => {
            assert_eq!(renderer.column_name, "email");
            assert_eq!(renderer.data_type, "text");
            assert!(renderer.nullable);
            assert!(!renderer.integer_only);
            assert_eq!(renderer.editor_kind_override, None);
        }
        other => panic!("expected a renderer, got {other:?}"),
    }
}

#[test]
fn every_optional_attribute_is_added_not_overwritten() {
    let mut cell_input = input();
    cell_input.selected = true;
    cell_input.active = true;
    cell_input.cell_type = Some("number".to_owned());
    cell_input.editing = true;
    cell_input.error = true;
    cell_input.dirty = true;
    cell_input.grid_semantics = true;
    cell_input.column_info = Some(ColumnCellFacts {
        nullable: false,
        integer_only: true,
        read_only_reason: Some("column-locked".to_owned()),
    });
    cell_input.editor = Some(EditorCellFacts {
        panel_id: None,
        has_value_handler: true,
        has_load_candidates: false,
        read_only_reason: Some("editor-locked".to_owned()),
    });

    let cell = view(&cell_input);
    assert_eq!(cell.attributes.get("data-dt-selected").map(String::as_str), Some("true"));
    assert_eq!(cell.attributes.get("data-dt-active").map(String::as_str), Some("true"));
    assert_eq!(cell.attributes.get("data-dt-cell-type").map(String::as_str), Some("number"));
    assert_eq!(cell.attributes.get("data-dt-editing").map(String::as_str), Some("true"));
    assert_eq!(cell.attributes.get("data-dt-cell-error").map(String::as_str), Some("true"));
    assert_eq!(cell.attributes.get("data-dt-dirty").map(String::as_str), Some("true"));
    assert_eq!(cell.attributes.get("data-dt-readonly").map(String::as_str), Some("editor-locked"));
    assert!(!cell.attributes.contains_key("data-dt-null"));
    assert_eq!(cell.role, Some(ROLE_GRIDCELL));
    assert_eq!(cell.aria_selected, Some("true"));
    assert_eq!(cell.write_target, CellWriteTarget::Editor);
    assert_eq!(
        cell.classes,
        vec![CLASS_BASE, CLASS_DIRTY_BORDER]
    );
}

#[test]
fn null_undefined_and_readonly_fallbacks_are_pinned() {
    let mut null_value = input();
    null_value.value = None;
    assert_eq!(
        view(&null_value).attributes.get("data-dt-null").map(String::as_str),
        Some("true")
    );
    let mut explicit_null = input();
    explicit_null.value = Some(Value::Null);
    assert_eq!(
        view(&explicit_null).attributes.get("data-dt-null").map(String::as_str),
        Some("true")
    );

    let mut column_only = input();
    column_only.column_info = Some(ColumnCellFacts {
        nullable: false,
        integer_only: true,
        read_only_reason: Some("column-locked".to_owned()),
    });
    let cell = view(&column_only);
    assert_eq!(cell.attributes.get("data-dt-readonly").map(String::as_str), Some("column-locked"));
    match &cell.children[0] {
        CellChild::Renderer(renderer) => {
            assert!(!renderer.nullable);
            assert!(renderer.integer_only);
        }
        other => panic!("expected a renderer, got {other:?}"),
    }
}

#[test]
fn sticky_cells_add_an_opaque_base_and_an_overlay() {
    let mut sticky_input = input();
    sticky_input.sticky = Some(StickyInfo {
        left: 48,
        bg_class: "bg-surface-alt".to_owned(),
    });
    let cell = view(&sticky_input);
    assert_eq!(cell.sticky_style, Some(StickyStyle { left: 48, z_index: 2 }));
    assert_eq!(cell.classes, vec![CLASS_BASE, CLASS_STICKY]);
    match &cell.children[0] {
        CellChild::StickyOverlay { classes } => {
            assert_eq!(classes, &vec![CLASS_STICKY_OVERLAY.to_owned(), "bg-surface-alt".to_owned()]);
        }
        other => panic!("expected an overlay, got {other:?}"),
    }
}

#[test]
fn foreign_key_buttons_follow_navigable_pickable_and_editing() {
    let mut fk_input = input();
    fk_input.fk = Some(FkCellAffordance {
        target_label: "customers.id".to_owned(),
        navigable: true,
        resolving: false,
        pickable: Some(true),
    });
    let cell = view(&fk_input);
    assert_eq!(cell.attributes.get("data-dt-fk").map(String::as_str), Some("customers.id"));
    assert_eq!(cell.title.as_deref(), Some("jump:customers.id"));
    assert_eq!(cell.aria_label.as_deref(), Some("jump:customers.id"));
    assert_eq!(cell.children.len(), 3);
    match &cell.children[1] {
        CellChild::FkJump(button) => {
            assert_eq!(button.test_marker, "data-dt-fk-click");
            assert!(!button.disabled);
            assert!(!button.resolving);
            assert_eq!(button.title.as_deref(), Some("jump:customers.id"));
        }
        other => panic!("expected a jump button, got {other:?}"),
    }
    match &cell.children[2] {
        CellChild::FkPick(button) => {
            assert_eq!(button.test_marker, "data-dt-fk-pick");
            assert_eq!(button.title.as_deref(), Some("pick:customers.id"));
        }
        other => panic!("expected a pick button, got {other:?}"),
    }

    fk_input.editing = true;
    let editing = view(&fk_input);
    assert_eq!(editing.children.len(), 1);
    match &editing.children[0] {
        CellChild::Renderer(renderer) => assert_eq!(renderer.editor_kind_override, Some(EDITOR_KIND_ENUM)),
        other => panic!("expected a renderer, got {other:?}"),
    }

    let mut resolving = input();
    resolving.fk = Some(FkCellAffordance {
        target_label: "customers.id".to_owned(),
        navigable: true,
        resolving: true,
        pickable: None,
    });
    let resolving_view = view(&resolving);
    assert_eq!(resolving_view.title.as_deref(), Some("resolving"));
    match &resolving_view.children[1] {
        CellChild::FkJump(button) => assert!(button.disabled && button.resolving),
        other => panic!("expected a jump button, got {other:?}"),
    }
}

#[test]
fn dirty_tooltip_and_aria_label_win_over_foreign_key_text() {
    let mut dirty_input = input();
    dirty_input.dirty = true;
    dirty_input.selected = true;
    dirty_input.dirty_tooltip = Some("old -> new".to_owned());
    dirty_input.dirty_aria_label = Some("changed".to_owned());
    dirty_input.fk = Some(FkCellAffordance {
        target_label: "customers.id".to_owned(),
        navigable: true,
        resolving: false,
        pickable: None,
    });
    let cell = view(&dirty_input);
    assert_eq!(cell.title.as_deref(), Some("old -> new"));
    assert_eq!(cell.aria_label.as_deref(), Some("changed"));
    assert_eq!(cell.classes, vec![CLASS_BASE, CLASS_DIRTY_BORDER]);
    match cell.children.last() {
        Some(CellChild::DirtyMark { classes }) => assert_eq!(classes, &vec![CLASS_DIRTY_MARK.to_owned()]),
        other => panic!("expected a dirty mark, got {other:?}"),
    }
    let mut unselected = dirty_input;
    unselected.selected = false;
    let unselected_view = view(&unselected);
    assert_eq!(
        unselected_view.classes,
        vec![CLASS_BASE, CLASS_DIRTY_BORDER, CLASS_DIRTY_BG]
    );
}

#[test]
fn events_obey_their_render_guards() {
    let cell = view(&input());
    let outcome = dispatch_cell_event(CellEvent::DoubleClick, &input(), &cell);
    assert!(outcome.stop_propagation && outcome.command_emitted);
    assert_eq!(
        cell_command(CellEvent::DoubleClick, &input(), &cell),
        Some(CellCommand::DoubleClick {
            row_index: 2,
            column_name: "email".to_owned()
        })
    );

    let mut quick = input();
    quick.has_quick_look = false;
    let blocked = dispatch_cell_event(CellEvent::MouseDown { button: 1 }, &quick, &view(&quick));
    assert!(!blocked.command_emitted && !blocked.prevent_default);
    quick.has_quick_look = true;
    let allowed = dispatch_cell_event(CellEvent::MouseDown { button: 1 }, &quick, &view(&quick));
    assert!(allowed.command_emitted && allowed.prevent_default);
    assert_eq!(
        cell_command(CellEvent::MouseDown { button: 1 }, &quick, &view(&quick)),
        Some(CellCommand::QuickLook {
            row_index: 2,
            column_name: "email".to_owned()
        })
    );
    let left = dispatch_cell_event(CellEvent::MouseDown { button: 0 }, &quick, &view(&quick));
    assert!(!left.command_emitted);

    let mut fk_input = input();
    fk_input.fk = Some(FkCellAffordance {
        target_label: "customers.id".to_owned(),
        navigable: true,
        resolving: false,
        pickable: Some(true),
    });
    let fk_view = view(&fk_input);
    let no_handler = dispatch_cell_event(CellEvent::FkActivateClick, &fk_input, &fk_view);
    assert!(!no_handler.command_emitted);
    fk_input.has_fk_activate = true;
    let activated = dispatch_cell_event(CellEvent::FkActivateClick, &fk_input, &view(&fk_input));
    assert!(activated.command_emitted && activated.stop_propagation);
    assert_eq!(
        cell_command(CellEvent::FkPickClick, &fk_input, &view(&fk_input)),
        Some(CellCommand::DoubleClick {
            row_index: 2,
            column_name: "email".to_owned()
        })
    );
    let child = dispatch_cell_event(CellEvent::FkChildDoubleClick, &fk_input, &view(&fk_input));
    assert!(child.stop_propagation && !child.command_emitted);
    let child_down = dispatch_cell_event(CellEvent::FkChildMouseDown, &fk_input, &view(&fk_input));
    assert!(child_down.prevent_default && !child_down.command_emitted);

    let commit = cell_command(
        CellEvent::RendererCommit {
            value: Some(json!(7)),
        },
        &input(),
        &cell,
    );
    assert_eq!(
        commit,
        Some(CellCommand::Commit {
            row_index: 2,
            column_name: "email".to_owned(),
            value: Some(json!(7)),
            target: CellWriteTarget::Legacy,
        })
    );
}

#[test]
fn crate_root_reexports_the_grid_cell_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::GridCellInput>();
    resolves::<crate::GridCellView>();
    resolves::<crate::FkCellAffordance>();
    resolves::<crate::CellEvent>();
    resolves::<crate::CellCommand>();
    assert_eq!(crate::CELL_TEST_ID, "data-table-cell");
    assert_eq!(crate::EDITOR_KIND_ENUM, "enum");
    assert!(crate::build_grid_cell_view(&input(), &text).attributes.contains_key("data-dt-row"));
}
