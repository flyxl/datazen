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

/// Button surfaces. Primary reuses the blue accent token; danger is the one
/// hue the kit has no token for, and it lives here so no view hardcodes it.
#[derive(Clone, Copy)]
pub(crate) struct ButtonPalette {
    pub primary: Color,
    pub primary_hover: Color,
    pub on_primary: Color,
    pub danger: Color,
    pub danger_hover: Color,
    pub on_danger: Color,
}

const DANGER_HUE: f32 = 25.0;
const ACCENT_HUE: f32 = 256.0;

pub(crate) fn button_palette(dark: bool) -> ButtonPalette {
    let l = if dark { 0.66 } else { 0.6 };
    ButtonPalette {
        primary: tok::accent(0, dark),
        primary_hover: rune::oklch(l - 0.05, 0.17, ACCENT_HUE),
        on_primary: Color::WHITE,
        danger: rune::oklch(l - 0.04, 0.19, DANGER_HUE),
        danger_hover: rune::oklch(l - 0.09, 0.19, DANGER_HUE),
        on_danger: Color::WHITE,
    }
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