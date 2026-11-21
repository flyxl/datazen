mod parse;
mod selection;
mod serialize;
mod tests;
mod types;

pub use parse::{parse_clipboard_matrix, sniff_clipboard_delimiter, utf8_byte_length};
pub use selection::{build_rows_matrix, build_selection_matrix, to_clipboard_range};
pub use serialize::{
    escape_csv_field, escape_html_cell_text, escape_markdown_cell_text_value,
    escape_markdown_field, serialize_clipboard_cells,
};
pub use types::{
    ClipboardCellCoord, ClipboardDelimiter, ClipboardFormat, ClipboardMatrix, ClipboardParseResult,
    GridClipboardAnchor, GridClipboardRange, PasteParseErrorCode, PasteParseRefusal, PasteProblem, PasteProblemCode,
    SerializeCellsOptions, GRID_PASTE_CHUNK_CELLS, GRID_PASTE_CONFIRM_CELLS, GRID_PASTE_MAX_BYTES,
    GRID_PASTE_MAX_CELLS,
};
