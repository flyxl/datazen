//! The live native Data Browsing view presents only preferences with real host writes.

use datazen_ui_api::types::{RowCountMode, WorkflowStepResultOrder};
use datazen_ui_api::{BuiltinLanguage, DefaultTableSort, GridDensity, Setting};
use rune::{col, row, text, tok, Color, El, Shadow, TextEdit, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Placement};

use super::settings_content_state::{SettingsContentState, SettingsSelect, RESULT_LIMIT_OPTIONS};
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
const CUSTOM_VALUE: &str = "__custom_rows__";
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
    let legacy_max_rows = settings
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
    mount_custom_rows(state, dark, &k, &mut overlays);
    let row_count = state.desktop.read_native_row_count_mode().ok();
    let default_sort = state.desktop.read_native_default_table_sort().ok();
    let density = state.desktop.read_native_grid_density().ok();
    let striped = state.desktop.read_native_grid_alternating_rows().ok();
    let row_numbers = state.desktop.read_native_grid_row_numbers().ok();
    let auto_hide_scrollbar = state.desktop.read_native_grid_auto_hide_scrollbar().ok();
    let select_limit = state.desktop.read_native_select_limit().ok();
    let auto_chart = state.desktop.read_native_auto_chart_on_query().ok();
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
    if let Some(density) = density {
        section = section.child(grid_density_row(state, density, &k, &mut overlays));
    }
    if let Some(striped) = striped {
        section = section.child(grid_appearance_toggle(
            "settings-grid-alternating-rows",
            t("settings.gridAlternatingRows"),
            t("settings.gridAlternatingRows.hint"),
            striped,
            &k,
            |state| {
                let current = state.desktop.read_native_grid_alternating_rows()?;
                state.desktop.write_native_grid_alternating_rows(!current)
            },
        ));
    }
    if let Some(visible) = row_numbers {
        section = section.child(grid_appearance_toggle(
            "settings-grid-row-numbers",
            t("settings.gridRowNumbers"),
            t("settings.gridRowNumbers.hint"),
            visible,
            &k,
            |state| {
                let current = state.desktop.read_native_grid_row_numbers()?;
                state.desktop.write_native_grid_row_numbers(!current)
            },
        ));
    }
    if let Some(auto_hide) = auto_hide_scrollbar {
        section = section.child(grid_appearance_toggle(
            "settings-grid-auto-hide-scrollbar",
            t("settings.gridAutoHideScrollbar"),
            t("settings.gridAutoHideScrollbar.hint"),
            auto_hide,
            &k,
            |state| {
                let current = state.desktop.read_native_grid_auto_hide_scrollbar()?;
                state.desktop.write_native_grid_auto_hide_scrollbar(!current)
            },
        ));
    }
    if let Some(limit) = select_limit {
        section = section.child(select_limit_row(limit.effective_rows.is_some(), &k));
    }
    // No dormant editor when the switch is off. A legacy host without a
    // switch still supports its original numeric preference.
    if let Some(rows) = select_limit.map_or(Some(legacy_max_rows), |limit| limit.effective_rows) {
        let select = result_limit_select(state, rows, &k, &mut overlays);
        section = section.child(setting_row(
            &k, &mut overlays, "settings-max-rows-row".to_owned(),
            t("settings.maxRows"), None, select,
        ));
    }
    if let Some(enabled) = auto_chart {
        let (label, hint) = match state.builtin_language() {
            BuiltinLanguage::English => (
                "Auto-switch to chart (simple results)",
                "Open Chart after a complete SELECT with 1–12 nonnegative whole-number values in its chart series.",
            ),
            BuiltinLanguage::SimplifiedChinese => (
                "查询后自动切换图表（简单结果）",
                "一条 SELECT 完整返回 1–12 个非负整数图表值时，自动打开图表。",
            ),
        };
        section = section.child(grid_appearance_toggle(
            "settings-auto-chart", label, hint,
            enabled,
            &k,
            |state| {
                let current = state.desktop.read_native_auto_chart_on_query()?;
                state.desktop.write_native_auto_chart_on_query(!current)
            },
        ));
    }
    section = section.child(workflow_step_order_row(state, &k, &mut overlays));
    if state.settings_error.is_some() {
        section = section.child(settings_error_row(state, theme));
    }

    let some_grid_unsupported = density.is_none() || striped.is_none()
        || row_numbers.is_none() || auto_hide_scrollbar.is_none();
    let unavailable = match state.builtin_language() {
        BuiltinLanguage::English => {
            let mut fields = vec!["time display", "connection pool"];
            if row_count.is_none() { fields.push("row count mode"); }
            if default_sort.is_none() { fields.push("default sorting"); }
            if some_grid_unsupported { fields.push("some grid settings"); }
            if select_limit.is_none() { fields.push("the SELECT-limit toggle"); }
            if auto_chart.is_none() { fields.push("auto-chart"); }
            let mut message = format!("Settings not available here: {}.", fields.join(", "));
            if row_count == Some(RowCountMode::Auto) {
                message.push_str(" Automatic uses the database's default counting strategy; the estimate threshold is not configurable in Native.");
            }
            message
        }
        BuiltinLanguage::SimplifiedChinese => {
            let mut fields = vec!["时间显示", "连接池"];
            if row_count.is_none() { fields.push("行数统计"); }
            if default_sort.is_none() { fields.push("默认排序"); }
            if some_grid_unsupported { fields.push("部分表格设置"); }
            if select_limit.is_none() { fields.push("SELECT 限制开关"); }
            if auto_chart.is_none() { fields.push("自动图表"); }
            let mut message = format!("此处暂不支持{}设置。", fields.join("、"));
            if row_count == Some(RowCountMode::Auto) {
                message.push_str("自动模式使用数据库默认的计数策略；Native 暂不支持设置估算阈值。");
            }
            message
        }
    };
    // Export and Import keep their existing entry until their replacement
    // paths are wired. Demo hosts without the Tools Backup destination also
    // keep their existing backup form.
    let mut file_tools = col()
        .key("settings-data-tools")
        .w_full()
        .gap(12.0)
        .pt(16.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        .child(export_section(state, theme, dark))
        .child(import_section(state, theme, dark));
    if !state.supports_tools_backup() {
        file_tools = file_tools.child(backup_section(state, theme));
    }
    section
        .child(file_tools)
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

fn select_limit_row(checked: bool, k: &Kit) -> El {
    let label = t("settings.limitSelect");
    let switch = row()
        .key("settings-limit-select-results")
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
                state.settings_error = match state.desktop.read_native_select_limit() {
                    Ok(limit) => state.desktop.write_native_select_limit(limit.effective_rows.is_none()).err(),
                    Err(error) => Some(error),
                };
            }
        });
    row().key("settings-limit-select-results-row").w_full().justify_between().center_y()
        .child(text(label).font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_muted))
        .child(switch)
}

fn grid_appearance_toggle(
    key: &str,
    label: &str,
    hint: &str,
    checked: bool,
    k: &Kit,
    toggle_preference: impl Fn(&mut AppState) -> Result<(), String> + 'static,
) -> El {
    // React's white ToggleRow knob uses Tailwind's two-layer `shadow`.
    let knob_shadow = Color::BLACK.alpha(26.0 / 255.0);
    let toggle = row()
        .key(key)
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
        .child(
            col()
                .size(14.0, 14.0)
                .radius(7.0)
                .fill(Color::WHITE)
                .shadow(1.0, 3.0, knob_shadow)
                .box_shadow(Shadow {
                    y: 1.0,
                    blur: 2.0,
                    spread: -1.0,
                    color: knob_shadow,
                    ..Shadow::default()
                })
                .a11y_hidden(),
        )
        .on_press(move |state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing" {
                state.settings_error = toggle_preference(state).err();
            }
        });
    row().key(format!("{key}-row")).w_full().justify_between().center_y()
        .child(row().gap(8.0).center_y()
            .child(text(label).font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_muted))
            .child(data_hint(k, key, label, hint)))
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
    hinted_setting_row(k, "settings-default-table-sort-row", label, t("settings.defaultTableSort.hint"), select)
}

fn grid_density_row(state: &AppState, density: GridDensity, k: &Kit, overlays: &mut Vec<El>) -> El {
    let label = t("settings.gridDensity");
    let values = GridDensity::ALL.map(GridDensity::as_str);
    let labels = GridDensity::ALL.map(|density| t(density.label_key()));
    let select = Select::new("settings-grid-density", &labels)
        .values(&values)
        .current(Some(t(density.label_key())))
        .label(label)
        .open(state.settings_content.open_select == Some(SettingsSelect::GridDensity))
        .on_toggle(|state: &mut AppState| {
            if data_browsing_route(state) {
                state.settings_content.toggle_select(SettingsSelect::GridDensity);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::GridDensity) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if !data_browsing_route(state) || state.settings_content.open_select != Some(SettingsSelect::GridDensity) {
                return;
            }
            state.settings_content.close_select();
            if let Some(density) = GridDensity::from_wire(value) {
                state.settings_error = state.desktop.write_native_grid_density(density).err();
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT)
        .radius(9.0)
        .border(1.0, k.c.border_base)
        .pl(3.0)
        .pr(8.0);
    hinted_setting_row(k, "settings-grid-density-row", label, t("settings.gridDensity.hint"), select)
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
    hinted_setting_row(k, "settings-workflow-step-order-row", label, t("settings.workflowStepResultOrderHint"), select)
}

/// The stock native help overlay is driven by actual pointer movement.
fn data_hint(k: &Kit, key: &str, label: &str, hint: &str) -> El {
    row().key(format!("{key}-hint-tooltip-trigger")).size(16.0, 16.0).child(
        row().key(format!("{key}-hint-mark"))
            .size(16.0, 16.0).center().radius(8.0)
            .border(1.0, k.c.border_base)
            .help(hint).help_above()
            .label(label).description(hint)
            .focusable(true).focus_ring(2.0, 1.0, k.c.border_focus)
            .child(text("?").font(TextStyle::new(11.0, 400.0).lh(16.5))
                .color(k.c.text_faint).a11y_hidden()),
    )
}

/// React wraps the label within its 160px column, reserving a nonshrinking
/// hint and the 4px row gap + 4px hint margin.
fn hinted_setting_row(k: &Kit, key: &str, label: &str, hint: &str, select: El) -> El {
    const LABEL_WIDTH: f32 = 160.0;
    const HINT_WIDTH: f32 = 16.0;
    const HINT_GAP: f32 = 8.0;
    row()
        .key(key)
        .w_full()
        .gap(16.0)
        .items_start()
        .child(
            row()
                .key(format!("{key}-label"))
                .w(LABEL_WIDTH)
                .min_h(FIELD_HEIGHT)
                .gap(HINT_GAP)
                .center_y()
                .child(
                    text(label.to_owned())
                        .key(format!("{key}-label-text"))
                        .font(TextStyle::new(14.0, 400.0).lh(20.0))
                        .color(k.c.text_muted)
                        .min_w(0.0)
                        .max_w(LABEL_WIDTH - HINT_WIDTH - HINT_GAP)
                        .shrink()
                        .wrap(),
                )
                .child(data_hint(k, key, label, hint)),
        )
        .child(col().key(format!("{key}-content")).min_w(0.0).grow(1.0).child(select))
}

fn page_size_select(state: &AppState, page_size: u32, k: &Kit, overlays: &mut Vec<El>) -> El {
    let mut labels = PRESETS.map(|rows| format!("{rows} {}", t("common.rows"))).to_vec();
    let mut values = PRESETS.map(|rows| rows.to_string()).to_vec();
    let (custom, _, _, _) = CustomRows::Page.copy(state.builtin_language());
    labels.push(custom.to_owned());
    values.push(CUSTOM_VALUE.to_owned());
    let selected = format!("{page_size} {}", t("common.rows"));
    Select::new("settings-page-size-preset", &labels)
        .values(&values)
        .current(Some(&selected))
        .label(t("settings.defaultPageSize"))
        .open(state.settings_content.open_select == Some(SettingsSelect::PageSize))
        .on_toggle(|state: &mut AppState| {
            if data_browsing_route(state) {
                state.settings_content.toggle_select(SettingsSelect::PageSize);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::PageSize) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, tx| {
            if !data_browsing_route(state) || state.settings_content.open_select != Some(SettingsSelect::PageSize) {
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

fn data_browsing_route(state: &AppState) -> bool {
    state.active == crate::types::Section::Settings && state.settings_section() == "dataBrowsing"
}

/// Both native number editors share the same modal behavior. Only the
/// active one owns focus, its own draft and a transient validation error.
#[derive(Clone, Copy)]
enum CustomRows { Page, Result }
impl CustomRows {
    fn open(state: &AppState) -> Option<Self> {
        if !data_browsing_route(state) { return None; }
        match state.settings_content.open_select? {
            SettingsSelect::CustomPageSize => Some(Self::Page),
            SettingsSelect::CustomResultLimit => Some(Self::Result),
            _ => None,
        }
    }

    fn trigger(self) -> &'static str {
        match self { Self::Page => PAGE_SIZE_TRIGGER, Self::Result => "settings-query-result-limit-trigger" }
    }

    fn input(self) -> &'static str {
        match self { Self::Page => PAGE_SIZE_INPUT, Self::Result => "settings-max-rows-input" }
    }

    fn key(self) -> &'static str {
        match self { Self::Page => "settings-page-size-custom", Self::Result => "settings-result-limit-custom" }
    }

    fn draft(self, state: &AppState) -> &TextEdit {
        match self { Self::Page => &state.page_size_input, Self::Result => &state.max_rows_input }
    }

    fn copy(self, language: BuiltinLanguage) -> (&'static str, &'static str, &'static str, &'static str) {
        match (self, language) {
            (Self::Page, BuiltinLanguage::English) => ("Custom…", "Custom page size", "Rows per page (1–500)", "Set page size"),
            (Self::Page, BuiltinLanguage::SimplifiedChinese) => ("自定义…", "自定义每页行数", "每页行数（1–500）", "设定每页行数"),
            (Self::Result, BuiltinLanguage::English) => ("Custom…", "Custom result limit", "Maximum rows (1–10,000)", "Set result limit"),
            (Self::Result, BuiltinLanguage::SimplifiedChinese) => ("自定义…", "自定义返回行数", "最大返回行数（1–10,000）", "设置返回行数"),
        }
    }
}

fn dismiss_custom_rows(state: &mut AppState, tx: &mut Tx) {
    if let Some(field) = CustomRows::open(state) {
        state.settings_content.close_select();
        tx.focus(field.trigger(), tx.focus_visible());
    }
}

fn commit_custom_rows(state: &mut AppState, tx: &mut Tx) {
    let Some(field) = CustomRows::open(state) else { return; };
    // Reject stale events after disabling a live cap; a legacy host has
    // no separate switch. Both setters validate before a profile write.
    if matches!(field, CustomRows::Result) && state.desktop.read_native_select_limit()
        .is_ok_and(|limit| limit.effective_rows.is_none()) {
        dismiss_custom_rows(state, tx);
        return;
    }
    let prior_error = state.settings_error.take();
    match field {
        CustomRows::Page => state.set_table_page_size(),
        CustomRows::Result => state.set_query_max_rows(),
    }
    let error = std::mem::replace(&mut state.settings_error, prior_error);
    if let Some(error) = error {
        state.settings_content.custom_number_error = Some(error);
        tx.focus(field.input(), tx.focus_visible());
    } else {
        dismiss_custom_rows(state, tx);
    }
}

fn mount_custom_rows(state: &AppState, dark: bool, k: &Kit, overlays: &mut Vec<El>) {
    let Some(field) = CustomRows::open(state) else { return; };
    let (_, title, hint, save_label) = field.copy(state.builtin_language());
    let mut body = col()
        .key(field.key())
        .role("dialog")
        .label(title)
        .description(hint)
        .w(320.0)
        .padding(12.0)
        .gap(8.0)
        .modal_tx(true, dismiss_custom_rows)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(hint).font(k.ty.ui_compact).color(k.c.text_muted))
        .child(
            TextField::new(field.input(), field.draft(state))
                .height(FIELD_HEIGHT)
                .fill(k.c.bg_base)
                .border(1.0, k.c.border_base)
                .radius(k.radius.md)
                .label(match field { CustomRows::Page => t("settings.defaultPageSize"), CustomRows::Result => t("settings.maxRows") })
                .on_edit(move |state: &mut AppState| match field {
                    CustomRows::Page => &mut state.page_size_input, CustomRows::Result => &mut state.max_rows_input,
                })
                .on_submit(commit_custom_rows)
                .build(dark),
        );
    if let Some(error) = &state.settings_content.custom_number_error {
        body = body.child(
            text(error.clone())
                .key(format!("{}-error", field.key()))
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
                .key(format!("{}-cancel", field.key())).focusable(true)
                .on_press_tx(dismiss_custom_rows))
            .child(Button::new(t("common.save")).control_size(ControlSize::Regular).build(k)
                .key(match field { CustomRows::Page => "settings-page-size-set", CustomRows::Result => "settings-max-rows-set" }).focusable(true).label(save_label)
                .on_press_tx(commit_custom_rows)),
    );
    overlays.push(rune_kit::popover::anchored(
        k, rune_kit::popover::raised(k, body, k.radius.md),
        field.trigger(), Placement::BottomStart, 4.0,
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

fn result_limit_select(state: &AppState, rows: u32, k: &Kit, overlays: &mut Vec<El>) -> El {
    // The React reference also offers 50,000. The native result publisher
    // only admits 1..=10,000, including the lookahead row contract.
    let presets: Vec<_> = RESULT_LIMIT_OPTIONS.iter().copied()
        .filter(|rows| (Setting::QueryMaxRows { rows: *rows }).validate().is_ok())
        .collect();
    let caption = |n| format!("{} {}", SettingsContentState::formatted_rows(n), t("common.rows"));
    let mut labels: Vec<String> = presets.iter().copied().map(caption).collect();
    let mut values: Vec<String> = presets.iter().map(u32::to_string).collect();
    labels.push(CustomRows::Result.copy(state.builtin_language()).0.to_owned());
    values.push(CUSTOM_VALUE.to_owned());
    Select::new("settings-query-result-limit", &labels)
        .values(&values)
        .current(Some(&caption(rows)))
        .label(t("settings.maxRows"))
        .open(state.settings_content.open_select == Some(SettingsSelect::ResultLimit))
        .on_toggle(|state: &mut AppState| {
            if data_browsing_route(state) { state.settings_content.toggle_select(SettingsSelect::ResultLimit); }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::ResultLimit) { state.settings_content.close_select(); }
        })
        .on_choose(move |state: &mut AppState, value, tx| {
            if !data_browsing_route(state) || state.settings_content.open_select != Some(SettingsSelect::ResultLimit) { return; }
            state.settings_content.close_select();
            let current = match state.desktop.read_native_select_limit() {
                Ok(limit) => match limit.effective_rows { Some(rows) => rows, None => return },
                Err(_) => rows, // legacy host: no native toggle
            };
            if value == CUSTOM_VALUE {
                state.max_rows_input = TextEdit::new(current.to_string());
                state.settings_content.open_select = Some(SettingsSelect::CustomResultLimit);
                tx.focus(CustomRows::Result.input(), tx.focus_visible());
            } else if let Ok(rows) = value.parse::<u32>() {
                if presets.contains(&rows) {
                    state.settings_error = state.write_ui_setting(Setting::QueryMaxRows { rows }).err();
                }
            }
        })
        .build(&preset_field_kit(k), overlays)
        .h(FIELD_HEIGHT).radius(9.0).border(1.0, k.c.border_base).pl(3.0).pr(8.0)
}
