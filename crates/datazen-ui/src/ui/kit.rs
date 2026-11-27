//! DataZen's branded entry point into `rune-kit`.

use std::sync::OnceLock;

use rune::Shadow;
use rune_kit::Kit;

use super::theme::colors;

static DARK: OnceLock<Kit> = OnceLock::new();
static LIGHT: OnceLock<Kit> = OnceLock::new();

pub fn kit(dark: bool) -> &'static Kit {
    let slot = if dark { &DARK } else { &LIGHT };
    slot.get_or_init(|| {
        let mut kit = Kit::neutral(dark);
        let app = colors(dark);
        let c = &mut kit.c;
        c.page = app.surface;
        c.bg_base = app.surface_inset;
        c.bg_deep = app.surface;
        c.layer_01 = app.surface;
        c.layer_02 = app.surface_alt;
        c.layer_03 = app.surface_raised;
        c.layer_04 = app.edge_hi;
        c.bg_inverse = app.fg;
        c.bg_contrast = app.fg;
        c.bg_button_neutral = app.surface;
        c.bg_icon_button_contrast = app.fg;
        c.bg_accent = app.accent;
        c.submit_fg = app.on_accent;
        c.submit_icon = app.on_accent;
        c.submit_hover = app.accent_hover;
        c.text_base = app.fg;
        c.text_muted = app.fg_secondary;
        c.text_faint = app.fg_muted;
        c.text_inverse = app.surface_inset;
        c.text_contrast = app.surface_inset;
        c.text_accent = app.accent;
        c.icon_base = app.fg;
        c.icon_muted = app.fg_secondary;
        c.icon_faint = app.fg_muted;
        c.icon_inverse = app.surface_inset;
        c.icon_contrast = app.surface_inset;
        c.icon_accent = app.accent;
        c.border_muted = app.edge;
        c.border_base = app.edge;
        c.border_strong = app.edge_hi;
        c.border_focus = app.accent;
        c.hover = app.surface_raised;
        c.pressed = app.edge_hi;
        for (status, color) in [
            (&mut c.success, app.success),
            (&mut c.warning, app.warning),
            (&mut c.danger, app.danger),
            (&mut c.info, app.accent),
        ] {
            status.fg = color;
            status.icon = color;
        }
        c.surface_base = app.surface;
        c.surface_weak = app.surface_alt;
        c.surface_hover = app.surface_raised;
        c.surface_raised = app.surface_raised;
        c.surface_raised_base = app.surface_inset;
        c.surface_raised_hover = app.surface_raised;
        c.surface_float = app.surface_inset;
        c.surface_disabled = app.surface_alt;
        c.border_weak = app.edge;
        c.border_hover = app.edge_hi;
        c.border_active = app.accent;
        c.border_disabled = app.edge;
        c.text_strong = app.fg;
        c.text_weak = app.fg_secondary;
        c.text_invert_strong = app.surface_inset;
        c.icon_weaker = app.fg_muted;
        c.std_border = app.edge;
        c.std_icon = app.fg_secondary;
        c.std_icon_invert = app.surface_inset;
        c.std_icon_strong = app.fg;
        c.std_text = app.fg;
        kit.ring = Some(|palette| palette.border_base);
        kit.elevation = kit.elevation.with_input(vec![Shadow {
            spread: 0.5,
            color: app.edge,
            ..Shadow::default()
        }]);
        kit
    })
}
