//! Live Windows/Linux titlebar menu. The React chrome uses the system menu
//! instead on macOS. Unported host actions are omitted rather than shown as
//! functioning controls.

use rune::{row, El, Key, KeyEvent, Tx};

use crate::components::menu_bar::{
    build_menus, next_menu_id, route_menu_action, MenuBar, MenuBarAction, MenuBarItem,
    MenuBarMenu, MenuBarTheme,
};
use crate::native_transfer_state::NativeTool;
use crate::state_def::AppState;
use crate::types::Section;
use crate::views::appearance_section::ThemeMode;

const REPORT_ISSUE: &str = "https://github.com/flyxl/datazen/issues/new";

fn in_window_menu() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

fn theme(state: &AppState) -> MenuBarTheme {
    match state.theme_mode() {
        ThemeMode::Light => MenuBarTheme::Light,
        ThemeMode::Dark => MenuBarTheme::Dark,
        ThemeMode::System => MenuBarTheme::System,
    }
}

fn menus(state: &AppState) -> Vec<MenuBarMenu> {
    let native_tools = state.desktop.supports_native_transfer();
    let keep = |id: &str| match id {
        "new-connection" | "open-settings" | "theme-light" | "theme-dark" | "theme-system"
        | "workflow" | "dashboard" | "help-report" => true,
        "schema-diff" | "data-sync" | "data-transfer" => native_tools,
        _ => false,
    };
    let mut menus = build_menus(theme(state));
    for menu in &mut menus {
        // Keep the original group boundaries without orphaned or doubled
        // separators when the intervening unsupported actions are hidden.
        let mut items = Vec::new();
        let mut separator: Option<MenuBarItem> = None;
        for item in std::mem::take(&mut menu.items) {
            if item.separator && !items.is_empty() {
                separator = Some(item);
            } else if keep(&item.id) {
                items.extend(separator.take());
                items.push(item);
            }
        }
        menu.items = items;
    }
    menus
}

fn open(state: &mut AppState, id: &str) {
    state.theme_toggle_open = false;
    state.titlebar_menu.open_menu = Some(id.to_owned());
}

pub(super) fn titlebar_menu(state: &AppState, dark: bool, layer: &mut Vec<El>) -> El {
    let host = row().key("menu-bar-host").h(28.0).center_y().min_w(0.0);
    if !in_window_menu() {
        return host;
    }
    let entries = menus(state);
    host.child(
        MenuBar::new(&entries, state.titlebar_menu.open_menu.as_deref())
            .on_open(open)
            .on_hover(open)
            .on_close(|state: &mut AppState| state.titlebar_menu.open_menu = None)
            .on_select_action(dispatch)
            .on_navigate(|state: &mut AppState, id, forward| {
                if let Some(next) = next_menu_id(&menus(state), id, forward) {
                    open(state, &next);
                }
            })
            .build(state, dark, layer),
    )
}

fn dispatch(state: &mut AppState, id: &str, tx: &mut Tx) {
    match route_menu_action(id) {
        MenuBarAction::SetTheme(mode) => state.save_theme_mode(match mode {
            MenuBarTheme::Light => ThemeMode::Light,
            MenuBarTheme::Dark => ThemeMode::Dark,
            MenuBarTheme::System => ThemeMode::System,
        }),
        MenuBarAction::OpenSchemaDiff => native_tool(state, NativeTool::SchemaDiff),
        MenuBarAction::OpenDataSync => native_tool(state, NativeTool::SyncPreview),
        MenuBarAction::OpenDataTransfer => native_tool(state, NativeTool::Transfer),
        MenuBarAction::OpenReportIssue => {
            // Fixed public destination, handled by Rune's URL policy.
            let _ = tx.open_url(REPORT_ISSUE);
        }
        MenuBarAction::EmitMenuAction(id) => match id.as_str() {
            "new-connection" if !state.connection_dialog_open => state.open_connection_dialog(),
            "open-settings" => state.open_settings(None),
            "workflow" => state.select(Section::Workflows),
            "dashboard" => state.select(Section::Dashboards),
            _ => {}
        },
        _ => {}
    }
}

fn native_tool(state: &mut AppState, tool: NativeTool) {
    if !state.desktop.supports_native_transfer() {
        return;
    }
    state.select(Section::Tools);
    if state.active == Section::Tools {
        state.set_native_tool(tool);
    }
}

/// Same actions as the visible File items, via Rune's normalized Ctrl chords.
/// Do not reset an already open connection draft.
pub(crate) fn shortcut(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !in_window_menu()
        || state.connection_dialog_open
        || !ev.mods.cmd
        || ev.mods.alt
        || ev.mods.shift
        || ev.mods.logo
    {
        return false;
    }
    let id = match ev.key {
        Key::Command('n') => "new-connection",
        Key::Command(',') => "open-settings",
        _ => return false,
    };
    state.titlebar_menu.open_menu = None;
    state.theme_toggle_open = false;
    dispatch(state, id, tx);
    true
}
