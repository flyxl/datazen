//! SQL editor shaped like CodeMirror 6, drawn by Rune.
//!
//! CodeMirror 6 keeps an immutable document and a selection (many ranges, one main). The
//! contenteditable node is only the input surface: plain characters arrive as key events, and
//! an IME composition is drawn at the main caret until it commits. This module does the same.
//! The document is a `String`. Rune does not edit it. `on_key` and `on_ime` build changes, and
//! the view paints lines, highlight spans and carets. The main caret is an `El::ime_cursor`,
//! which is what turns the platform input method on.

mod highlight;
mod input;
mod model;
mod view;

pub use highlight::highlight_sql;
pub use input::{on_ime, on_key};
pub use model::{Sel, Selection, SqlEditor};
pub use view::{editor_font, panel, plain_panel, EDITOR_SURFACE_WIDTH};

#[cfg(test)]
mod tests;
