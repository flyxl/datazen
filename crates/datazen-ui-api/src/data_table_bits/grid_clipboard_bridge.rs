#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCellCoord {
    pub row_index: u64,
    pub column_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridClipboardRange {
    pub top_row: u64,
    pub bottom_row: u64,
    pub left_column: String,
    pub right_column: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridCopyTarget {
    Row { row_indices: Vec<u64> },
    Cell { range: GridClipboardRange },
}

impl GridCopyTarget {
    pub fn mode(&self) -> &'static str {
        match self {
            Self::Row { .. } => "row",
            Self::Cell { .. } => "cell",
        }
    }

    pub fn row_target(row_indices: Vec<u64>) -> Self {
        Self::Row { row_indices }
    }

    pub fn cell_target(range: GridClipboardRange) -> Self {
        Self::Cell { range }
    }
}

pub trait GridClipboardHandlers {
    fn build_copy_text(&self, target: &GridCopyTarget) -> Option<String>;

    fn on_copied(&self, target: &GridCopyTarget, ok: bool);

    fn on_paste_text(&self, text: &str, anchor: &ClipboardCellCoord) -> bool;

    fn on_paste_from_clipboard(&self, anchor: &ClipboardCellCoord) -> bool {
        let _ = anchor;
        false
    }

    fn can_copy(&self) -> bool;

    fn can_paste(&self) -> bool;
}

pub fn bridge_availability(handlers: &dyn GridClipboardHandlers) -> (bool, bool) {
    (handlers.can_copy(), handlers.can_paste())
}
