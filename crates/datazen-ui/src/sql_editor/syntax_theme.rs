//! Native projection of the shared React syntax preset palette. Resolve the
//! preset once per surface, not once per lexical token.

use datazen_ui_api::{SqlSyntaxTheme, SQL_SYNTAX_PRESETS};
use rune::{color, Color};
use crate::ui::palette::{self, CodePalette};

#[derive(Clone, Copy)]
pub(crate) struct SyntaxPalette {
    pub tokens: CodePalette,
    pub operator: Option<Color>,
    pub punctuation: Option<Color>,
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub selection: Option<Color>,
}

impl SyntaxPalette {
    pub fn live(theme: SqlSyntaxTheme, dark: bool) -> Self {
        if theme == SqlSyntaxTheme::DEFAULT {
            // Preserve the existing Rune SQL paint on missing or Default.
            Self {
                tokens: palette::code_palette(dark),
                operator: None,
                punctuation: None,
                foreground: None,
                background: None,
                selection: None,
            }
        } else {
            Self::named(theme, dark)
        }
    }

    pub fn preview(theme: SqlSyntaxTheme, dark: bool) -> Self {
        // The original preview uses the palette of the Default preset, but
        // the actual editor's Default follows the current app appearance.
        let mut palette = Self::named(theme, dark);
        palette.background = palette.background.or_else(|| color::hex(if dark { "#1e1e1e" } else { "#ffffff" }));
        palette.foreground = palette.foreground.or_else(|| color::hex(if dark { "#d4d4d4" } else { "#000000" }));
        palette
    }

    fn named(theme: SqlSyntaxTheme, dark: bool) -> Self {
        let preset = SQL_SYNTAX_PRESETS.iter().find(|preset| preset.id == theme.id())
            .expect("SqlSyntaxTheme is validated by the shared preset table");
        let entries = if dark { preset.dark.entries } else { preset.light.entries };
        let get = |key| entries.iter().find(|(name, _)| *name == key).and_then(|(_, value)| parse_color(value));
        let base = palette::code_palette(dark);
        Self {
            tokens: CodePalette {
                keyword: get("keyword").unwrap_or(base.keyword),
                string: get("string").unwrap_or(base.string),
                comment: get("comment").unwrap_or(base.comment),
                number: get("number").unwrap_or(base.number),
            },
            operator: get("operator"),
            punctuation: get("punctuation"),
            foreground: get("foreground"),
            background: get("background"),
            selection: get("selection"),
        }
    }
}

/// Preset data uses hex and a bounded CSS rgba form for selection. Decode
/// RGB with the Rune sRGB conversion (the renderer uses linear colors).
fn parse_color(value: &str) -> Option<Color> {
    if let Some(hex) = color::hex(value) {
        return Some(hex);
    }
    let mut parts = value.strip_prefix("rgba(")?.strip_suffix(')')?.split(',').map(str::trim);
    let byte = |s: &str| s.parse::<u8>().ok();
    let r = byte(parts.next()?)?;
    let g = byte(parts.next()?)?;
    let b = byte(parts.next()?)?;
    let a: f32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(0.0..=1.0).contains(&a) {
        return None;
    }
    Some(Color {
        r: color::srgb_decode(f32::from(r) / 255.0),
        g: color::srgb_decode(f32::from(g) / 255.0),
        b: color::srgb_decode(f32::from(b) / 255.0),
        a,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql_editor::highlight_with_palette;

    #[test]
    fn typed_ids_are_exact_and_all_named_palettes_parse() {
        assert_eq!(SqlSyntaxTheme::from_id("default"), Some(SqlSyntaxTheme::DEFAULT));
        assert_eq!(SqlSyntaxTheme::from_id("DRACULA"), None);
        assert_eq!(SqlSyntaxTheme::from_id("unknown"), None);
        for preset in SQL_SYNTAX_PRESETS {
            let theme = SqlSyntaxTheme::from_id(preset.id).unwrap();
            assert_eq!(theme.id(), preset.id);
            for palette in [preset.light, preset.dark] {
                assert!(palette.entries.iter().all(|(_, value)| parse_color(value).is_some()));
            }
        }
    }

    #[test]
    fn default_retains_all_live_roles_and_follows_light_dark() {
        for dark in [false, true] {
            let resolved = SyntaxPalette::live(SqlSyntaxTheme::DEFAULT, dark);
            assert_eq!(resolved.tokens, palette::code_palette(dark));
            assert_eq!(resolved.foreground, None);
            assert_eq!(resolved.background, None);
            assert_eq!(resolved.selection, None);
            assert_eq!(resolved.operator, None);
            assert_eq!(resolved.punctuation, None);
        }
    }

    #[test]
    fn real_lexer_uses_the_same_named_colors_as_preview() {
        let theme = SqlSyntaxTheme::from_id("dracula").unwrap();
        let sql = "SELECT 42 WHERE id = '雪'; -- tail";
        let expected = [
            ("SELECT", "#ff79c6"),
            ("42", "#bd93f9"),
            ("=", "#ff79c6"),
            ("'雪'", "#f1fa8c"),
            (";", "#f8f8f2"),
            ("-- tail", "#6272a4"),
        ];
        for resolved in [SyntaxPalette::live(theme, true), SyntaxPalette::preview(theme, true)] {
            assert_eq!(resolved.background.unwrap(), color::hex("#282a36").unwrap());
            assert_eq!(resolved.selection.unwrap().a, 0.6);
            let spans = highlight_with_palette(sql, resolved);
            for (token, hex) in expected {
                let span = spans.iter().find(|span|
                    &sql[span.start as usize..span.end as usize] == token
                ).unwrap();
                assert_eq!(span.color, color::hex(hex));
            }
        }
        let default = highlight_with_palette(sql, SyntaxPalette::live(SqlSyntaxTheme::DEFAULT, true));
        assert_ne!(default[0].color, color::hex("#ff79c6"));
        let light = highlight_with_palette(sql, SyntaxPalette::live(theme, false));
        assert_eq!(light[0].color, color::hex("#e3116c"));
    }
}
