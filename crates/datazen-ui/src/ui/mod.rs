//! Reusable UI primitives for DataZen's Rune interface.
//!
//! Thin wrappers over `rune` primitives and shared theme/tokens, so feature
//! views (for example the connection navigator) compose consistent controls
//! instead of ad-hoc styling.

pub(crate) mod nav;
pub(crate) mod theme;

/// Vertical padding that centres one line of `style` inside a `height`-tall field.
///
/// Rune lays a single line of text out from the content box's top edge, so a
/// field taller than its line leaves the text (and the caret) pinned to the top
/// instead of centred. Handing the surplus back as symmetric padding centres it
/// while keeping the whole field clickable, unlike shrinking the field to the
/// line height.
pub(crate) fn centered_field_pad(style: rune::tok::TextStyle, height: f32) -> f32 {
    ((height - style.line_px()) / 2.0).max(0.0)
}
