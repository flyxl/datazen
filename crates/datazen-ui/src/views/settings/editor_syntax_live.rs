//! Built-in syntax themes for the actual native SQL workspace.
use datazen_ui_api::{SqlSyntaxTheme, SQL_SYNTAX_PRESETS};
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
        })
        .build(kit, overlays)
        .h(36.0);
    setting_row(
        kit,
        overlays,
        "settings-live-syntax-theme".to_owned(),
        label,
        Some(t("settings.sqlSyntaxThemeHint")),
        col().w_full().gap(8.0).child(select).child(preview(theme, dark, kit)),
    )
}

fn preview(theme: SqlSyntaxTheme, dark: bool, kit: &Kit) -> El {
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
    let font = TextStyle::new(12.0, 400.0).lh(20.4).mono();
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
    col()
        .key("settings-sql-syntax-preview")
        .w_full()
        .radius(kit.radius.md)
        .clip_rounded(kit.radius.md)
        .border(1.0, kit.c.border_base)
        .fill(colors.background.unwrap_or(kit.c.bg_base))
        .child(
            col()
                .key("settings-sql-syntax-preview-scroll")
                .w_full()
                .scroll_x()
                .child(lines),
        )
}
