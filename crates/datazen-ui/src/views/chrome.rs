//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;
use crate::i18n::t;

use crate::components::theme_toggle::{theme_icon_resolver, ThemeToggle};
use crate::constants::*;
use crate::icons::{self, section_icon, ICON_PANEL_LEFT_CLOSE, ICON_PANEL_LEFT_OPEN};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, row, text, text_input, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, TextStyle, Tx};

/// The original `bg-accent/20` over the fixed rail surface. These are
/// the flat sRGB fills from Chromium in both default themes; a native
/// compositor's linear-light alpha blend does not produce the same pixels.
fn rail_selected_fill(dark: bool, accent: Color) -> Color {
    let painted = if dark { "#1b3546" } else { "#d0ebf8" };
    rune::color::hex(painted).unwrap_or(accent.alpha(0.20))
}

const RAIL_ITEM_HEIGHT: f32 = 40.0;

pub fn rail_item(state: &AppState, section: Section, expanded: bool, dark: bool) -> El {
    let colors = crate::ui::theme::colors(dark);
    let selected = state.active == section;
    let label = section.label();
    let fg = if selected { colors.accent } else { colors.fg_secondary };
    let mut item = row()
        .key(format!("rail-{}", section.key()))
        .w_full()
        .h(RAIL_ITEM_HEIGHT)
        .px(if expanded { 12.0 } else { 0.0 })
        .center_y()
        .gap(if expanded { 10.0 } else { 0.0 })
        .radius(0.0)
        .role("tab")
        .label(label)
        .help(label)
        .on_press(move |state: &mut AppState| state.select(section));
    if selected {
        item = item.fill(rail_selected_fill(dark, colors.accent));
    } else {
        item = item.hover(colors.surface_raised).hover_group();
    }
    // On the inactive original button, :hover changes icon and label ink as
    // well as its fill. Do not apply the inactive hover style to selected rows.
    let glyph = rune::icon(section_icon(section), 16.0).color(fg).a11y_hidden();
    let glyph = if selected { glyph } else { glyph.group_color(colors.fg) };
    if expanded {
        let name = text(label)
            .key(format!("rail-{}-label", section.key()))
            .font(TextStyle::new(12.0, 400.0).lh(16.0))
            .color(fg)
            .grow(1.0)
            .min_w(0.0)
            .truncate();
        let name = if selected { name } else { name.group_color(colors.fg) };
        item.child(glyph).child(name)
    } else {
        item.justify_center().child(glyph)
    }
}

pub fn rail_view(state: &AppState, dark: bool) -> El {
    let colors = crate::ui::theme::colors(dark);
    let expanded = state.sidebar_expanded();
    let width = state.sidebar_rail_width() as f32;
    // React WorkspaceModeSidebar top group: Databases, Workflow, Dashboard,
    // Workspace, Extensions.
    let mut items = col().w_full();
    for section in [
        Section::Connections,
        Section::Workflows,
        Section::Dashboards,
        Section::Workspace,
        Section::Wapps,
    ] {
        items = items.child(rail_item(state, section, expanded, dark));
    }
    // Windows/Linux already expose the real Tools menu in the titlebar.
    // Keep the only existing entry on hosts without a mounted Tools menu.
    if !super::shell_menu::in_window_menu() && state.desktop.supports_native_transfer() {
        items = items.child(rail_item(state, Section::Tools, expanded, dark));
    }
    let (toggle_icon, toggle_label) = if expanded {
        (&ICON_PANEL_LEFT_CLOSE, t("connWin.collapseSidebar"))
    } else {
        (&ICON_PANEL_LEFT_OPEN, t("connWin.expandSidebar"))
    };
    let toggle = row()
        .key("rail-toggle")
        .w_full()
        .h(RAIL_ITEM_HEIGHT)
        .center_y()
        .radius(0.0)
        .hover(colors.surface_raised)
        .hover_group()
        .role("button")
        .label(toggle_label)
        .help(toggle_label)
        .on_press(AppState::toggle_sidebar_expanded);
    let glyph = rune::icon(toggle_icon, 16.0)
        .color(colors.fg_secondary)
        .group_color(colors.fg)
        .a11y_hidden();
    let toggle = if expanded {
        toggle
            .px(12.0)
            .gap(10.0)
            .child(glyph)
            .child(
                text(toggle_label)
                    .key("rail-toggle-label")
                    .font(TextStyle::new(12.0, 400.0).lh(16.0))
                    .color(colors.fg_secondary)
                    .group_color(colors.fg)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    } else {
        toggle.justify_center().child(glyph)
    };
    let bottom = col()
        .key("rail-bottom")
        .w_full()
        .child(rail_item(state, Section::Settings, expanded, dark))
        .child(toggle);
    col()
        .key("datazen-rail")
        .w(width)
        .h_full()
        .fill(colors.surface_alt)
        .border_sides([0.0, 1.0, 0.0, 0.0], colors.edge)
        .border_layout()
        .child(items)
        .child(col().w_full().grow(1.0))
        .child(bottom)
}
pub fn content_toolbar(state: &AppState, theme: tok::Theme) -> El {
    // The two lookups stay here because the T0-03 i18n sweep pins this key's
    // call count in views/chrome.rs; the controlled builder consumes them.
    let new_query_label = t("common.newQuery").to_owned();
    let new_query_aria = t("common.newQuery").to_owned();
    let show_new_query = state.active == Section::Workspace;
    let vm = ContentToolbarVm {
        show_new_query,
        // T009-S7's workspace AI toggle: React's `ContentToolbar` shows it on
        // a relational panel (there is no KV relay in this build), so it lives
        // in the same workspace-only action cluster as New Query. The open
        // flag is part of the VM so the button paints its active style while
        // the aside is mounted. The T173 port left both fields at their
        // defaults, which silently dropped the toggle the AI panel tests pin.
        show_ai: show_new_query,
        ai_chat_open: state.ai_chat_open,
        new_query_label,
        new_query_aria,
        title: state.active.label().to_owned(),
        ..Default::default()
    };
    let mut toolbar = ContentToolbar::<AppState>::new(&vm);
    if show_new_query {
        toolbar = toolbar.on_new_query(|state: &mut AppState| {
            state.open_query_tab("Query");
        });
        toolbar = toolbar.on_ai_chat(|state: &mut AppState| state.toggle_ai_chat());
    }
    toolbar.build(theme)
}

/// The session object tree: databases, schemas and tables from the port's
/// catalog, with the tree's collapsed nodes kept in UI state.
/// L5: workspace panel tab strip (48pt). A tablist with roving keyboard
/// focus (Arrow/Home/End via focus_group), active underline, and a close
/// control per tab. Keys `tab-strip` / `tab-{id}` / `tab-close-{id}` are
/// kept for existing interactions and tests.
pub fn tab_strip(state: &AppState, theme: tok::Theme) -> El {
    let mut strip = row()
        .key("tab-strip")
        .role("tablist")
        .focus_group()
        .w_full()
        .h(48.0)
        .center_y()
        .fill(theme.bg);
    for tab in state.tabs.tabs() {
        let id = tab.id;
        let selected = state.tabs.selected().is_some_and(|open| open.id == id);
        let underline = if selected {
            crate::ui::theme::accent(true)
        } else {
            Color::TRANSPARENT
        };
        let tab_item = row()
            .key(format!("tab-{id}"))
            .role("tab")
            .label(tab.title.clone())
            .focusable(true)
            .h(48.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .fill(if selected {
                theme.control_off
            } else {
                theme.bg
            })
            .on_press(move |state: &mut AppState| {
                state.select_tab(id);
            })
            .child(text(tab.title.clone()).font(type_::LABEL).color(theme.text))
            .child(
                row()
                    .key(format!("tab-close-{id}"))
                    .w(20.0)
                    .h(20.0)
                    .center()
                    .radius(4.0)
                    .label(format!("Close {}", tab.title))
                    .focusable(true)
                    .child(text("×").font(type_::LABEL).color(theme.text_secondary))
                    .on_press(move |state: &mut AppState| {
                        state.close_tab(id);
                    }),
            )
            .child(
                col()
                    .key(format!("tab-underline-{id}"))
                    .absolute(0.0, 46.0)
                    .w_full()
                    .h(2.0)
                    .fill(underline),
            );
        strip = strip.child(tab_item);
    }
    strip
}

/// The titlebar's theme toggle: React `ThemeToggle` in the chrome slot's
/// right side. The old binary DARK/LIGHT text button had no System option and
/// no icon; this is the 1:1 port — a square icon trigger with a
/// light/dark/system popover mounted in the page's overlay layer.
pub fn theme_toggle_slot(state: &AppState, dark: bool, layer: &mut Vec<El>) -> El {
    let k = crate::ui::kit::kit(dark);
    row()
        .key("datazen-theme-toggle")
        .w(32.0)
        .h(32.0)
        .center_y()
        .child(
            ThemeToggle::<AppState>::new(state.theme_mode(), state.theme_toggle_open)
                .resolver(theme_icon_resolver())
                .on_toggle(|state: &mut AppState| {
                    state.titlebar_menu.open_menu = None;
                    state.theme_toggle_open = !state.theme_toggle_open;
                })
                .on_select(|state: &mut AppState, mode| {
                    state.theme_toggle_open = false;
                    state.save_theme_mode(mode);
                })
                .on_close(|state: &mut AppState| {
                    state.theme_toggle_open = false;
                })
                .build(k, layer),
        )
}

/// L5: 48pt content toolbar per section (action row). Actions are those
/// already wired in the Rune UI for the active section.
/// L6: 40pt content status bar — replaces the migration preview banner.
/// Shows connection state, context counts (table/columns/rows) when available,
/// and shortcut hints.
pub fn content_status_bar(state: &AppState, dark: bool) -> El {
    content_status_bar_with_live_table_paint(state, dark, false)
}

pub(super) fn content_status_bar_with_live_table_paint(
    state: &AppState, dark: bool, live_table_data: bool,
) -> El {
    // Kept in this file because the T0-03 sweep pins the call count here.
    let connected_label = t("conn.connected").to_owned();
    let profile = state.desktop_connection.as_ref().and_then(|id| {
        state
            .desktop_connections
            .as_ref()
            .and_then(|list| list.iter().find(|c| &c.id == id))
    });
    let connection_name = profile.map(|c| c.name.clone());
    let database_label = profile.map(|c| crate::views::backup::db_label(&c.engine));
    // The first catalog item need not be the database the user opened.
    let current_database = state.query_database_selection()
        .map(|(_, context)| context.current.database)
        .or_else(|| state.desktop.native_value_search_scope()
            .filter(|scope| state.desktop_connection.as_deref() == Some(&scope.connection_id[..]))
            .map(|scope| scope.database));

    let mut table_name: Option<String> = None;
    let mut column_count = 0u64;
    use crate::views::content_status_bar::CountStrategy;
    use datazen_ui_api::NativeTableRowCount;
    let mut total_rows = 0u64;
    let mut total_rows_kind = CountStrategy::Exact;
    if let Some(tab) = state.tabs.selected() {
        if tab.kind == datazen_ui_api::TabKind::Table {
            table_name = Some(tab.title.clone());
            if state.grid.has_page() {
                column_count = state.grid.cols as u64;
                (total_rows, total_rows_kind) = match state.grid.row_count() {
                    NativeTableRowCount::Exact(total) => (total, CountStrategy::Exact),
                    NativeTableRowCount::Estimated(total) => (total, CountStrategy::Estimated),
                    NativeTableRowCount::Unknown => (0, CountStrategy::Unsupported),
                };
            }
        } else if tab.kind == datazen_ui_api::TabKind::Query {
            if let Some(result) = state.result_grid.as_ref() {
                if result.has_page() {
                    column_count = result.cols as u64;
                    total_rows = u64::from(result.total_rows());
                }
            }
        }
    }

    let vm = crate::views::content_status_bar::StatusBarVm {
        connected: state.session_open(),
        database_label,
        connection_name,
        current_database,
        table_name,
        column_count,
        total_rows,
        total_rows_kind,
        has_kv_slot: false,
        mod_key: if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" }.to_owned(),
        key_sep: if cfg!(target_os = "macos") { "" } else { "+" }.to_owned(),
        connected_label,
        shortcuts_override: None,
    };
    crate::views::content_status_bar::build_with_live_table_paint(&vm, dark, live_table_data)
}
