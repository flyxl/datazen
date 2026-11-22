use super::types::{GutterClickAction, RowClickAction, RowKeyAction, RowModifiers};

pub fn row_click_action(
    index: usize,
    inside_cell: bool,
    modifiers: RowModifiers,
) -> RowClickAction {
    if inside_cell {
        RowClickAction::CellSelection { open_detail: index }
    } else {
        RowClickAction::SelectRow { index, modifiers }
    }
}

pub fn row_key_down_action(
    cell_selection_enabled: bool,
    key: &str,
    index: usize,
) -> RowKeyAction {
    if cell_selection_enabled {
        return RowKeyAction::None;
    }
    if key == "Enter" || key == " " {
        RowKeyAction::SelectRow { index }
    } else {
        RowKeyAction::None
    }
}

pub fn row_double_click_action(column_names: &[String]) -> Option<&str> {
    column_names.first().map(String::as_str)
}

pub fn gutter_click_action(
    index: usize,
    modifiers: RowModifiers,
    has_gutter_handler: bool,
) -> GutterClickAction {
    if has_gutter_handler {
        GutterClickAction::Gutter { index, modifiers }
    } else {
        GutterClickAction::Row { index, modifiers }
    }
}

pub fn modifiers_from_flags(meta_or_ctrl: bool, shift: bool) -> RowModifiers {
    RowModifiers { multi: meta_or_ctrl, range: shift }
}
