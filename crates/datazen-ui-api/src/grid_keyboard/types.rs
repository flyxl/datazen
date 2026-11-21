use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellCoord {
    pub row_index: usize,
    pub column_name: String,
}

impl CellCoord {
    pub fn new(row_index: usize, column_name: &str) -> Self {
        Self { row_index, column_name: column_name.to_owned() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridSelectionMode {
    #[default]
    None,
    Cell,
    Row,
}

impl GridSelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cell => "cell",
            Self::Row => "row",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSelection {
    pub anchor: Option<CellCoord>,
    pub focus: Option<CellCoord>,
    pub extra_ranges: Vec<CellRange>,
    pub mode: GridSelectionMode,
}

impl GridSelection {
    pub fn empty() -> Self {
        Self::default()
    }
}

pub static EMPTY_ROW_MODE_SELECTION: LazyLock<GridSelection> = LazyLock::new(|| GridSelection {
    anchor: None,
    focus: None,
    extra_ranges: Vec::new(),
    mode: GridSelectionMode::Row,
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridKeyEventLike {
    pub key: String,
    pub meta_key: bool,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub is_composing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridNativeKeyEventLike {
    pub key: String,
    pub meta_key: bool,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub is_composing: bool,
    pub key_code: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridPageDirection {
    Up,
    Down,
}

impl GridPageDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridEdge {
    RowStart,
    RowEnd,
    GridStart,
    GridEnd,
}

impl GridEdge {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RowStart => "row-start",
            Self::RowEnd => "row-end",
            Self::GridStart => "grid-start",
            Self::GridEnd => "grid-end",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridKeyIntent {
    Move { delta_row: i64, delta_column: i64 },
    MovePage { direction: GridPageDirection },
    MoveEdge { edge: GridEdge },
    SelectAllRows,
    BeginEdit,
    LeaveGrid,
    Ignored,
    Pass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridKeyboardContext {
    pub column_order: Vec<String>,
    pub row_count: usize,
    pub mode: GridSelectionMode,
    pub cursor: Option<CellCoord>,
    pub viewport_row_count: usize,
    pub editing: bool,
    pub extend: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridKeyCommand {
    SetSelection { selection: GridSelection },
    RowSelect { row_index: usize, multi: bool, range: bool },
    SelectAllRows,
    BeginEdit { coord: CellCoord },
    Consume,
    Ignored,
    Pass,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_modes_and_edges_keep_their_literals() {
        assert_eq!(GridSelectionMode::None.as_str(), "none");
        assert_eq!(GridSelectionMode::Cell.as_str(), "cell");
        assert_eq!(GridSelectionMode::Row.as_str(), "row");
        assert_eq!(GridPageDirection::Up.as_str(), "up");
        assert_eq!(GridPageDirection::Down.as_str(), "down");
        assert_eq!(GridEdge::RowStart.as_str(), "row-start");
        assert_eq!(GridEdge::RowEnd.as_str(), "row-end");
        assert_eq!(GridEdge::GridStart.as_str(), "grid-start");
        assert_eq!(GridEdge::GridEnd.as_str(), "grid-end");
    }

    #[test]
    fn empty_row_mode_selection_has_no_geometry() {
        assert_eq!(EMPTY_ROW_MODE_SELECTION.mode, GridSelectionMode::Row);
        assert_eq!(EMPTY_ROW_MODE_SELECTION.anchor, None);
        assert_eq!(EMPTY_ROW_MODE_SELECTION.focus, None);
        assert!(EMPTY_ROW_MODE_SELECTION.extra_ranges.is_empty());
    }

    #[test]
    fn the_default_selection_is_none_empty() {
        let selection = GridSelection::empty();
        assert_eq!(selection.mode, GridSelectionMode::None);
        assert_eq!(selection.anchor, None);
        assert_eq!(selection.focus, None);
    }
}
