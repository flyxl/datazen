//! Spinner: the one loading indicator.
//!
//! Ported from `packages/ui/src/Spinner.tsx`. React uses CSS `animate-spin`;
//! this rune implementation renders a **static** indicator (no app frame clock,
//! no `Tx::wake_after`) to keep the base deterministic and dependency-light.
//! Motion parity can be layered later with a renderer-owned loop if needed.
//!
//! ## Key convention
//!
//! | part        | key               |
//! |-------------|-------------------|
//! | root        | `<key>`           |
//! | indicator   | `<key>-indicator` |
//! | label text  | `<key>-label`     |
//!
//! Without a `label`, the root is decorative (`a11y_hidden`). With a `label`,
//! the root is `role="progressbar"` and carries the accessible label/text.
// No `views/` caller uses this yet; it is a reusable base for later migration tasks.

use crate::icons::{ICON_LOADER2, ICON_RING_ARC};
use rune::{el, icon, row, text, El};

/// Visual size. Three tiers as required by acceptance.
#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SpinnerSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl SpinnerSize {
    fn px(self) -> f32 {
        match self {
            Self::Sm => 12.0,
            Self::Md => 16.0,
            Self::Lg => 20.0,
        }
    }
}

/// Color tone.
#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SpinnerTone {
    #[default]
    Current,
    Accent,
    Muted,
}

/// Glyph stroke icon or ring-like arc.
#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SpinnerVariant {
    #[default]
    Icon,
    Ring,
}

#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct Spinner {
    key: String,
    size: SpinnerSize,
    tone: SpinnerTone,
    variant: SpinnerVariant,
    label: Option<String>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl Spinner {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            size: SpinnerSize::Md,
            tone: SpinnerTone::Current,
            variant: SpinnerVariant::Icon,
            label: None,
        }
    }

    pub(crate) fn size(mut self, size: SpinnerSize) -> Self {
        self.size = size;
        self
    }

    pub(crate) fn tone(mut self, tone: SpinnerTone) -> Self {
        self.tone = tone;
        self
    }

    pub(crate) fn variant(mut self, variant: SpinnerVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Standalone/loading announcement label. When set, root is `role="progressbar"` with text.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let px = self.size.px();
        let color = match self.tone {
            SpinnerTone::Current => k.c.text_base,
            SpinnerTone::Accent => k.c.bg_accent,
            SpinnerTone::Muted => k.c.text_muted,
        };
        let key = self.key;

        let indicator = match self.variant {
            SpinnerVariant::Icon => icon(&ICON_LOADER2, px)
                .key(format!("{key}-indicator"))
                .color(color),
            SpinnerVariant::Ring => icon(&ICON_RING_ARC, px)
                .key(format!("{key}-indicator"))
                .color(color),
        };

        if let Some(label) = self.label {
            row()
                .key(key.clone())
                .role("progressbar")
                .label(label.clone())
                .center()
                .gap(6.0)
                .child(indicator.a11y_hidden())
                .child(
                    text(label)
                        .key(format!("{key}-label"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
        } else {
            // Decorative: hidden from accessibility tree.
            el().key(key.clone())
                .size(px, px)
                .center()
                .a11y_hidden()
                .child(indicator.a11y_hidden())
        }
    }
}
