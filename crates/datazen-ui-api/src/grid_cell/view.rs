use super::types::*;
use super::FkTextFn;
use serde_json::Value;
use std::collections::BTreeMap;

pub fn build_grid_cell_view(input: &GridCellInput, text: &FkTextFn<'_>) -> GridCellView {
    let fk_title = input.fk.as_ref().map(|fk| {
        if fk.resolving {
            text(FkTextKind::Resolving, "")
        } else {
            text(FkTextKind::Jump, &fk.target_label)
        }
    });
    let pick_title = input
        .fk
        .as_ref()
        .map(|fk| text(FkTextKind::Selector, &fk.target_label));
    let read_only_reason = input
        .editor
        .as_ref()
        .and_then(|editor| editor.read_only_reason.clone())
        .or_else(|| {
            input
                .column_info
                .as_ref()
                .and_then(|info| info.read_only_reason.clone())
        });
    let nullable = input
        .column_info
        .as_ref()
        .map(|info| info.nullable)
        .unwrap_or(true);
    let integer_only = input
        .column_info
        .as_ref()
        .map(|info| info.integer_only)
        .unwrap_or(false);
    let is_null = input.value.is_none() || input.value.as_ref() == Some(&Value::Null);

    let mut attributes = BTreeMap::new();
    attributes.insert("data-testid".to_owned(), CELL_TEST_ID.to_owned());
    attributes.insert("data-dt-row".to_owned(), input.row_index.to_string());
    attributes.insert("data-dt-col".to_owned(), input.column_name.clone());
    if input.selected {
        attributes.insert("data-dt-selected".to_owned(), "true".to_owned());
    }
    if input.active {
        attributes.insert("data-dt-active".to_owned(), "true".to_owned());
    }
    if let Some(cell_type) = &input.cell_type {
        attributes.insert("data-dt-cell-type".to_owned(), cell_type.clone());
    }
    if input.editing {
        attributes.insert("data-dt-editing".to_owned(), "true".to_owned());
    }
    if is_null {
        attributes.insert("data-dt-null".to_owned(), "true".to_owned());
    }
    if let Some(reason) = &read_only_reason {
        attributes.insert("data-dt-readonly".to_owned(), reason.clone());
    }
    if input.dirty {
        attributes.insert("data-dt-dirty".to_owned(), "true".to_owned());
    }
    if input.error {
        attributes.insert("data-dt-cell-error".to_owned(), "true".to_owned());
    }
    if let Some(fk) = &input.fk {
        attributes.insert("data-dt-fk".to_owned(), fk.target_label.clone());
    }

    let mut classes = vec![CLASS_BASE.to_owned()];
    if input.dirty {
        classes.push(CLASS_DIRTY_BORDER.to_owned());
    }
    if input.dirty && !input.selected {
        classes.push(CLASS_DIRTY_BG.to_owned());
    }
    if input.sticky.is_some() {
        classes.push(CLASS_STICKY.to_owned());
    }

    let sticky_style = input.sticky.as_ref().map(|sticky| StickyStyle {
        left: sticky.left,
        z_index: 2,
    });
    let mut children = Vec::new();
    if let Some(sticky) = &input.sticky {
        children.push(CellChild::StickyOverlay {
            classes: vec![
                CLASS_STICKY_OVERLAY.to_owned(),
                sticky.bg_class.clone(),
            ],
        });
    }

    let pickable = input
        .fk
        .as_ref()
        .and_then(|fk| fk.pickable)
        .unwrap_or(false);
    children.push(CellChild::Renderer(CellRendererView {
        column_name: input.column_name.clone(),
        data_type: input.column_type.clone(),
        value: input.value.clone(),
        is_editing: input.editing,
        cell_type: input.cell_type.clone(),
        nullable,
        integer_only,
        panel_id: input.editor.as_ref().and_then(|editor| editor.panel_id.clone()),
        has_load_candidates: input
            .editor
            .as_ref()
            .map(|editor| editor.has_load_candidates)
            .unwrap_or(false),
        editor_kind_override: if input.editing && pickable {
            Some(EDITOR_KIND_ENUM)
        } else {
            None
        },
        presentation_plan: input.presentation_plan.clone(),
    }));

    if let Some(fk) = &input.fk {
        if fk.navigable && !input.editing {
            children.push(CellChild::FkJump(FkButtonView {
                test_marker: "data-dt-fk-click",
                title: fk_title.clone(),
                disabled: fk.resolving,
                classes: vec![CLASS_FK_JUMP.to_owned()],
                resolving: fk.resolving,
                pick: false,
            }));
        }
        if pickable && !input.editing {
            children.push(CellChild::FkPick(FkButtonView {
                test_marker: "data-dt-fk-pick",
                title: pick_title,
                disabled: false,
                classes: vec![CLASS_FK_PICK.to_owned()],
                resolving: false,
                pick: true,
            }));
        }
    }

    if input.dirty {
        children.push(CellChild::DirtyMark {
            classes: vec![CLASS_DIRTY_MARK.to_owned()],
        });
    }

    let write_target = if input
        .editor
        .as_ref()
        .map(|editor| editor.has_value_handler)
        .unwrap_or(false)
    {
        CellWriteTarget::Editor
    } else {
        CellWriteTarget::Legacy
    };

    GridCellView {
        id: input.id.clone(),
        role: if input.grid_semantics {
            Some(ROLE_GRIDCELL)
        } else {
            None
        },
        aria_selected: if input.grid_semantics && input.selected {
            Some("true")
        } else {
            None
        },
        aria_label: if input.dirty {
            input.dirty_aria_label.clone()
        } else {
            fk_title.clone()
        },
        title: if input.dirty {
            input.dirty_tooltip.clone()
        } else {
            fk_title
        },
        attributes,
        classes,
        width: input.width,
        sticky_style,
        children,
        write_target,
    }
}
