//! Single-line text fields.
//!
//! Rune lays a field's line out from the top of its content box, and its
//! `center_y` only aligns children, so a field built with a fixed height by hand
//! leaves the text and caret pinned to the top edge. Every single-line field in
//! the interface goes through [`single_line_field`], which gives the surplus
//! height back as symmetric padding: the line sits in the middle and the field
//! keeps its full click target.

use rune::tok::TextStyle;
use rune::{text_input, El, TextEdit};

/// Vertical padding that centres one line of `style` inside a `height`-tall field.
///
/// Exposed for fields that cannot use [`single_line_field`] (for example a
/// padded control that owns its own chrome) and for the geometry test.
pub(crate) fn centered_pad(style: TextStyle, height: f32) -> f32 {
    ((height - style.line_px()) / 2.0).max(0.0)
}

/// A single-line text field `height` points tall whose text sits in the middle.
///
/// The rest of the field's look (width, radius, fill, border, colour, placeholder,
/// handlers) stays with the caller, which chains it onto the returned element.
pub(crate) fn single_line_field(
    key: impl Into<String>,
    edit: &TextEdit,
    style: TextStyle,
    height: f32,
) -> El {
    text_input(edit, style)
        .key(key.into())
        .h(height)
        .py(centered_pad(style, height))
        .single_line()
}
