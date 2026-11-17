//! Visual tokens and layout metrics for navigator and toolbar primitives.

use rune::tok;
use rune::Color;

pub(crate) const NAV_ROW_H: f32 = 28.0;
pub(crate) const NAV_SEARCH_H: f32 = 28.0;
pub(crate) const NAV_GAP: f32 = 2.0;
pub(crate) const NAV_INDENT: f32 = 16.0;
pub(crate) const NAV_RADIUS: f32 = 6.0;
pub(crate) const NAV_ICON: f32 = 14.0;
pub(crate) const NAV_CHEVRON: f32 = 14.0;

#[derive(Clone, Copy)]
pub(crate) struct NavPalette {
    pub bg: Color,
    pub hover: Color,
    pub selected: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub border: Color,
    pub icon: Color,
    pub icon_muted: Color,
    pub accent: Color,
}

pub(crate) fn nav_palette(theme: tok::Theme, dark: bool) -> NavPalette {
    NavPalette {
        bg: theme.bg,
        hover: theme.hover,
        selected: tok::accent(0, dark).alpha(0.18),
        text: theme.text,
        text_muted: theme.text_secondary,
        text_faint: theme.text_secondary.alpha(if dark { 0.7 } else { 0.8 }),
        border: theme.card_border,
        icon: theme.text,
        icon_muted: theme.text_secondary,
        accent: tok::accent(0, dark),
    }
}