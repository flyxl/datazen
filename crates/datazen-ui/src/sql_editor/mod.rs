//! SQL editor shaped like CodeMirror 6, drawn by Rune.
//!
//! CodeMirror 6 keeps an immutable document and a selection (many ranges, one main). The
//! contenteditable node is only the input surface: plain characters arrive as key events, and
//! an IME composition is drawn at the main caret until it commits. This module does the same.
//! The document is a `String`. Rune does not edit it. `on_key` and `on_ime` build changes, and
//! the view paints lines, highlight spans and carets. The main caret is an `El::ime_cursor`,
//! which is what turns the platform input method on.

mod function_completion;
pub(crate) mod find_dock;
mod find_paint;
pub(crate) mod builtin_snippets;
mod from_completion;
pub(crate) mod goto_line;
mod highlight;
mod input;
mod literal_cursor;
mod line_comment;
mod manual_completion;
mod model;
mod navigation;
mod plain_find;
mod select_occurrences;
pub(crate) mod select_next_occurrence;
mod bracket_pairs;
mod syntax_theme;
mod view;
mod whole_word;
mod whole_word_data;

pub use highlight::highlight_sql;
pub(crate) use highlight::highlight_with_palette;
pub(crate) use syntax_theme::SyntaxPalette;
pub use input::{on_ime, on_key};
pub(crate) use input::on_query_key;
pub(crate) use manual_completion::is_manual_completion_key;
pub(crate) use model::CompletionScope;
pub use model::{Sel, Selection, SqlEditor};
pub use view::{editor_font, panel, plain_panel, EDITOR_SURFACE_WIDTH};
// T200: crate-visible row renderers — the workflow yaml editor mounts the
// same rows the plain panel renders.
pub use view::{line_row, split_lines};
pub(crate) use view::{panel_with_completion_scope, query_font_family};

#[cfg(test)]
mod tests;
