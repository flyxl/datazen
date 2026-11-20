//! Semantic colors for the Rune UI, all sourced from `kit(dark)` tokens where
//! possible. Literal `oklch`/`oklcha` values live only in this module so the
//! rest of the UI reads intent, not raw numbers.

use rune::Color;

use super::kit::kit;

/// Syntax-highlighting colors for the SQL editor. The values keep the
/// pre-migration hue/lightness so the editor look does not drift; kit has no
/// dedicated syntax tokens, so these remain explicit literals here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodePalette {
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
}

pub(crate) fn code_palette(dark: bool) -> CodePalette {
    if dark {
        CodePalette {
            // kit 无对应 token: syntax keyword (violet)
            keyword: rune::oklch(0.72, 0.13, 305.0),
            // kit 无对应 token: syntax string (green)
            string: rune::oklch(0.74, 0.12, 145.0),
            // kit 无对应 token: syntax comment (olive/neutral)
            comment: rune::oklch(0.62, 0.02, 95.0),
            // kit 无对应 token: syntax number (amber)
            number: rune::oklch(0.78, 0.12, 70.0),
        }
    } else {
        CodePalette {
            // kit 无对应 token: syntax keyword (violet)
            keyword: rune::oklch(0.48, 0.16, 305.0),
            // kit 无对应 token: syntax string (green)
            string: rune::oklch(0.50, 0.14, 145.0),
            // kit 无对应 token: syntax comment (olive/neutral)
            comment: rune::oklch(0.52, 0.03, 95.0),
            // kit 无对应 token: syntax number (amber)
            number: rune::oklch(0.52, 0.14, 70.0),
        }
    }
}

/// Syntax-highlighting colors for the workflow YAML editor (T265's port of
/// `yamlMode.ts`'s `--cm-*` token mapping). The four tags the React file
/// shares with the SQL palette reuse those values (comment/propertyName/
/// string/number ride `--cm-*` twins of the same hues); the three it adds —
/// atom literals (`true`/`null`…, React `--cm-operator`), punctuation
/// (flow markers and `${param}` refs, React `--cm-punctuation`) and the
/// foreground ink — stay explicit literals here under the same rule: kit
/// has no dedicated syntax tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct YamlPalette {
    pub comment: Color,
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub atom: Color,
    pub punctuation: Color,
    pub foreground: Color,
}

pub(crate) fn yaml_palette(dark: bool, foreground: Color) -> YamlPalette {
    let code = code_palette(dark);
    let (atom, punctuation) = if dark {
        // kit 无对应 token: yaml atom literal (React --cm-operator)
        (rune::oklch(0.74, 0.11, 70.0), rune::oklch(0.66, 0.02, 250.0))
    } else {
        // kit 无对应 token: yaml atom literal (React --cm-operator)
        (rune::oklch(0.50, 0.13, 70.0), rune::oklch(0.45, 0.03, 250.0))
    };
    YamlPalette {
        comment: code.comment,
        keyword: code.keyword,
        string: code.string,
        number: code.number,
        atom,
        punctuation,
        foreground,
    }
}

/// Data-grid chrome and cell background colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GridPalette {
    pub header_bg: Color,
    pub header_gutter_bg: Color,
    pub gutter_bg: Color,
    pub frozen_bg: Color,
    pub find_current: Color,
    pub find_hit: Color,
    pub active: Color,
    pub selected: Color,
    pub frozen_cell: Color,
    pub zebra_even: Color,
    pub zebra_odd_dark: Color,
    pub zebra_odd_light: Color,
}

pub(crate) fn grid_palette(dark: bool) -> GridPalette {
    if dark {
        GridPalette {
            // kit 无对应 token: grid header chrome (cool dark)
            header_bg: rune::oklch(0.22, 0.02, 250.0),
            // kit 无对应 token: grid gutter header chrome
            header_gutter_bg: rune::oklch(0.20, 0.02, 250.0),
            // kit 无对应 token: grid row-number gutter
            gutter_bg: rune::oklch(0.19, 0.02, 250.0),
            // kit 无对应 token: frozen column/header tint
            frozen_bg: rune::oklch(0.24, 0.03, 250.0),
            // kit 无对应 token: current find hit (amber)
            find_current: rune::oklch(0.55, 0.14, 85.0),
            // kit 无对应 token: other find hits (amber)
            find_hit: rune::oklch(0.42, 0.10, 85.0),
            // kit 无对应 token: active/focused cell
            active: rune::oklch(0.45, 0.09, 250.0),
            // kit 无对应 token: selected range
            selected: rune::oklch(0.38, 0.05, 250.0),
            // kit 无对应 token: frozen cell background
            frozen_cell: rune::oklch(0.20, 0.02, 250.0),
            // kit 无对应 token: zebra stripe even
            zebra_even: rune::oklch(0.18, 0.01, 250.0),
            // kit 无对应 token: zebra stripe odd (dark)
            zebra_odd_dark: rune::oklch(0.16, 0.01, 250.0),
            // kit 无对应 token: zebra stripe odd (light)
            zebra_odd_light: rune::oklch(0.965, 0.005, 250.0),
        }
    } else {
        GridPalette {
            // kit 无对应 token: grid header chrome (cool light)
            header_bg: rune::oklch(0.92, 0.01, 250.0),
            // kit 无对应 token: grid gutter header chrome
            header_gutter_bg: rune::oklch(0.90, 0.01, 250.0),
            // kit 无对应 token: grid row-number gutter
            gutter_bg: rune::oklch(0.93, 0.01, 250.0),
            // kit 无对应 token: frozen column/header tint
            frozen_bg: rune::oklch(0.90, 0.01, 250.0),
            // kit 无对应 token: current find hit (amber)
            find_current: rune::oklch(0.86, 0.14, 85.0),
            // kit 无对应 token: other find hits (amber)
            find_hit: rune::oklch(0.92, 0.10, 85.0),
            // kit 无对应 token: active/focused cell
            active: rune::oklch(0.78, 0.10, 250.0),
            // kit 无对应 token: selected range
            selected: rune::oklch(0.86, 0.05, 250.0),
            // kit 无对应 token: frozen cell background
            frozen_cell: rune::oklch(0.94, 0.01, 250.0),
            // kit 无对应 token: zebra stripe even
            zebra_even: rune::oklch(0.99, 0.005, 250.0),
            // kit 无对应 token: zebra stripe odd (dark, unused in light mode)
            zebra_odd_dark: rune::oklch(0.16, 0.01, 250.0),
            // kit 无对应 token: zebra stripe odd (light)
            zebra_odd_light: rune::oklch(0.965, 0.005, 250.0),
        }
    }
}

/// SQL editor surface/completion/caret colors.
pub(crate) fn editor_bg(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: editor document background
        rune::oklch(0.22, 0.03, 250.0)
    } else {
        // kit 无对应 token: editor document background
        rune::oklch(0.97, 0.01, 250.0)
    }
}

pub(crate) fn editor_selection_bg(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: editor selection/completion highlight
        rune::oklch(0.34, 0.08, 250.0)
    } else {
        // kit 无对应 token: editor selection/completion highlight
        rune::oklch(0.86, 0.06, 250.0)
    }
}

pub(crate) fn editor_composition_underline(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: IME composition underline
        rune::oklch(0.8, 0.1, 95.0)
    } else {
        // kit 无对应 token: IME composition underline
        rune::oklch(0.55, 0.12, 95.0)
    }
}

pub(crate) fn editor_caret_fill(main: bool, dark: bool) -> Color {
    if dark {
        if main {
            // kit 无对应 token: main caret
            rune::oklch(0.9, 0.12, 250.0)
        } else {
            // kit 无对应 token: secondary caret
            rune::oklch(0.8, 0.08, 250.0)
        }
    } else if main {
        // kit 无对应 token: main caret
        rune::oklch(0.35, 0.10, 250.0)
    } else {
        // kit 无对应 token: secondary caret
        rune::oklch(0.45, 0.08, 250.0)
    }
}

/// Amber folder icon used in navigator section headers.
pub(crate) fn folder_icon(_dark: bool) -> Color {
    // kit 无对应 token: amber folder glyph
    rune::oklcha(0.78, 0.15, 80.0, 1.0)
}

/// Splitter drag handle fill.
pub(crate) fn splitter_drag_fill(_dark: bool) -> Color {
    // kit 无对应 token: neutral drag handle with alpha
    rune::oklch(0.5, 0.0, 0.0).alpha(0.2)
}

/// Status indicators for compare/sync/run results and connection states.
pub(crate) fn status_ok(_dark: bool) -> Color {
    // kit 无对应 token: success indicator dot (kept as explicit hue)
    rune::oklch(0.75, 0.12, 145.0)
}

pub(crate) fn status_fail(_dark: bool) -> Color {
    // kit 无对应 token: failure indicator dot (kept as explicit hue)
    rune::oklch(0.7, 0.14, 25.0)
}

/// Accent bar fill used in simple charts/widgets.
pub(crate) fn chart_bar_accent(_dark: bool) -> Color {
    // kit 无对应 token: chart bar accent
    rune::oklch(0.72, 0.13, 250.0)
}

/// Warning banner tint (desktop adapter banner).
pub(crate) fn warning_banner_fill(_dark: bool) -> Color {
    // kit 无对应 token: warm warning banner tint with alpha
    rune::oklcha(0.66, 0.15, 58.0, 0.14)
}

/// Subtle accent-tinted surface (MCP promo icon well, empty-state icon well).
pub(crate) fn accent_tint_fill(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: accent-tinted translucent fill
        rune::oklcha(0.62, 0.17, 256.0, 0.14)
    } else {
        // kit 无对应 token: accent-tinted translucent fill
        rune::oklcha(0.6, 0.17, 256.0, 0.14)
    }
}

/// Connected status pill background.
pub(crate) fn status_pill_connected_bg(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: connected pill translucent green
        rune::oklcha(0.7, 0.16, 152.0, 0.16)
    } else {
        // kit 无对应 token: connected pill translucent green
        rune::oklcha(0.55, 0.16, 152.0, 0.16)
    }
}

/// Connecting status pill background.
pub(crate) fn status_pill_connecting_bg(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: connecting pill translucent blue
        rune::oklcha(0.66, 0.17, 256.0, 0.16)
    } else {
        // kit 无对应 token: connecting pill translucent blue
        rune::oklcha(0.6, 0.17, 256.0, 0.16)
    }
}

/// Engine badge fill keyed by hue, matching the React connection badges.
pub(crate) fn engine_badge_fill(dark: bool, hue: f32) -> Color {
    // kit 无对应 token: engine badge chroma keyed by engine hue
    rune::oklch(if dark { 0.5 } else { 0.58 }, 0.12, hue)
}

/// Near-white ink used on saturated badges.
pub(crate) fn badge_ink(_dark: bool) -> Color {
    // kit 无对应 token: badge foreground near-white
    rune::oklch(0.99, 0.005, 256.0)
}

/// Text that stays readable on accent fills in both appearances.
pub(crate) fn on_accent(_dark: bool) -> Color {
    // kit 无对应 token: on-accent near-white
    rune::oklch(0.985, 0.005, 256.0)
}

/// Danger/error text for inline form validation.
pub(crate) fn danger_text(dark: bool) -> Color {
    let c = kit(dark).c;
    // Mapped to kit semantic danger foreground for consistency.
    c.danger.fg
}

/// Dashboard tile alert glyph (T069, `ChartWidgetTile.tsx`'s
/// `text-amber-500` on the fired-alert triangle). Tailwind amber-500
/// (oklch 0.769 0.164 70.08) verbatim — the same hue `ai_amber_border`
/// carries at 30%.
pub(crate) fn alert_amber(_dark: bool) -> Color {
    // kit 无对应 token: dashboard alert amber
    rune::oklch(0.769, 0.164, 70.08)
}

/// Dashboard tile run-timeout ink (T069, `ChartWidgetTile.tsx`'s
/// `text-amber-400` on the timeout line). Tailwind amber-400
/// (oklch 0.828 0.189 84.429) verbatim.
pub(crate) fn timeout_amber(_dark: bool) -> Color {
    // kit 无对应 token: dashboard timeout amber
    rune::oklch(0.828, 0.189, 84.429)
}

/// Run-history ok-status ink (T175, `RunHistoryDrawer.tsx`'s
/// `text-emerald-500` on an ok index entry — the `statusClass` table's ok
/// arm). Tailwind emerald-500 (oklch 0.696 0.17 162.48) verbatim, the
/// sibling of the tile's `alert_amber` / `timeout_amber`.
pub(crate) fn run_ok_emerald(_dark: bool) -> Color {
    // kit 无对应 token: dashboard run-status emerald
    rune::oklch(0.696, 0.17, 162.48)
}

/// AI profile shield tint per safety preset (T009-S3, `AiChatPanel.tsx`:
/// `text-emerald-400` / `text-amber-400` / `text-blue-400`). Exact Tailwind
/// sRGB values, so the laid-out shield reports the same hue React paints.
pub(crate) fn ai_shield_local_trust(_dark: bool) -> Color {
    rune::color::hex("#34d399").unwrap_or(Color::WHITE)
}

pub(crate) fn ai_shield_enterprise_balanced(_dark: bool) -> Color {
    rune::color::hex("#fbbf24").unwrap_or(Color::WHITE)
}

pub(crate) fn ai_shield_cloud_strict(_dark: bool) -> Color {
    rune::color::hex("#60a5fa").unwrap_or(Color::WHITE)
}

/// AI input-area amber (T009-S5): the egress notice and the draft-conflict
/// bar use Tailwind's amber-500 / amber-200 verbatim (`AiEgressNotice.tsx`
/// and `AiChatPanel.tsx:900` — `border-amber-500/30 bg-amber-500/10
/// text-amber-200`), as oklch literals in this module only. The database
/// line is the same amber-200 text at 70% opacity, applied by the caller
/// through `kit.alpha`.
pub(crate) fn ai_amber_border(_dark: bool) -> Color {
    // Tailwind amber-500 (oklch 0.769 0.164 70.08) at 30% — `/30`.
    rune::oklcha(0.769, 0.164, 70.08, 0.30)
}

pub(crate) fn ai_amber_fill(_dark: bool) -> Color {
    // Tailwind amber-500 at 10% — `/10`.
    rune::oklcha(0.769, 0.164, 70.08, 0.10)
}

pub(crate) fn ai_amber_text(_dark: bool) -> Color {
    // Tailwind amber-200 (oklch 0.924 0.12 95.08).
    rune::oklch(0.924, 0.12, 95.08)
}

/// AI preset-button borders (T107, `ModelProfileForm.tsx`'s safety-preset
/// buttons: `border-blue-500` / `border-amber-500` / `border-emerald-500`).
/// Exact Tailwind sRGB values so the active preset reports the hue React
/// paints; the `-400` label inks are the `ai_shield_*` helpers above.
pub(crate) fn ai_preset_blue_border(_dark: bool) -> Color {
    rune::color::hex("#3b82f6").unwrap_or(Color::WHITE)
}

pub(crate) fn ai_preset_amber_border(_dark: bool) -> Color {
    rune::color::hex("#f59e0b").unwrap_or(Color::WHITE)
}

pub(crate) fn ai_preset_emerald_border(_dark: bool) -> Color {
    rune::color::hex("#10b981").unwrap_or(Color::WHITE)
}

/// AI preset-badge purple (T141, `ModelProfileListItem.tsx`'s custom-preset
/// badge: `bg-purple-500/10` — the 500-hue fill the other three presets
/// take from the `ai_preset_*_border` helpers above, at 10% via `Kit::alpha`).
pub(crate) fn ai_preset_purple_border(_dark: bool) -> Color {
    rune::color::hex("#a855f7").unwrap_or(Color::WHITE)
}

/// The custom-preset badge's purple-400 ink (T141, `text-purple-400`), the
/// `ai_shield_*` naming of the T009-S3/T107 400-hue label inks.
pub(crate) fn ai_shield_custom(_dark: bool) -> Color {
    rune::color::hex("#c084fc").unwrap_or(Color::WHITE)
}


/// ER diagram box background.
pub(crate) fn er_box_fill(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: ER box surface
        rune::oklch(0.24, 0.03, 250.0)
    } else {
        // kit 无对应 token: ER box surface
        rune::oklch(0.96, 0.01, 250.0)
    }
}

/// The onboarding brand sidebar's fixed dark vertical gradient (React
/// `linear-gradient(180deg,#0e131c,#0a0e15)` — an always-dark brand anchor
/// regardless of the app theme), as `(top, bottom)`.
pub(crate) fn brand_sidebar_gradient() -> (Color, Color) {
    // kit 无对应 token: brand sidebar anchor #0e131c / #0a0e15
    (rune::oklch(0.186, 0.020, 262.0), rune::oklch(0.163, 0.016, 262.0))
}

/// The brand sidebar's radial accent tint (React
/// `rgba(79,195,247,0.13)` — #4fc3f7 at 13% — fading to transparent at 55%,
/// centred on the sidebar's top-left corner).
pub(crate) fn brand_sidebar_radial_tint() -> Color {
    // kit 无对应 token: brand radial tint #4fc3f7 @ 13%
    rune::oklcha(0.773, 0.127, 231.0, 0.13)
}

/// The onboarding S0 entry cards' dark-mode vertical gradient
/// (`WelcomeStep.tsx`'s `dark:bg-[linear-gradient(180deg,#121824,#0f131c)]`;
/// the light theme paints the flat `bg-surface` instead), as `(top, bottom)`.
/// React's opaque `bg-edge` surface (themes.css `--c-edge`: the future
/// step-dot pin and other solid edge fills — NOT the kit's translucent
/// border ink).
pub(crate) fn edge_solid(dark: bool) -> Color {
    if dark {
        rune::color::hex("#1e2534").unwrap_or(Color::WHITE)
    } else {
        rune::color::hex("#e1e8f1").unwrap_or(Color::WHITE)
    }
}

pub(crate) fn welcome_card_gradient() -> (Color, Color) {
    // kit 无对应 token: the entry cards' dark pair #121824 / #0f131c
    (
        rune::color::hex("#121824").unwrap_or(Color::BLACK),
        rune::color::hex("#0f131c").unwrap_or(Color::BLACK),
    )
}

/// The onboarding S0 entry cards' hover fill: dark is React's
/// `dark:hover:bg-[#141a26]`, light is `hover:bg-accent-dim` — the
/// `--c-accent-dim` token, #4fc3f7 at 12%.
pub(crate) fn welcome_card_hover(dark: bool) -> Color {
    if dark {
        // kit 无对应 token: the entry cards' dark hover #141a26
        rune::color::hex("#141a26").unwrap_or(Color::BLACK)
    } else {
        // kit 无对应 token: bg-accent-dim = #4fc3f7 @ 12%
        rune::color::hex("#4fc3f7").unwrap_or(Color::WHITE).alpha(0.12)
    }
}
