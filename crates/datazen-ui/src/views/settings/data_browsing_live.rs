//! The live native Data Browsing view presents only preferences with real host writes.

use datazen_ui_api::types::{RowCountMode, WorkflowStepResultOrder};
use datazen_ui_api::{BuiltinLanguage, DefaultTableSort, Setting};
use rune::{col, row, text, tok, El, TextEdit, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Placement};

use super::setting_hint::SettingHint;
use super::settings_content_state::SettingsSelect;
use super::settings_ui::{section_title, setting_row};
use super::{backup_section, export_section, import_section, settings_error_row};
use crate::constants::TABLE_PAGE_SIZE;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::ui::{field::TextField, kit::kit, select::Select};

// Original presets stay in order; Native additionally discloses its existing
// validated custom size rather than placing a duplicate editor in the route.
const PRESETS: [u32; 5] = [25, 50, 100, 200, 500];
const FIELD_HEIGHT: f32 = 36.0;
const CUSTOM_VALUE: &str = "__custom_page_size__";
const PAGE_SIZE_TRIGGER: &str = "settings-page-size-preset-trigger";
const PAGE_SIZE_INPUT: &str = "settings-page-size-input";

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
    mount_custom_page_size(state, dark, &k, &mut overlays);
    let row_count = state.desktop.read_native_row_count_mode().ok();
    let default_sort = state.desktop.read_native_default_table_sort().ok();
    let striped = state.desktop.read_native_grid_alternating_rows().ok();
    section = section
        .child(setting_row(
            &k,
            &mut overlays,
            "settings-page-size-preset-row".to_owned(),
            t("settings.defaultPageSize"),
            None,
            presets,
        ));
    // Keep the original preference order, without reserving space for
    // controls that the native read path does not yet implement.
    if let Some(mode) = row_count {
        section = section.child(row_count_mode_row(state, mode, &k, &mut overlays));
    }
    if let Some(order) = default_sort {
        section = section.child(default_table_sort_row(state, order, &k, &mut overlays));
    }
    if let Some(striped) = striped {
        section = section.child(grid_alternating_rows(striped, &k, &mut overlays));
    }
    section = section
        .child(
            col()
                .key("settings-data")
                .w_full()
                .gap(12.0)
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
        )
        .child(workflow_step_order_row(state, &k, &mut overlays));
    if state.settings_error.is_some() {
        section = section.child(settings_error_row(state, theme));
    }

    let unavailable = match state.builtin_language() {
        BuiltinLanguage::English => format!(
            "Settings not available here: {}{}{}, time display, connection pool, the SELECT-limit toggle, and auto-chart.{}",
            if row_count.is_none() { "row count mode, " } else { "" },
            if default_sort.is_none() { "default sorting, " } else { "" },
            if striped.is_none() { "grid settings" } else { "other grid settings" },
            if row_count == Some(RowCountMode::Auto) {
                " Automatic uses the database's default counting strategy; the estimate threshold is not configurable in Native."
            } else { "" },
        ),
        BuiltinLanguage::SimplifiedChinese => format!(
            "此处暂不支持{}{}{}、时间显示、连接池、SELECT 限制开关及自动图表设置。{}",
            if row_count.is_none() { "行数统计、" } else { "" },
            if default_sort.is_none() { "默认排序、" } else { "" },
            if striped.is_none() { "表格设置" } else { "其他表格设置" },
            if row_count == Some(RowCountMode::Auto) {
                "自动模式使用数据库默认的计数策略；Native 暂不支持设置估算阈值。"
            } else { "" },
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

fn grid_alternating_rows(checked: bool, k: &Kit, overlays: &mut Vec<El>) -> El {
    const KEY: &str = "settings-grid-alternating-rows";
    let label = t("settings.gridAlternatingRows");
    let hint = t("settings.gridAlternatingRows.hint");
    let toggle = row()
        .key(KEY)
        .size(36.0, 20.0)
        .shrink()
        .radius(10.0)
        .fill(if checked { k.c.bg_accent } else { k.c.border_base })
        .pl(if checked { 18.0 } else { 3.0 })
        .center_y()
        .role("switch")
        .label(label)
        .checked(checked)
        .focusable(true)
        .focus_ring(2.0, 1.0, k.c.border_focus)
        .child(col().size(14.0, 14.0).radius(7.0).fill(rune::Color::WHITE).a11y_hidden())
        .on_press(|state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing" {
                state.settings_error = match state.desktop.read_native_grid_alternating_rows() {
                    Ok(current) => state.desktop.write_native_grid_alternating_rows(!current).err(),
                    Err(error) => Some(error),
                };
            }
        });
    row().key(format!("{KEY}-row")).w_full().justify_between().center_y()
        .child(row().gap(4.0).center_y()
            .child(text(label).font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_muted))
            .child(SettingHint::new(format!("{KEY}-hint"), label.to_owned(), hint.to_owned())
                .build(k, overlays)))
        .child(toggle)
}

fn row_count_mode_row(
    state: &AppState,
    mode: RowCountMode,
    k: &Kit,
    overlays: &mut Vec<El>,
) -> El {
    use RowCountMode::{Auto, Estimated, Exact, None as DoNotCount};
    let label = t("settings.rowCountMode");
    let labels = [
        t("settings.rowCountMode.auto"),
        t("settings.rowCountMode.exact"),
        t("settings.rowCountMode.estimated"),
        t("settings.rowCountMode.none"),
    ];
    let current = match mode {
        Auto => labels[0],
        Exact => labels[1],
        Estimated => labels[2],
        DoNotCount => labels[3],
    };
    let select = Select::new("settings-row-count-mode", &labels)
        .values(&[Auto.as_str(), Exact.as_str(), Estimated.as_str(), DoNotCount.as_str()])
        .current(Some(current))
        .label(label)
        .open(state.settings_content.open_select == Some(SettingsSelect::RowCountMode))
        .on_toggle(|state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing" {
                state.settings_content.toggle_select(SettingsSelect::RowCountMode);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::RowCountMode) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if state.active != crate::types::Section::Settings
                || state.settings_section() != "dataBrowsing"
                || state.settings_content.open_select != Some(SettingsSelect::RowCountMode)
            {
                return;
            }
            state.settings_content.close_select();
            if let Some(mode) = RowCountMode::from_wire(value) {
                state.settings_error = state.desktop.write_native_row_count_mode(mode).err();
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT)
        .radius(9.0)
        .border(1.0, k.c.border_base)
        .pl(3.0)
        .pr(8.0);
    setting_row(
        k,
        overlays,
        "settings-row-count-mode-row".to_owned(),
        label,
        None,
        select,
    )
}

fn default_table_sort_row(
    state: &AppState,
    order: DefaultTableSort,
    k: &Kit,
    overlays: &mut Vec<El>,
) -> El {
    use DefaultTableSort::{None as Unordered, PrimaryKeyAsc, PrimaryKeyDesc};
    let label = t("settings.defaultTableSort");
    let labels = [
        t("settings.defaultTableSort.primaryKeyAsc"),
        t("settings.defaultTableSort.primaryKeyDesc"),
        t("settings.defaultTableSort.none"),
    ];
    let current = match order {
        PrimaryKeyAsc => labels[0],
        PrimaryKeyDesc => labels[1],
        Unordered => labels[2],
    };
    let select = Select::new("settings-default-table-sort", &labels)
        .values(&[PrimaryKeyAsc.as_str(), PrimaryKeyDesc.as_str(), Unordered.as_str()])
        .current(Some(current))
        .label(label)
        .open(state.settings_content.open_select == Some(SettingsSelect::DefaultTableSort))
        .on_toggle(|state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing" {
                state.settings_content.toggle_select(SettingsSelect::DefaultTableSort);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::DefaultTableSort) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if state.active != crate::types::Section::Settings
                || state.settings_section() != "dataBrowsing"
                || state.settings_content.open_select != Some(SettingsSelect::DefaultTableSort)
            {
                return;
            }
            state.settings_content.close_select();
            if let Some(order) = DefaultTableSort::from_wire(value) {
                state.settings_error = state.desktop.write_native_default_table_sort(order).err();
                // Do not refresh a table or discard a draft from Settings.
                // The next normal read samples the host's saved preference.
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT)
        .radius(9.0)
        .border(1.0, k.c.border_base)
        .pl(3.0)
        .pr(8.0);
    setting_row(
        k,
        overlays,
        "settings-default-table-sort-row".to_owned(),
        label,
        Some(t("settings.defaultTableSort.hint")),
        select,
    )
}

fn workflow_step_order_row(state: &AppState, k: &Kit, overlays: &mut Vec<El>) -> El {
    use WorkflowStepResultOrder::{Asc, Desc};

    let label = t("settings.workflowStepResultOrder");
    let labels = [
        t("settings.workflowStepOrder.desc"),
        t("settings.workflowStepOrder.asc"),
    ];
    let current = match state.workflow_step_order() {
        Desc => labels[0],
        Asc => labels[1],
    };
    let select = Select::new("settings-workflow-step-order", &labels)
        .values(&[Desc.as_str(), Asc.as_str()])
        .current(Some(current))
        .label(label)
        .open(state.settings_content.open_select == Some(SettingsSelect::WorkflowOrder))
        .on_toggle(|state: &mut AppState| {
            state.settings_content.toggle_select(SettingsSelect::WorkflowOrder);
        })
        .on_close(|state: &mut AppState| state.settings_content.close_select())
        .on_choose(|state: &mut AppState, value, _tx| {
            if state.active != crate::types::Section::Settings
                || state.settings_section() != "dataBrowsing"
                || state.settings_content.open_select != Some(SettingsSelect::WorkflowOrder)
            {
                return;
            }
            state.settings_content.close_select();
            if let Some(order) = WorkflowStepResultOrder::from_wire(value) {
                state.settings_error = state.save_workflow_step_order(order).err();
                if state.settings_error.is_none() {
                    state.workflow_step_order_error = None;
                }
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT)
        .radius(9.0)
        .border(1.0, k.c.border_base)
        .pl(3.0)
        .pr(8.0);
    setting_row(
        k,
        overlays,
        "settings-workflow-step-order-row".to_owned(),
        label,
        Some(t("settings.workflowStepResultOrderHint")),
        select,
    )
}

fn page_size_select(state: &AppState, page_size: u32, k: &Kit, overlays: &mut Vec<El>) -> El {
    let mut labels = PRESETS.map(|rows| format!("{rows} {}", t("common.rows"))).to_vec();
    let mut values = PRESETS.map(|rows| rows.to_string()).to_vec();
    let (custom, _, _, _) = custom_page_size_copy(state.builtin_language());
    labels.push(custom.to_owned());
    values.push(CUSTOM_VALUE.to_owned());
    let selected = format!("{page_size} {}", t("common.rows"));
    Select::new("settings-page-size-preset", &labels)
        .values(&values)
        .current(Some(&selected))
        .label(t("settings.defaultPageSize"))
        .open(state.settings_content.open_select == Some(SettingsSelect::PageSize))
        .on_toggle(|state: &mut AppState| {
            if page_size_route(state) {
                state.settings_content.toggle_select(SettingsSelect::PageSize);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::PageSize) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, tx| {
            if !page_size_route(state) || state.settings_content.open_select != Some(SettingsSelect::PageSize) {
                return;
            }
            state.settings_content.close_select();
            if value == CUSTOM_VALUE {
                state.page_size_input = TextEdit::new(state.table_page_size().to_string());
                state.settings_content.open_select = Some(SettingsSelect::CustomPageSize);
                tx.focus(PAGE_SIZE_INPUT, tx.focus_visible());
                return;
            }
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

fn page_size_route(state: &AppState) -> bool {
    state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing"
}

fn custom_page_size_is_open(state: &AppState) -> bool {
    page_size_route(state) && state.settings_content.open_select == Some(SettingsSelect::CustomPageSize)
}

fn dismiss_custom_page_size(state: &mut AppState, tx: &mut Tx) {
    if custom_page_size_is_open(state) {
        state.settings_content.close_select();
        tx.focus(PAGE_SIZE_TRIGGER, tx.focus_visible());
    }
}

fn commit_custom_page_size(state: &mut AppState, tx: &mut Tx) {
    if !custom_page_size_is_open(state) {
        return;
    }
    // The established setter owns validation and the actual host write. A
    // custom draft error belongs to this popover, not the Query Limit row.
    let prior_error = state.settings_error.take();
    state.set_table_page_size();
    let custom_error = std::mem::replace(&mut state.settings_error, prior_error);
    if let Some(error) = custom_error {
        state.settings_content.custom_page_size_error = Some(error);
        tx.focus(PAGE_SIZE_INPUT, tx.focus_visible());
    } else {
        dismiss_custom_page_size(state, tx);
    }
}

fn custom_page_size_copy(language: BuiltinLanguage) -> (&'static str, &'static str, &'static str, &'static str) {
    match language {
        BuiltinLanguage::English => ("Custom…", "Custom page size", "Rows per page (1–500)", "Set page size"),
        BuiltinLanguage::SimplifiedChinese => ("自定义…", "自定义每页行数", "每页行数（1–500）", "设定每页行数"),
    }
}

fn mount_custom_page_size(state: &AppState, dark: bool, k: &Kit, overlays: &mut Vec<El>) {
    if !custom_page_size_is_open(state) {
        return;
    }
    let (_, title, hint, save_label) = custom_page_size_copy(state.builtin_language());
    let mut body = col()
        .key("settings-page-size-custom")
        .role("dialog")
        .label(title)
        .description(hint)
        .w(320.0)
        .padding(12.0)
        .gap(8.0)
        .modal_tx(true, dismiss_custom_page_size)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(hint).font(k.ty.ui_compact).color(k.c.text_muted))
        .child(
            TextField::new(PAGE_SIZE_INPUT, &state.page_size_input)
                .height(FIELD_HEIGHT)
                .fill(k.c.bg_base)
                .border(1.0, k.c.border_base)
                .radius(k.radius.md)
                .label(t("settings.defaultPageSize"))
                .on_edit(|state: &mut AppState| &mut state.page_size_input)
                .on_submit(commit_custom_page_size)
                .build(dark),
        );
    if let Some(error) = &state.settings_content.custom_page_size_error {
        body = body.child(
            text(error.clone())
                .key("settings-page-size-custom-error")
                .role("alert")
                .wrap()
                .color(k.c.danger.fg),
        );
    }
    body = body.child(
        row()
            .w_full()
            .justify_end()
            .gap(8.0)
            .child(Button::new(t("common.cancel")).variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Regular).build(k)
                .key("settings-page-size-custom-cancel").focusable(true)
                .on_press_tx(dismiss_custom_page_size))
            .child(Button::new(t("common.save")).control_size(ControlSize::Regular).build(k)
                .key("settings-page-size-set").focusable(true).label(save_label)
                .on_press_tx(commit_custom_page_size)),
    );
    overlays.push(rune_kit::popover::anchored(
        k, rune_kit::popover::raised(k, body, k.radius.md),
        PAGE_SIZE_TRIGGER, Placement::BottomStart, 4.0,
    ));
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
