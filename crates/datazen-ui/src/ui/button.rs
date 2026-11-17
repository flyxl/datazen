//! The one button, mirroring `packages/ui/src/Button.tsx` (which
//! `packages/frontend/src/components/ui/Button.tsx` re-exports).
//!
//! Four variants share one height and one text baseline, so a dialog footer or
//! a toolbar row of mixed variants lines up without per-call-site padding. The
//! surface colours come from `ui::theme` — a view never picks its own fill.
//!
//! The full variant/size set ships ahead of its call sites: the port ledger in
//! `docs/development/rune-migration/tasks-react-parity.md` consumes it, and the
//! first views only need two of the four variants so far.

#![allow(dead_code)]

use rune::tok::{self, type_};
use rune::{row, text, El, Icon};

use crate::state_def::AppState;
use crate::ui::theme;

/// Toolbar height, matching the navigator row.
pub(crate) const BUTTON_SM: f32 = 28.0;
/// Form height, matching `TextField`'s default.
pub(crate) const BUTTON_MD: f32 = 32.0;
/// Footer height, matching the React dialog footers.
pub(crate) const BUTTON_LG: f32 = 36.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonVariant {
    /// The one emphasised action in a surface (accent fill, white label).
    Primary,
    /// Everything next to the primary action (bordered card fill).
    Secondary,
    /// Borderless, for rows that already carry a border.
    Ghost,
    /// Destructive action, used behind a `ConfirmDialog`.
    Danger,
}

/// A labelled button. Build it with the chain, then `.build(dark)`.
pub(crate) struct Button<'a> {
    key: String,
    label: &'a str,
    variant: ButtonVariant,
    height: f32,
    icon: Option<(&'static Icon, f32)>,
    disabled: bool,
    pressed: Option<Box<dyn Fn(&mut AppState) + 'static>>,
}

impl<'a> Button<'a> {
    pub(crate) fn new(key: impl Into<String>, label: &'a str) -> Self {
        Self {
            key: key.into(),
            label,
            variant: ButtonVariant::Secondary,
            height: BUTTON_LG,
            icon: None,
            disabled: false,
            pressed: None,
        }
    }

    pub(crate) fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// A leading icon. The label stays, so the button still reads in the a11y
    /// tree and in the `--tree` snapshot.
    pub(crate) fn icon(mut self, icon: &'static Icon, size: f32) -> Self {
        self.icon = Some((icon, size));
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_press(mut self, f: impl Fn(&mut AppState) + 'static) -> Self {
        self.pressed = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self, theme: tok::Theme, dark: bool) -> El {
        let palette = theme::button_palette(dark);
        let (fill, hover, border, ink) = match self.variant {
            ButtonVariant::Primary => (
                palette.primary,
                palette.primary_hover,
                None,
                palette.on_primary,
            ),
            ButtonVariant::Secondary => (
                theme.card,
                theme.hover,
                Some(theme.card_border),
                theme.text_secondary,
            ),
            ButtonVariant::Ghost => (
                rune::Color::TRANSPARENT,
                theme.hover,
                None,
                theme.text,
            ),
            ButtonVariant::Danger => (
                palette.danger,
                palette.danger_hover,
                None,
                palette.on_danger,
            ),
        };
        let ink = if self.disabled {
            ink.alpha(0.55)
        } else {
            ink
        };

        let pad = if self.icon.is_some() { 12.0 } else { 14.0 };
        let mut e = row()
            .key(self.key)
            .h(self.height)
            .px(pad)
            .gap(6.0)
            .center()
            .radius(8.0)
            .fill(fill)
            .hover(hover)
            .disabled(self.disabled);
        if let Some(width) = border {
            e = e.border(1.0, width);
        }
        if let Some((icon, size)) = self.icon {
            e = e.child(rune::icon(icon, size).color(ink));
        }
        e = e.child(text(self.label).font(type_::LABEL).color(ink));

        if let Some(pressed) = self.pressed {
            if !self.disabled {
                e = e.on_press(move |state: &mut AppState| pressed(state));
            }
        }
        e
    }
}
