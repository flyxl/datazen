//! Visual tokens and layout metrics for navigator and toolbar primitives.

use rune::tok;
use rune::Color;

/// The default app appearance, mirroring packages/frontend/src/styles/themes.css.
#[derive(Clone, Copy)]
pub(crate) struct AppColors {
    pub surface: Color,
    pub surface_alt: Color,
    pub surface_raised: Color,
    pub surface_inset: Color,
    pub edge: Color,
    pub edge_hi: Color,
    pub fg: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub on_accent: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub titlebar: Color,
    pub titlebar_fg: Color,
    pub titlebar_fg_muted: Color,
    pub titlebar_hover: Color,
}

fn rgb(hex: u32) -> Color {
    let channel = |shift: u32| rune::color::srgb_decode(((hex >> shift) & 0xff) as f32 / 255.0);
    Color {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.0,
    }
}

pub(crate) fn colors(dark: bool) -> AppColors {
    let accent = rgb(0x4fc3f7);
    let accent_hover = rgb(0x6fd0fa);
    if dark {
        AppColors {
            surface: rgb(0x0b0e14),
            surface_alt: rgb(0x0e121b),
            surface_raised: rgb(0x161b28),
            surface_inset: rgb(0x0a0d14),
            edge: rgb(0x1e2534),
            edge_hi: rgb(0x2a3549),
            fg: rgb(0xe6eaf2),
            fg_secondary: rgb(0x8b94a7),
            fg_muted: rgb(0x5f6879),
            accent,
            accent_hover,
            on_accent: rgb(0x04121b),
            success: rgb(0x5fd08a),
            warning: rgb(0xfbbf24),
            danger: rgb(0xff7b72),
            titlebar: rgb(0x0a0d14),
            titlebar_fg: rgb(0xe6eaf2),
            titlebar_fg_muted: rgb(0x5f6879),
            titlebar_hover: accent.alpha(0.16),
        }
    } else {
        AppColors {
            surface: rgb(0xf6f8fb),
            surface_alt: rgb(0xf1f5f9),
            surface_raised: rgb(0xe8eef6),
            surface_inset: rgb(0xffffff),
            edge: rgb(0xe1e8f1),
            edge_hi: rgb(0xc6d2e1),
            fg: rgb(0x26364d),
            fg_secondary: rgb(0x58708d),
            fg_muted: rgb(0x8498af),
            accent,
            accent_hover,
            on_accent: Color::WHITE,
            success: rgb(0x18b77a),
            warning: rgb(0xe39a27),
            danger: rgb(0xe05252),
            titlebar: Color::WHITE,
            titlebar_fg: rgb(0x26364d),
            titlebar_fg_muted: rgb(0x7187a2),
            titlebar_hover: rgb(0x4385f5).alpha(0.1),
        }
    }
}

pub(crate) fn app_theme(dark: bool) -> tok::Theme {
    let c = colors(dark);
    tok::Theme {
        bg: c.surface,
        card: c.surface_inset,
        card_border: c.edge,
        text: c.fg,
        text_secondary: c.fg_secondary,
        separator: c.edge,
        control_track: c.surface_alt,
        control_off: c.surface_raised,
        indicator: c.accent,
        hover: c.surface_raised,
        press: c.edge_hi,
        ..tok::theme(dark)
    }
}

pub(crate) fn accent(dark: bool) -> Color {
    colors(dark).accent
}

pub const NAV_ROW_H: f32 = 28.0;
pub const NAV_SEARCH_H: f32 = 28.0;
pub const NAV_GAP: f32 = 2.0;
pub const NAV_INDENT: f32 = 16.0;
pub const NAV_RADIUS: f32 = 6.0;
pub const NAV_ICON: f32 = 14.0;
pub const NAV_CHEVRON: f32 = 14.0;

#[derive(Clone, Copy)]
pub struct NavPalette {
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

pub fn nav_palette(theme: tok::Theme, dark: bool) -> NavPalette {
    NavPalette {
        bg: theme.bg,
        hover: theme.hover,
        selected: accent(dark).alpha(0.18),
        text: theme.text,
        text_muted: theme.text_secondary,
        text_faint: theme.text_secondary.alpha(if dark { 0.7 } else { 0.8 }),
        border: theme.card_border,
        icon: theme.text,
        icon_muted: theme.text_secondary,
        accent: accent(dark),
    }
}