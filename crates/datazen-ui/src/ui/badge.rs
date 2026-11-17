//! Badge: compact status/label chip.
//!
//! Ported from `packages/ui/src/Badge.tsx`. React exposes tones
//! `neutral|success|warning|danger|accent` (no `dot` prop in current source).
//!
//! ## Key convention
//!
//! | part | key          |
//! |------|--------------|
//! | root | `<key>`      |
//! | text | `<key>-text` |
//!
//! Long text truncates and the badge should not force a constrained parent wider.

use rune::{row, text, El};

#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BadgeTone {
    #[default]
    Neutral,
    Success,
    Warning,
    Danger,
    Accent,
}

#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct Badge {
    key: String,
    label: String,
    tone: BadgeTone,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl Badge {
    pub(crate) fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            tone: BadgeTone::Neutral,
        }
    }

    pub(crate) fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let (fill, fg, border) = match self.tone {
            BadgeTone::Neutral => (k.c.layer_02, k.c.text_muted, k.c.border_base),
            BadgeTone::Success => (
                k.alpha(k.c.success.fg, 0.12),
                k.c.success.fg,
                k.alpha(k.c.success.fg, 0.25),
            ),
            BadgeTone::Warning => (
                k.alpha(k.c.warning.fg, 0.12),
                k.c.warning.fg,
                k.alpha(k.c.warning.fg, 0.25),
            ),
            BadgeTone::Danger => (
                k.alpha(k.c.danger.fg, 0.12),
                k.c.danger.fg,
                k.alpha(k.c.danger.fg, 0.25),
            ),
            BadgeTone::Accent => (
                k.alpha(k.c.bg_accent, 0.12),
                k.c.bg_accent,
                k.alpha(k.c.bg_accent, 0.25),
            ),
        };

        let key = self.key;
        row()
            .key(key.clone())
            .role("status")
            .label(self.label.clone())
            .center_y()
            .shrink()
            .min_w(0.0)
            .px(8.0)
            .py(2.0)
            .radius(9.0)
            .fill(fill)
            .border(1.0, border)
            .child(
                text(self.label)
                    .key(format!("{key}-text"))
                    .font(k.ty.small_medium)
                    .color(fg)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate()
                    .a11y_hidden(),
            )
    }
}
