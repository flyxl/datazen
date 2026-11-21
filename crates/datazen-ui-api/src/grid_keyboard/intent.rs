use crate::grid_keyboard::types::{
    GridEdge, GridKeyEventLike, GridKeyboardContext, GridKeyIntent, GridNativeKeyEventLike,
    GridPageDirection,
};

pub fn to_grid_key_event_like(event: &GridNativeKeyEventLike) -> GridKeyEventLike {
    GridKeyEventLike {
        key: event.key.to_lowercase(),
        meta_key: event.meta_key,
        ctrl_key: event.ctrl_key,
        shift_key: event.shift_key,
        alt_key: event.alt_key,
        is_composing: event.is_composing || event.key_code == Some(229),
    }
}

pub fn is_ime_composing_key(event: &GridKeyEventLike) -> bool {
    event.is_composing || event.key == "process"
}

fn is_mod_key(event: &GridKeyEventLike) -> bool {
    event.meta_key || event.ctrl_key
}

pub fn read_grid_key_intent(
    event: &GridKeyEventLike,
    context: &GridKeyboardContext,
) -> GridKeyIntent {
    if is_ime_composing_key(event) {
        return GridKeyIntent::Pass;
    }
    if context.editing {
        return GridKeyIntent::Pass;
    }
    if event.key == "escape" {
        return GridKeyIntent::Pass;
    }
    if is_mod_key(event)
        && matches!(event.key.as_str(), "c" | "x" | "v" | "z")
    {
        return GridKeyIntent::Pass;
    }
    if event.alt_key {
        return GridKeyIntent::Pass;
    }

    let mod_key = is_mod_key(event);
    if mod_key && matches!(event.key.as_str(), "arrowleft" | "arrowright") {
        return GridKeyIntent::MoveEdge {
            edge: if event.key == "arrowleft" { GridEdge::RowStart } else { GridEdge::RowEnd },
        };
    }
    match event.key.as_str() {
        "arrowup" => GridKeyIntent::Move { delta_row: -1, delta_column: 0 },
        "arrowdown" => GridKeyIntent::Move { delta_row: 1, delta_column: 0 },
        "arrowleft" => GridKeyIntent::Move { delta_row: 0, delta_column: -1 },
        "arrowright" => GridKeyIntent::Move { delta_row: 0, delta_column: 1 },
        "home" => GridKeyIntent::MoveEdge {
            edge: if mod_key { GridEdge::GridStart } else { GridEdge::RowStart },
        },
        "end" => GridKeyIntent::MoveEdge {
            edge: if mod_key { GridEdge::GridEnd } else { GridEdge::RowEnd },
        },
        "pageup" => GridKeyIntent::MovePage { direction: GridPageDirection::Up },
        "pagedown" => GridKeyIntent::MovePage { direction: GridPageDirection::Down },
        "tab" => GridKeyIntent::LeaveGrid,
        "enter" | "f2" => {
            if mod_key {
                GridKeyIntent::Pass
            } else if context.mode == crate::grid_keyboard::types::GridSelectionMode::None {
                GridKeyIntent::Ignored
            } else {
                GridKeyIntent::BeginEdit
            }
        }
        "a" if mod_key => GridKeyIntent::SelectAllRows,
        _ => GridKeyIntent::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_keyboard::types::{GridKeyEventLike, GridSelectionMode};

    fn context() -> GridKeyboardContext {
        GridKeyboardContext {
            column_order: vec!["id".to_owned()],
            row_count: 2,
            mode: GridSelectionMode::Cell,
            cursor: None,
            viewport_row_count: 5,
            editing: false,
            extend: false,
        }
    }

    fn key(name: &str) -> GridKeyEventLike {
        GridKeyEventLike {
            key: name.to_owned(),
            meta_key: false,
            ctrl_key: false,
            shift_key: false,
            alt_key: false,
            is_composing: false,
        }
    }

    #[test]
    fn native_projection_lowercases_and_detects_legacy_ime() {
        let event = GridNativeKeyEventLike {
            key: "ArrowDown".to_owned(),
            meta_key: false,
            ctrl_key: false,
            shift_key: true,
            alt_key: false,
            is_composing: false,
            key_code: Some(229),
        };
        let projected = to_grid_key_event_like(&event);
        assert_eq!(projected.key, "arrowdown");
        assert!(projected.shift_key);
        assert!(projected.is_composing);
        assert!(is_ime_composing_key(&projected));
    }

    #[test]
    fn composition_and_process_pass_before_everything_else() {
        let mut ctx = context();
        ctx.editing = true;
        let mut event = key("process");
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass);
        event = key("arrowdown");
        event.is_composing = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass);
    }

    #[test]
    fn an_open_editor_and_escape_pass() {
        let mut ctx = context();
        ctx.editing = true;
        assert_eq!(read_grid_key_intent(&key("arrowdown"), &ctx), GridKeyIntent::Pass);
        ctx.editing = false;
        assert_eq!(read_grid_key_intent(&key("escape"), &ctx), GridKeyIntent::Pass);
    }

    #[test]
    fn clipboard_and_undo_combinations_pass() {
        let mut ctx = context();
        for name in ["c", "x", "v", "z"] {
            let mut event = key(name);
            event.ctrl_key = true;
            assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass, "{name}");
        }
        let mut event = key("c");
        event.meta_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass);
        let mut event = key("a");
        event.alt_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass);
    }

    #[test]
    fn arrows_and_edges_use_the_priority_table() {
        let ctx = context();
        assert_eq!(
            read_grid_key_intent(&key("arrowleft"), &ctx),
            GridKeyIntent::Move { delta_row: 0, delta_column: -1 }
        );
        let mut event = key("arrowleft");
        event.meta_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::MoveEdge { edge: GridEdge::RowStart });
        event.key = "arrowright".to_owned();
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::MoveEdge { edge: GridEdge::RowEnd });
        assert_eq!(
            read_grid_key_intent(&key("home"), &ctx),
            GridKeyIntent::MoveEdge { edge: GridEdge::RowStart }
        );
        assert_eq!(
            read_grid_key_intent(&key("end"), &ctx),
            GridKeyIntent::MoveEdge { edge: GridEdge::RowEnd }
        );
    }

    #[test]
    fn modified_home_end_page_and_tab() {
        let ctx = context();
        let mut event = key("home");
        event.ctrl_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::MoveEdge { edge: GridEdge::GridStart });
        let mut event = key("end");
        event.ctrl_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::MoveEdge { edge: GridEdge::GridEnd });
        assert_eq!(
            read_grid_key_intent(&key("pageup"), &ctx),
            GridKeyIntent::MovePage { direction: GridPageDirection::Up }
        );
        assert_eq!(
            read_grid_key_intent(&key("pagedown"), &ctx),
            GridKeyIntent::MovePage { direction: GridPageDirection::Down }
        );
        assert_eq!(read_grid_key_intent(&key("tab"), &ctx), GridKeyIntent::LeaveGrid);
    }

    #[test]
    fn edit_select_all_and_unclaimed_keys() {
        let ctx = context();
        assert_eq!(read_grid_key_intent(&key("f2"), &ctx), GridKeyIntent::BeginEdit);
        assert_eq!(read_grid_key_intent(&key("enter"), &ctx), GridKeyIntent::BeginEdit);
        let mut event = key("enter");
        event.meta_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::Pass);
        let mut event = key("a");
        event.ctrl_key = true;
        assert_eq!(read_grid_key_intent(&event, &ctx), GridKeyIntent::SelectAllRows);
        assert_eq!(read_grid_key_intent(&key("a"), &ctx), GridKeyIntent::Pass);
        assert_eq!(read_grid_key_intent(&key(" "), &ctx), GridKeyIntent::Pass);
        assert_eq!(read_grid_key_intent(&key("delete"), &ctx), GridKeyIntent::Pass);
    }

    #[test]
    fn none_mode_enter_is_ignored() {
        let ctx = GridKeyboardContext { mode: GridSelectionMode::None, ..context() };
        assert_eq!(read_grid_key_intent(&key("enter"), &ctx), GridKeyIntent::Ignored);
    }
}
