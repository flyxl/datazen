//! Port of `packages/frontend/src/components/export/ExportSuccessPanel.tsx` (T014).
//!
//! The React component is a small centred pane a single-table export shows on
//! completion: a 48pt green wash circle carrying a 24pt check glyph, then the
//! caller's message line. Every colour comes from `kit(dark)` — React's
//! `bg-green-500/15` / `text-green-400` map to the kit's `success` state — and
//! the glyph is the registry's `ICON_CHECK`, never a new SVG literal.
//!
//! ## React → Rune
//!
//! | React                                   | Rune                                   |
//! |-----------------------------------------|----------------------------------------|
//! | `flex flex-col items-center gap-2 py-6`  | `col().items_center().gap(8).py(24)`   |
//! | `h-12 w-12 rounded-full` + `bg-green/15` | 48pt row, radius 999, `success.fg` /15 |
//! | `h-6 w-6` check (`stroke-width 2.5`)     | `ICON_CHECK` at 24pt                   |
//! | `text-sm font-medium text-fg`            | `k.ty.body_medium` + `k.c.text_base`   |
//!
//! ## Keys
//!
//! | part    | key                            |
//! |---------|--------------------------------|
//! | root    | `export-success-panel`         |
//! | badge   | `export-success-panel-badge`   |
//! | icon    | `export-success-panel-icon`    |
//! | message | `export-success-panel-message` |

#![allow(dead_code)] // the export view (later track) is the production caller

use rune::{col, icon, row, text, Align, El};

use crate::icons::ICON_CHECK;
use crate::ui::kit::kit;

/// Root pane, React's `flex flex-col items-center gap-2 py-6 text-center`.
pub(crate) const PANEL_KEY: &str = "export-success-panel";
/// The green circle, React's `h-12 w-12 rounded-full bg-green-500/15`.
pub(crate) const BADGE_KEY: &str = "export-success-panel-badge";
/// The check glyph, React's `h-6 w-6` inline svg.
pub(crate) const ICON_KEY: &str = "export-success-panel-icon";
/// The message line, React's `text-sm font-medium text-fg`.
pub(crate) const MESSAGE_KEY: &str = "export-success-panel-message";

/// React `h-12 w-12` — the wash circle is 48pt square.
const BADGE_SIZE: f32 = 48.0;
/// React `h-6 w-6` — the check glyph is 24pt square.
const ICON_SIZE: f32 = 24.0;
/// React `gap-2`.
const GAP: f32 = 8.0;
/// React `py-6`.
const PAD_Y: f32 = 24.0;
/// React `bg-green-500/15` — a 15% wash, carried on the success ink.
const WASH_ALPHA: f32 = 0.15;

/// The green "export finished" pane.
pub(crate) struct ExportSuccessPanel {
    message: String,
}

impl ExportSuccessPanel {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    /// Builds the pane. The root grows to the parent's width so `items_center`
    /// puts the badge and the message on the pane's horizontal centre line.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let green = k.c.success.fg;
        col()
            .key(PANEL_KEY)
            .w_full()
            .items_center()
            .gap(GAP)
            .py(PAD_Y)
            .child(
                row()
                    .key(BADGE_KEY)
                    .size(BADGE_SIZE, BADGE_SIZE)
                    .radius(999.0)
                    .center()
                    .fill(k.alpha(green, WASH_ALPHA))
                    .child(
                        icon(&ICON_CHECK, ICON_SIZE)
                            .key(ICON_KEY)
                            .color(green)
                            .a11y_hidden(),
                    ),
            )
            .child(
                text(self.message)
                    .key(MESSAGE_KEY)
                    .font(k.ty.body_medium)
                    .color(k.c.text_base)
                    .align(Align::Center),
            )
    }
}
