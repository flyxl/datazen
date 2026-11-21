mod fk_selector_text;
mod grid_clipboard_bridge;
mod row_height;

#[cfg(test)]
mod tests;

pub use fk_selector_text::{
    fk_selector_label, fk_text_with_selector, FkSelectorTranslator, FK_SELECTOR_PICK_FROM_KEY,
};
pub use grid_clipboard_bridge::{
    bridge_availability, ClipboardCellCoord, GridClipboardHandlers, GridClipboardRange,
    GridCopyTarget,
};
pub use row_height::{density_row_height_delta, row_height_for_density, DATA_TABLE_ROW_HEIGHT};

pub const DATA_TABLE_BITS_SURFACE: [&str; 10] = [
    "fkSelectorLabel",
    "FkSelectorTranslator",
    "GridCopyTarget",
    "GridClipboardHandlers",
    "ClipboardCellCoord",
    "DATA_TABLE_ROW_HEIGHT",
    "rowHeightForDensity",
    "GridDensity",
    "gridClipboardBridge",
    "DATA_TABLE_BITS_GAP",
];

pub const DATA_TABLE_BITS_GAP: [&str; 6] = [
    "the standalone t() locale lookup behind FkSelectorTranslator",
    "FkTextKind Jump/Resolving labels owned by the grid_cell port",
    "native copy/paste events, navigator.clipboard and preventDefault",
    "useGridClipboard and the DataTable owner that hold the copy timer",
    "optional onPasteFromClipboard binding is a no-op trait default here",
    "row-index to scroll-offset conversion in foreign-key navigation",
];
