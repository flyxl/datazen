//! The live native Data Browsing view presents only preferences with real host writes.

use datazen_ui_api::{BuiltinLanguage, Setting};
use rune::{col, row, text, tok, El, TextEdit, TextStyle};
use rune_kit::Kit;

use super::settings_content_state::SettingsSelect;
use super::settings_ui::{section_title, setting_row};
use super::{backup_section, export_section, import_section, settings_error_row};
use crate::constants::TABLE_PAGE_SIZE;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::ui::{field::TextField, kit::kit, select::Select};

// The original choices are all in the native API's 1..=500 range. Manual
// entries still support every integer in that range, including custom values.
const PRESETS: [u32; 5] = [25, 50, 100, 200, 500];
const FIELD_HEIGHT: f32 = 36.0;

pub(super) fn view(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let settings = state.ui_settings();
    let page_size = settings
        .iter()
        .find_map(|setting| match setting {
            Setting::TablePageSize { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(TABLE_PAGE_SIZE);
    let max_rows = settings
        .iter()
        .find_map(|setting| match setting {
            Setting::QueryMaxRows { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(100);
    let k = kit(dark);
    let mut overlays = Vec::new();
    let mut section = col()
        .key("settings-section-dataBrowsing")
        .w_full()
        .gap(20.0)
        .child(section_title(
            &k,
            &mut overlays,
            "settings-data-browsing-title".to_owned(),
            t("settings.dataBrowsing"),
            None,
        ));
    let presets = page_size_select(state, page_size, &k, &mut overlays);
    section = section
        .child(setting_row(
            &k,
            &mut overlays,
            "settings-page-size-preset-row".to_owned(),
            t("settings.defaultPageSize"),
            None,
            presets,
        ))
        .child(
            col()
                .key("settings-data")
                .w_full()
                .gap(12.0)
                .child(setting_row(
                    &k,
                    &mut overlays,
                    "settings-page-size-manual-row".to_owned(),
                    &format!("Rows per page: {page_size}"),
                    None,
                    number_row(
                        &k,
                        dark,
                        NumberControl {
                            input_key: "settings-page-size-input",
                            draft: &state.page_size_input,
                            field_label: t("settings.defaultPageSize"),
                            edit: |state| &mut state.page_size_input,
                            set_key: "settings-page-size-set",
                            set_label: "Set page size",
                            save: AppState::set_table_page_size,
                        },
                    ),
                ))
                .child(setting_row(
                    &k,
                    &mut overlays,
                    "settings-max-rows-row".to_owned(),
                    &format!("Query max rows: {max_rows}"),
                    None,
                    number_row(
                        &k,
                        dark,
                        NumberControl {
                            input_key: "settings-max-rows-input",
                            draft: &state.max_rows_input,
                            field_label: t("settings.maxRows"),
                            edit: |state| &mut state.max_rows_input,
                            set_key: "settings-max-rows-set",
                            set_label: "Set query max rows",
                            save: AppState::set_query_max_rows,
                        },
                    ),
                )),
        );
    if state.settings_error.is_some() {
        section = section.child(settings_error_row(state, theme));
    }

    let unavailable = match state.builtin_language() {
        BuiltinLanguage::English => concat!(
            "Row counting, default sorting, grid and time display, connection pool, ",
            "SELECT-limit toggle, auto-chart, and workflow result ordering are not configurable here."
        ),
        BuiltinLanguage::SimplifiedChinese => concat!(
            "此处暂不支持行数统计、默认排序、表格和时间显示、",
            "连接池、SELECT 限制开关、自动图表及工作流结果顺序设置。"
        ),
    };
    // Keep the working native actions, even though React places them elsewhere.
    section
        .child(
            col()
                .key("settings-data-tools")
                .w_full()
                .gap(12.0)
                .pt(16.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
                .child(export_section(state, theme, dark))
                .child(import_section(state, theme, dark))
                .child(backup_section(state, theme)),
        )
        .child(
            text(unavailable)
                .key("settings-data-unsupported")
                .font(tok::type_::LABEL)
                .color(k.c.text_muted)
                .w_full()
                .wrap(),
        )
        .children(overlays)
}

fn page_size_select(state: &AppState, page_size: u32, k: &Kit, overlays: &mut Vec<El>) -> El {
    let labels = PRESETS.map(|rows| format!("{rows} {}", t("common.rows")));
    let values = PRESETS.map(|rows| rows.to_string());
    let selected = format!("{page_size} {}", t("common.rows"));
    Select::new("settings-page-size-preset", &labels)
        .values(&values)
        .current(Some(&selected))
        .label(t("settings.defaultPageSize"))
        .open(state.settings_content.open_select == Some(SettingsSelect::PageSize))
        .on_toggle(|state: &mut AppState| {
            state.settings_content.toggle_select(SettingsSelect::PageSize)
        })
        .on_close(|state: &mut AppState| state.settings_content.close_select())
        .on_choose(|state: &mut AppState, value, _tx| {
            state.settings_content.close_select();
            if let Ok(rows) = value.parse::<u32>() {
                if PRESETS.contains(&rows) {
                    state.settings_error = state
                        .write_ui_setting(Setting::TablePageSize { rows })
                        .err();
                    if state.settings_error.is_none() {
                        state.page_size_input = TextEdit::new(rows.to_string());
                    }
                }
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT)
        .radius(9.0)
        .border(1.0, k.c.border_base)
        // Match the original Select's inset without changing the list
        // placement or stretching the row to imitate its scroller.
        .pl(3.0)
        .pr(8.0)
}

fn preset_field_kit(k: &Kit) -> Kit {
    let mut field = (*k).clone();
    // The actual React page-size field paints a 1px edge inside the
    // surface. An empty input shadow alone falls back to the neutral
    // button shadow. Remove both only for this Select.
    field.elevation = field.elevation.with_input(Vec::<rune::Shadow>::new());
    field.elevation.button_neutral.clear();
    field.ty.ui_compact = TextStyle::new(14.0, 400.0).lh(20.0);
    field.c.icon_muted = k.c.text_faint;
    field
}

struct NumberControl<'a> {
    input_key: &'static str,
    draft: &'a TextEdit,
    field_label: &'a str,
    edit: fn(&mut AppState) -> &mut TextEdit,
    set_key: &'static str,
    set_label: &'static str,
    save: fn(&mut AppState),
}

fn number_row(k: &Kit, dark: bool, control: NumberControl<'_>) -> El {
    row()
        .w_full()
        .gap(8.0)
        .child(
            TextField::new(control.input_key, control.draft)
                .height(FIELD_HEIGHT)
                .grow()
                .fill(k.c.bg_base)
                .border(1.0, k.c.border_base)
                .radius(k.radius.md)
                .label(control.field_label)
                .on_edit(control.edit)
                .build(dark),
        )
        .child(
            row()
                .key(control.set_key)
                .h(FIELD_HEIGHT)
                .px(12.0)
                .center_y()
                .radius(k.radius.md)
                .fill(k.c.bg_base)
                .border(1.0, k.c.border_base)
                .label(control.set_label)
                .child(
                    text("Set")
                        .font(TextStyle::new(14.0, 400.0).lh(20.0))
                        .color(k.c.text_base),
                )
                .on_press(control.save),
        )
}
