//! Live Windows/Linux titlebar menu. The React chrome uses the system menu
//! instead on macOS. Unported host actions are omitted rather than shown as
//! functioning controls.

use rune::{row, text, ChromeInsets, Color, El, Key, KeyEvent, TextStyle, Tx};

use crate::components::menu_bar::{
    build_menus, next_menu_id, route_menu_action, MenuBar, MenuBarAction, MenuBarItem,
    MenuBarMenu, MenuBarTheme,
};
use crate::native_transfer_state::NativeTool;
use crate::state_def::AppState;
use crate::types::Section;
use crate::views::appearance_section::ThemeMode;

const REPORT_ISSUE: &str = "https://github.com/flyxl/datazen/issues/new";

/// Match the original chrome's padding while respecting the real platform
/// buttons. In Linux, captions are in the OS frame rather than our client
/// area; do not reserve the React web-caption width or draw duplicate buttons.
pub(super) fn titlebar_padding(insets: ChromeInsets) -> (f32, f32) {
    let right = if insets.trailing > 0.0 {
        insets.trailing + 2.0
    } else if cfg!(target_os = "macos") {
        14.0
    } else {
        12.0
    };
    (insets.leading + 12.0, right)
}

/// The React Windows/Linux chrome reserves 140pt on the right for its
/// absolutely positioned web captions. Keep the real native controls where
/// the platform put them and give only the title's strip the missing reserve.
pub(super) fn titlebar_reference_reserve(right_pad: f32) -> El {
    let missing = if cfg!(any(target_os = "windows", target_os = "linux")) {
        (140.0 - right_pad).max(0.0)
    } else {
        // macOS already takes its leading traffic-light inset from the host
        // (zero in fullscreen) and uses the original 14pt trailing padding.
        0.0
    };
    row().key("titlebar-reference-reserve").w(missing).min_w(0.0).shrink()
}

/// The remaining strip centers the title between the actual left and right
/// controls. Keep the label independently addressable for shell tooling.
pub(super) fn titlebar_label(label: impl Into<String>, ink: Color) -> El {
    row()
        .key("datazen-titlebar-title")
        .grow(1.0)
        .min_w(0.0)
        .center()
        .child(
            text(label.into())
                .key("datazen-titlebar-label")
                .font(TextStyle::new(12.0, 500.0).lh(18.0))
                .min_w(0.0)
                .shrink()
                .truncate()
                .color(ink),
        )
}

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
        "backup" => state.supports_tools_backup(),
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
        MenuBarAction::OpenBackup { restore: false } => state.open_native_backup_tool(),
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
        state.show_native_transfer_tool(tool);
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
