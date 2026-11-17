//! The error banner: a short, announced message that something failed.
//!
//! This is a reusable base ported from `packages/ui/src/ErrorBanner.tsx`. No
//! `views/` caller uses it yet; it exists so later tasks can compose it instead
//! of re-styling error bars by hand.
//!
//! ## What it owns
//!
//! - `role="alert"` on the root, so a banner that appears is announced without
//!   each call site remembering to set it.
//! - Three shapes, matching the React call sites rather than a designer scale:
//!   - `plain`: inline danger text (form footers, list headers).
//!   - `boxed`: a tinted, bordered block for a message that must read as its own
//!     card (a dialog's submit error).
//!   - `strip`: a full-bleed band with a bottom rule that separates regions.
//! - Optional leading content (`icon`), an optional dismiss control, and a
//!   `build` that returns nothing when the message is empty (as the React
//!   `as: div | p | span` form does for empty children).
//!
//! Everything visual comes from [`Kit`]: colours from
//! `c.danger.{bg,fg,border,icon}` / `c.text_base`, type from `k.ty.text_xsmall`,
//! radii from `k.radius`, and the dismiss glyph from `k.glyph.close`. No
//! colour literals live here.
//!
//! ## What it leaves out
//!
//! - Warnings. Amber notices are a different meaning and must not be rendered
//!   through an error component.
//! - Translation. Callers own their text (pass `t(...)` or a composed string);
//!   the component does not translate, so extracting a banner cannot silently
//!   change a string.
//! - The React `as` element choice is carried as [`BannerElement`] for API
//!   parity but does not change Rune layout semantics in a meaningful way.
//!
//! ## Key convention
//!
//! | part    | key             |
//! |---------|-----------------|
//! | root    | `<key>`         |
//! | text    | `<key>-text`    |
//! | dismiss | `<key>-dismiss` |
//!
//! The dismiss control is focusable, labelled with the caller-supplied label,
//! and runs its handler on press via `El::on_press`.
#![allow(dead_code)]

use rune::{col, icon, row, text, El};
use rune_kit::Kit;

/// How much chrome the banner carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BannerVariant {
    /// Inline danger text next to the thing that failed.
    Plain,
    /// A self-contained tinted card.
    Boxed,
    /// A full-bleed band with a bottom rule.
    Strip,
}

/// Host element hint, carried for parity with the React API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BannerElement {
    Div,
    P,
    Span,
}

type Dismiss = Box<dyn FnOnce(El) -> El>;

/// An announced error message.
pub(crate) struct Banner {
    key: String,
    message: String,
    variant: BannerVariant,
    as_: BannerElement,
    icon: Option<El>,
    dismiss: Option<(String, Dismiss)>,
}

impl Banner {
    pub(crate) fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
            variant: BannerVariant::Plain,
            as_: BannerElement::Div,
            icon: None,
            dismiss: None,
        }
    }

    pub(crate) fn variant(mut self, variant: BannerVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Carried for parity; does not materially change layout.
    pub(crate) fn as_element(mut self, as_: BannerElement) -> Self {
        self.as_ = as_;
        self
    }

    /// Leading content placed before the message.
    pub(crate) fn icon(mut self, icon: El) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A dismiss control. The label is required for accessibility.
    pub(crate) fn on_dismiss<S: 'static>(
        mut self,
        label: &'static str,
        f: impl Fn(&mut S) + 'static,
    ) -> Self {
        self.dismiss = Some((
            label.to_owned(),
            Box::new(move |e: El| e.on_press(f)),
        ));
        self
    }

    /// Builds the banner, or returns `None` when the message is empty.
    pub(crate) fn build(self, k: &Kit) -> Option<El> {
        if self.message.is_empty() {
            return None;
        }

        let is_row = self.icon.is_some() || self.dismiss.is_some();

        let text_el = text(self.message.clone())
            .key(format!("{}-text", self.key))
            .font(k.ty.text_xsmall)
            .color(k.c.danger.fg)
            .selectable(true)
            .wrap_anywhere();
        let text_el = if is_row {
            text_el.grow(1.0).min_w(0.0)
        } else {
            text_el
        };

        // Start from the element shape; all variants share alert role and text color.
        let mut root = match self.as_ {
            BannerElement::Span => row().key(self.key.clone()),
            BannerElement::Div | BannerElement::P => col().key(self.key.clone()),
        };
        root = root.role("alert");

        match self.variant {
            BannerVariant::Plain => {
                // Inline danger text; no chrome.
            }
            BannerVariant::Boxed => {
                root = root
                    .radius(k.radius.md)
                    .fill(k.c.danger.bg)
                    .border(1.0, k.c.danger.border)
                    .padding(8.0);
            }
            BannerVariant::Strip => {
                // Full-bleed band with a bottom rule (border-b): px-3 py-1.5.
                root = root
                    .fill(k.c.danger.bg)
                    .border_sides([0.0, 0.0, 1.0, 0.0], k.c.danger.border)
                    .border_layout()
                    .px(12.0)
                    .py(6.0);
            }
        }

        if is_row {
            let mut line = row().w_full().items_start().gap(8.0);
            if let Some(ic) = self.icon {
                line = line.child(ic);
            }
            line = line.child(text_el);
            if let Some((label, handle)) = self.dismiss {
                let mut btn = row()
                    .key(format!("{}-dismiss", self.key))
                    .focusable(true)
                    .shrink()
                    .radius(k.radius.sm)
                    .px(4.0)
                    .py(2.0)
                    .center()
                    .label(label)
                    .hover(k.alpha(k.c.danger.fg, 0.2))
                    .child(icon(k.glyph.close, 14.0).color(k.c.danger.fg).a11y_hidden());
                btn = handle(btn);
                line = line.child(btn);
            }
            root = root.child(line);
        } else {
            root = root.child(text_el);
        }

        Some(root)
    }
}
