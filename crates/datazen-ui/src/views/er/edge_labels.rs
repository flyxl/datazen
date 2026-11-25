//! The label-fitting half of `interactionState.ts` (T564 split).
//!
//! `fitEdgeLabel` and its helpers live in their own module because the graph
//! builder consumes them while routing handles, while the hover transforms
//! stay in `interaction_state.rs` — the React file exports both families to
//! both consumers, and a Rust split along the same seam keeps each side
//! independently testable.

/// Rough width of one character at the 10px label size.
const LABEL_CHAR_WIDTH: f32 = 6.0;
/// Padding either side of the text.
const LABEL_PADDING: f32 = 12.0;

/// Whether a label has room to be drawn.
///
/// A label is centred on the edge, so it needs the horizontal gap between the
/// two nodes to be at least as wide as the text. An absent or empty label
/// never "fits" — callers use that to skip drawing entirely.
pub fn label_fits(label: Option<&str>, gap: f32) -> bool {
    let Some(label) = label else {
        return false;
    };
    if label.is_empty() {
        return false;
    }
    label.chars().count() as f32 * LABEL_CHAR_WIDTH + LABEL_PADDING <= gap
}

/// Shorten `text` to the longest form that fits in `gap`, ellipsis included.
///
/// The ellipsis occupies a character slot of its own, so the budget is spent
/// on `max_chars - 1` characters of text rather than on `max_chars`. Not even
/// an ellipsis fits; draw nothing rather than overflow the gap.
fn truncate_to_gap(text: &str, gap: f32) -> String {
    let max_chars = ((gap - LABEL_PADDING) / LABEL_CHAR_WIDTH).floor() as i32;
    if max_chars < 1 {
        return String::new();
    }
    let max_chars = max_chars as usize;
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    if max_chars == 1 {
        return "…".to_owned();
    }
    let mut truncated: String = text.chars().take(max_chars - 1).collect();
    truncated.push('…');
    truncated
}

/// Fit a relationship's column list into the space the edge actually has.
///
/// Measured against the layered layout, a gap of 110px holds about 16
/// characters — enough for every single-column foreign key, but not for a
/// composite one (`tenant_id, account_id` needs 138px). Rather than let such
/// a label overlap the nodes on either side, or drop it and lose the
/// relationship's meaning, it is shortened to the column the edge attaches to
/// plus a count of the rest.
pub fn fit_edge_label(columns: &[String], gap: f32) -> String {
    if columns.is_empty() {
        return String::new();
    }
    let full = columns.join(", ");
    if label_fits(Some(&full), gap) {
        return full;
    }

    let first = columns[0].clone();
    let summarised = if columns.len() > 1 {
        format!("{first} +{}", columns.len() - 1)
    } else {
        first
    };
    if label_fits(Some(&summarised), gap) {
        return summarised;
    }

    truncate_to_gap(&summarised, gap)
}
