#[cfg(test)]
mod tests;

use crate::{normalize_cell_range, CellCoord, CellRange, GridSelection, GridSelectionMode};
use std::collections::BTreeSet;

pub const CLIPBOARD_DOM_GAP: [&str; 3] = [
    "native clipboard event",
    "clipboardData.setData/getData",
    "closest() on EventTarget",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomTarget {
    None,
    Plain,
    Input,
    Textarea,
    Select,
    ContentEditableTrue,
    EditingCell,
}

pub fn is_editable_target(chain: &[DomTarget]) -> bool {
    chain.iter().any(|target| {
        matches!(
            target,
            DomTarget::Input
                | DomTarget::Textarea
                | DomTarget::Select
                | DomTarget::ContentEditableTrue
                | DomTarget::EditingCell
        )
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridCopyTarget {
    Cell { range: CellRange },
    Row { row_indices: Vec<usize> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteAnchor {
    pub row_index: usize,
    pub column_name: String,
}

pub struct ClipboardBridge<'a> {
    pub build_copy_text: &'a dyn Fn(&GridCopyTarget) -> Option<String>,
    pub on_copied: &'a dyn Fn(&GridCopyTarget, bool),
    pub on_paste_text: &'a dyn Fn(&str, &PasteAnchor),
}

pub struct ClipboardDomInput<'a> {
    pub clipboard: Option<&'a ClipboardBridge<'a>>,
    pub editing_cell: Option<CellCoord>,
    pub enabled: bool,
    pub column_names: &'a [String],
    pub selection: &'a GridSelection,
    pub selected_rows: &'a BTreeSet<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardEvent {
    pub target_chain: Vec<DomTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteEvent {
    pub target_chain: Vec<DomTarget>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardBinding {
    pub copy_bound: bool,
    pub paste_bound: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyOutcome {
    pub prevented: bool,
    pub set_text: Option<String>,
    pub copied_reported: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteOutcome {
    pub prevented: bool,
    pub pasted: Option<(String, PasteAnchor)>,
}

fn normalize_selection_range(selection: &GridSelection, column_names: &[String]) -> Option<CellRange> {
    let anchor = selection.anchor.as_ref()?;
    let focus = selection.focus.as_ref()?;
    normalize_cell_range(
        &CellRange {
            start: anchor.clone(),
            end: focus.clone(),
        },
        column_names,
    )
}

pub fn resolve_copy_target(input: &ClipboardDomInput<'_>) -> Option<GridCopyTarget> {
    if !input.enabled {
        return None;
    }
    if input.selection.mode == GridSelectionMode::Cell {
        let range = normalize_selection_range(input.selection, input.column_names)?;
        return Some(GridCopyTarget::Cell { range });
    }
    if input.selection.mode == GridSelectionMode::Row && !input.selected_rows.is_empty() {
        return Some(GridCopyTarget::Row {
            row_indices: input.selected_rows.iter().copied().collect(),
        });
    }
    None
}

pub fn resolve_paste_anchor(input: &ClipboardDomInput<'_>) -> Option<PasteAnchor> {
    if !input.enabled || input.selection.mode != GridSelectionMode::Cell {
        return None;
    }
    let range = normalize_selection_range(input.selection, input.column_names)?;
    Some(PasteAnchor {
        row_index: range.start.row_index,
        column_name: range.start.column_name,
    })
}

pub fn bind_clipboard_dom(input: &ClipboardDomInput<'_>) -> ClipboardBinding {
    let bound = input.clipboard.is_some();
    ClipboardBinding {
        copy_bound: bound,
        paste_bound: bound,
    }
}

pub fn handle_copy(input: &ClipboardDomInput<'_>, event: &ClipboardEvent) -> CopyOutcome {
    let idle = CopyOutcome {
        prevented: false,
        set_text: None,
        copied_reported: None,
    };
    let clipboard = match input.clipboard {
        Some(clipboard) => clipboard,
        None => return idle,
    };
    if input.editing_cell.is_some() || is_editable_target(&event.target_chain) {
        return idle;
    }
    let target = match resolve_copy_target(input) {
        Some(target) => target,
        None => return idle,
    };
    let text = match (clipboard.build_copy_text)(&target) {
        Some(text) => text,
        None => return idle,
    };
    if text.is_empty() {
        (clipboard.on_copied)(&target, false);
        return CopyOutcome {
            prevented: true,
            set_text: None,
            copied_reported: Some(false),
        };
    }
    (clipboard.on_copied)(&target, true);
    CopyOutcome {
        prevented: true,
        set_text: Some(text),
        copied_reported: Some(true),
    }
}

pub fn handle_paste(input: &ClipboardDomInput<'_>, event: &PasteEvent) -> PasteOutcome {
    let idle = PasteOutcome {
        prevented: false,
        pasted: None,
    };
    let clipboard = match input.clipboard {
        Some(clipboard) => clipboard,
        None => return idle,
    };
    if input.editing_cell.is_some() || is_editable_target(&event.target_chain) {
        return idle;
    }
    let anchor = match resolve_paste_anchor(input) {
        Some(anchor) => anchor,
        None => return idle,
    };
    (clipboard.on_paste_text)(&event.text, &anchor);
    PasteOutcome {
        prevented: true,
        pasted: Some((event.text.clone(), anchor)),
    }
}
