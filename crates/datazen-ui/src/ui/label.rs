//! Visible field label.
//!
//! Ported from `packages/ui/src/Label.tsx` (text label with optional required
//! marker). Rune has no `for/id` DOM linkage; accessible naming is carried by
//! associated controls (e.g. `TextField::label(...)` in `ui/field.rs`). This
//! component provides the visible label row and does not duplicate control
//! accessible-name wiring.
//!
//! ## Key convention
//!
//! | part     | key              |
//! |----------|------------------|
//! | root     | `<key>`          |
//! | text     | `<key>-text`     |
//! | required | `<key>-required` |
//! | hint     | `<key>-hint`     |

use rune::{col, row, text, El};

#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct Label {
    key: String,
    text: String,
    required: bool,
    hint: Option<String>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl Label {
    pub(crate) fn new(key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: text.into(),
            required: false,
            hint: None,
        }
    }

    pub(crate) fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let mut root = col().key(self.key.clone()).w_full().gap(2.0);
        let mut line = row().w_full().items_center().gap(2.0).child(
            text(self.text)
                .key(format!("{}-text", self.key))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
        if self.required {
            line = line.child(
                text("*")
                    .key(format!("{}-required", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            );
        }
        root = root.child(line);
        if let Some(hint) = self.hint {
            root = root.child(
                text(hint)
                    .key(format!("{}-hint", self.key))
                    .font(k.ty.small)
                    .color(k.c.text_faint),
            );
        }
        root
    }
}
