//! Built-in syntax themes for the actual native SQL workspace.
use datazen_ui_api::{EditorFontFamily, SqlSyntaxTheme, SQL_SYNTAX_PRESETS};
use rune::{col, text, Color, El, Span, TextStyle};
use rune_kit::Kit;

use super::{settings_content_state::SettingsSelect, settings_ui::setting_row};
use crate::i18n::t;
use crate::sql_editor::{highlight_with_palette, SyntaxPalette};
use crate::state_def::AppState;
use crate::ui::select::Select;

const PREVIEW_SQL: &str = concat!(
    "-- Sample query preview\n",
    "SELECT u.name, COUNT(o.id) AS total\n",
    "FROM users u\n",
    "WHERE u.status = 'active' AND u.age > 18\n",
    "ORDER BY total DESC;",
);

fn font_label(family: EditorFontFamily) -> &'static str {
    match family {
        EditorFontFamily::Bundled => t("settings.nativeEditorFont.bundled"),
        EditorFontFamily::System => t("settings.nativeEditorFont.system"),
    }
}

pub(super) fn font_family_row(state: &AppState, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let label = t("settings.fontFamily");
    let which = SettingsSelect::EditorFontFamily;
    let select = Select::new("settings-editor-font-family", &EditorFontFamily::MODES.map(font_label))
        .values(&EditorFontFamily::MODES.map(EditorFontFamily::css_family))
        .current(Some(font_label(state.editor_font_family())))
        .label(label)
        .open(state.settings_content.open_select == Some(which))
        .on_toggle(move |state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "editor" {
                state.settings_content.toggle_select(which);
            }
        })
        .on_close(move |state: &mut AppState| {
            if state.settings_content.open_select == Some(which) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if let Some(family) = EditorFontFamily::from_css_family(value) {
                state.save_editor_font_family(family);
            }
        });
    let field = editor_preference_trigger(select, kit, overlays);
    setting_row(kit, overlays, "settings-editor-font-family-row".to_owned(), label, None, field)
}

pub(super) fn row(state: &AppState, dark: bool, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let theme = state.sql_syntax_theme();
    let current = SQL_SYNTAX_PRESETS.iter().find(|preset| preset.id == theme.id())
        .expect("validated preset ID");
    let labels = SQL_SYNTAX_PRESETS.map(|preset| preset.label);
    let values = SQL_SYNTAX_PRESETS.map(|preset| preset.id);
    let label = t("settings.sqlSyntaxTheme");
    let select = Select::new("settings-sql-syntax-theme", &labels)
        .values(&values)
        .current(Some(current.label))
        .label(label)
        .open(state.settings_content.open_select == Some(SettingsSelect::SyntaxTheme))
        .on_toggle(|state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "editor" {
                state.settings_content.toggle_select(SettingsSelect::SyntaxTheme);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::SyntaxTheme) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if let Some(theme) = SqlSyntaxTheme::from_id(value) {
                state.save_sql_syntax_theme(theme);
            }
        });
    let select = editor_preference_trigger(select, kit, overlays);
    setting_row(
        kit,
        overlays,
        "settings-live-syntax-theme".to_owned(),
        label,
        Some(t("settings.sqlSyntaxThemeHint")),
        col().w_full().gap(8.0).child(select).child(preview(theme, state.editor_font_family(), dark, kit)),
    )
}

/// The original setting uses a 1pt inside edge on a 36pt field, not
/// the generic Kit control's softer outside elevation.
fn editor_preference_trigger(select: Select, kit: &Kit, overlays: &mut Vec<El>) -> El {
    select
        .build(&syntax_field_kit(kit), overlays)
        .h(36.0)
        .radius(9.0)
        .border(1.0, kit.c.border_base)
        // Match the original px-2.5 plus its border; the inner Kit
        // value slot already contributes most of the leading inset.
        .pl(3.0)
        .pr(8.0)
}

fn syntax_field_kit(kit: &Kit) -> Kit {
    let mut field = (*kit).clone();
    field.elevation = field.elevation.with_input(Vec::<rune::Shadow>::new());
    // The kit uses button-neutral as a fallback for an empty input list.
    field.elevation.button_neutral.clear();
    field.ty.ui_compact = TextStyle::new(14.0, 400.0).lh(20.0);
    field.c.icon_muted = kit.c.text_faint;
    field
}

fn preview_radius(dark: bool) -> f32 {
    // The original rounded-lg follows each stock appearance token.
    if dark { 12.0 } else { 11.0 }
}

fn preview(theme: SqlSyntaxTheme, family: EditorFontFamily, dark: bool, kit: &Kit) -> El {
    let colors = SyntaxPalette::preview(theme, dark);
    let spans = highlight_with_palette(PREVIEW_SQL, colors);
    // A horizontal viewport lays its direct children out side by side.
    // Give it one naturally sized, vertical code block, so the longest
    // line can scroll without turning the five lines into a filmstrip.
    let mut lines = col()
        .key("settings-sql-syntax-preview-content")
        .items_start()
        .px(16.0)
        .py(12.0);
    let mut start = 0usize;
    let font = TextStyle::new(12.0, 400.0).lh(20.4)
        .family(crate::sql_editor::query_font_family(family));
    let foreground = colors.foreground.unwrap_or(Color::BLACK);
    for (index, line) in PREVIEW_SQL.split('\n').enumerate() {
        let end = start + line.len();
        let row_spans: Vec<_> = spans.iter().filter_map(|span| {
            let lo = (span.start as usize).max(start);
            let hi = (span.end as usize).min(end);
            (lo < hi).then(|| {
                let mut relative = Span::new(lo - start, hi - start);
                relative.color = span.color;
                relative.weight = span.weight;
                relative.italic = span.italic;
                relative
            })
        }).collect();
        lines = lines.child(
            text(line)
                .key(format!("settings-sql-syntax-preview-line-{index}"))
                .font(font)
                .color(foreground)
                .spans(row_spans),
        );
        start = end + 1;
    }
    let radius = preview_radius(dark);
    col()
        .key("settings-sql-syntax-preview")
        .w_full()
        .radius(radius)
        .clip_rounded(radius)
        .border(1.0, kit.c.border_base)
        // CSS puts the five-line pre inside a 1px border box. Rune
        // paints the edge but does not add it to the child's quadrant.
        .padding(1.0)
        .fill(colors.background.unwrap_or(kit.c.bg_base))
        .child(
            col()
                .key("settings-sql-syntax-preview-scroll")
                .w_full()
                .scroll_x()
                .child(lines),
        )
}

#[cfg(test)]
mod visual_tests {
    use super::*;

    #[test]
    fn editor_skin_preserves_original_local_select_and_five_live_preview_lines() {
        for dark in [false, true] {
            let shared = crate::ui::kit::kit(dark);
            let trigger = syntax_field_kit(shared);
            assert!(trigger.elevation.input().is_empty());
            assert!(!shared.elevation.input().is_empty());
            assert_eq!(trigger.elevation.floating.len(), shared.elevation.floating.len());
            assert_eq!(trigger.c.border_focus, shared.c.border_focus);
            assert_eq!(trigger.c.icon_muted, shared.c.text_faint);

            fn view(dark: &bool, _: &mut rune::Cx<bool>) -> El {
                preview(SqlSyntaxTheme::DEFAULT, EditorFontFamily::Bundled, *dark, crate::ui::kit::kit(*dark))
            }
            let mut rt = rune::Runtime::new(dark, view, (496.0, 240.0), rune::Env::new(dark, 1.0));
            rt.frame(0.0);
            let outer = rt.presented("settings-sql-syntax-preview").unwrap().0;
            let inner = rt.presented("settings-sql-syntax-preview-scroll").unwrap().0;
            assert_eq!(inner[0] - outer[0], 1.0);
            assert_eq!(inner[1] - outer[1], 1.0);
            assert_eq!(outer[2] - inner[2], 2.0);
            for line in 0..5 {
                assert!(rt.presented(&format!("settings-sql-syntax-preview-line-{line}")).is_some());
            }
        }
    }
}
