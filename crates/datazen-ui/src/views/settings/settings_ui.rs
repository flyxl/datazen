//! Port of `packages/frontend/src/windows/settings/settingsUi.tsx`
//! (T399).
//!
//! The settings subtree's shared primitives: [`section_title`],
//! [`setting_row`] and [`toggle_row`] — the three exports the React
//! file hands every settings section.
//!
//! ## What maps where
//!
//! - `SectionTitle` (the `text-[13px] font-semibold uppercase
//!   tracking-wider text-fg-muted` heading plus its optional
//!   [`crate::views::settings::setting_hint::SettingHint`]) is
//!   [`section_title`]; the hint's label is the title, exactly as the
//!   React call sites pass it.
//! - `SettingRow` (the 16pt gutter, the 160pt `w-40` label column with
//!   its 36pt `min-h-9`, the `flex-1` content) is [`setting_row`].
//! - `ToggleRow` (label left, the hand-rolled pill switch right) is
//!   [`toggle_row`]: a 36×20pt track, a 14pt knob at x=3/x=18, the
//!   accent track when checked and the opaque `bg-edge` track when
//!   not, half-opacity when disabled — the React literal's geometry,
//!   the [`crate::ui::palette::edge_solid`] mapping the wizard dots
//!   established for `bg-edge`.
//!
//! The landed sections' local `section_title` stand-ins (T172's
//! shell, T222's appearance, T244's cleanup, …) predate this module
//! and keep their audits; new sections build on this one, and the
//! stand-ins retire as their sections are touched.
//!
//! Known deltas: `uppercase tracking-wider` has no Rune text
//! transform — the headings arrive already-cased from the callers'
//! message catalogs (the stand-ins' own documented delta); the knob's
//! `shadow` is a CSS paint flourish with no layout effect.

use std::rc::Rc;

use rune::{col, row, text, tok, Color, El, TextStyle};
use rune_kit::Kit;

use crate::ui::palette;
use crate::views::settings::setting_hint::SettingHint;

/// `text-sm` (14/400) — the rows' label ink size.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
/// `text-sm leading-5` — the SettingRow label's 20pt line box.
const LABEL_MIN_HEIGHT: f32 = 36.0;
/// `w-40` — the label column.
const LABEL_COLUMN_WIDTH: f32 = 160.0;
/// `gap-4` — the row gutter.
const ROW_GAP: f32 = 16.0;
/// The switch track (`h-5 w-9`).
const SWITCH_TRACK: (f32, f32) = (36.0, 20.0);
/// The switch knob (`h-3.5 w-3.5`).
const SWITCH_KNOB: f32 = 14.0;
/// The knob's two x positions (`translate-x-[3px]` / `[18px]`).
const SWITCH_KNOB_X: (f32, f32) = (3.0, 18.0);

/// `SectionTitle({ children, hint })`: the section heading with its
/// optional "?" hint.
pub fn section_title(
    k: &Kit,
    layer: &mut Vec<El>,
    key: String,
    title: &str,
    hint: Option<&str>,
) -> El {
    let mut heading = row()
        .key(key.clone())
        .w_full()
        .gap(4.0)
        .center_y()
        .child(text(title.to_owned()).font(tok::type_::TITLE).color(k.c.text_muted));
    if let Some(hint) = hint {
        heading = heading.child(
            SettingHint::new(format!("{key}-hint"), title.to_owned(), hint.to_owned())
                .build(k, layer),
        );
    }
    heading
}

/// `SettingRow({ label, hint, children })`: the two-column row — the
/// 160pt label column (with its optional hint) and the `flex-1`
/// content.
pub fn setting_row(
    k: &Kit,
    layer: &mut Vec<El>,
    key: String,
    label: &str,
    hint: Option<&str>,
    child: El,
) -> El {
    let mut label_col = row()
        .key(format!("{key}-label"))
        .min_h(LABEL_MIN_HEIGHT)
        .w(LABEL_COLUMN_WIDTH)
        .gap(4.0)
        .center_y()
        .child(text(label.to_owned()).font(TEXT_SM).color(k.c.text_muted)); // text-fg-secondary
    if let Some(hint) = hint {
        label_col = label_col.child(
            SettingHint::new(format!("{key}-hint"), label.to_owned(), hint.to_owned()).build(k, layer),
        );
    }
    row()
        .key(key.clone())
        .gap(ROW_GAP)
        .items_start()
        .child(label_col)
        .child(col().key(format!("{key}-content")).min_w(0.0).grow(1.0).child(child))
}

/// `ToggleRow`'s switch payload.
pub struct ToggleRow<'a> {
    /// The row key (the React `testId`: a stable hook for tests; the
    /// accessible name is translated, so it cannot be one).
    pub key: String,
    pub label: &'a str,
    pub hint: Option<&'a str>,
    pub checked: bool,
    /// A row can be unavailable while still displaying its current
    /// value.
    pub disabled: bool,
}

/// `ToggleRow({ label, hint, checked, onChange, testId, disabled })`:
/// the label left, the pill switch right. `on_change` fires only
/// when enabled (the React click guard), with the **next** value.
pub fn toggle_row<S: 'static>(
    k: &Kit,
    layer: &mut Vec<El>,
    spec: ToggleRow<'_>,
    on_change: Rc<dyn Fn(&mut S, bool)>,
) -> El {
    let mut label_col = row()
        .gap(4.0)
        .center_y()
        .child(text(spec.label.to_owned()).font(TEXT_SM).color(k.c.text_muted)); // text-fg-secondary
    if let Some(hint) = spec.hint {
        label_col = label_col.child(
            SettingHint::new(
                format!("{}-hint", spec.key),
                spec.label.to_owned(),
                hint.to_owned(),
            )
            .build(k, layer),
        );
    }

    // The pill: accent when checked, the opaque edge fill when not,
    // half-opacity when disabled (`cursor-not-allowed opacity-50`).
    let alpha = if spec.disabled { 0.5 } else { 1.0 };
    let track = if spec.checked { k.c.bg_accent } else { palette::edge_solid(k.dark) };
    let track = track.alpha(alpha);
    let knob_x = if spec.checked { SWITCH_KNOB_X.1 } else { SWITCH_KNOB_X.0 };

    let mut switch = row()
        .key(format!("{}-switch", spec.key))
        .size(SWITCH_TRACK.0, SWITCH_TRACK.1)
        .radius(SWITCH_TRACK.1 / 2.0)
        .fill(track)
        .pl(knob_x)
        .center_y()
        .child(
            col()
                .key(format!("{}-knob", spec.key))
                .size(SWITCH_KNOB, SWITCH_KNOB)
                .radius(SWITCH_KNOB / 2.0)
                .fill(Color::WHITE.alpha(alpha))
                .center(),
        );
    if !spec.disabled {
        let checked = spec.checked;
        switch = switch.on_press(move |s: &mut S| on_change(s, !checked));
    }

    row()
        .key(spec.key)
        .w_full()
        .gap(ROW_GAP)
        .justify_between()
        .center_y()
        .child(label_col)
        .child(switch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_geometry_is_the_react_literal() {
        // gap-4, w-40, min-h-9, the 36×20 track, the 14pt knob, the
        // 3/18 knob travel.
        assert_eq!(ROW_GAP, 16.0);
        assert_eq!(LABEL_COLUMN_WIDTH, 160.0);
        assert_eq!(LABEL_MIN_HEIGHT, 36.0);
        assert_eq!(SWITCH_TRACK, (36.0, 20.0));
        assert_eq!(SWITCH_KNOB, 14.0);
        assert_eq!(SWITCH_KNOB_X, (3.0, 18.0));
        assert_eq!(TEXT_SM, TextStyle::new(14.0, 400.0));
    }

    #[test]
    fn the_knob_x_follows_the_checked_state() {
        assert_eq!(SWITCH_KNOB_X.0, 3.0, "unchecked rests at 3");
        assert_eq!(SWITCH_KNOB_X.1, 18.0, "checked travels to 18");
        // 36 - 14 - 18 = 4pt of travel room on the far side: the knob
        // stays inside the track at both ends.
        assert!(SWITCH_KNOB_X.1 + SWITCH_KNOB <= SWITCH_TRACK.0);
        assert!(SWITCH_KNOB_X.0 + SWITCH_KNOB <= SWITCH_TRACK.0);
    }
}
