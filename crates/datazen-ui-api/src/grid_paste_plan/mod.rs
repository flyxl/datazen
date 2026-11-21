mod clipboard;
mod plan;
mod tests;
mod types;

pub use clipboard::{
    clear_clipboard_source, clear_row_copy, normalize_clipboard_text, peek_clipboard_source,
    peek_row_copy, pending_cell_key, record_clipboard_source, record_row_copy,
    row_copy_for_clipboard_text, source_for_clipboard_text, table_change_key,
};
pub use plan::{column_writability, plan_paste};
pub use types::{
    CellPasteWrite, ClipboardSource, ColumnWritability, ColumnWritabilityReason, PasteAlignment,
    PasteOverflow, PastePlan, PastePlanOptions, RowCopyPayload,
};
