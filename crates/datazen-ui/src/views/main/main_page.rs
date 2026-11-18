//! Port of `packages/frontend/src/windows/main/MainPage.tsx` (T020).
//!
//! The React main window's composition root: the window chrome slot, the
//! connection page, the two connection dialog hosts and the connection
//! store's first load. The window is created by the Rust host **after** the
//! onboarding wizard has finished (or immediately on an upgrade/normal
//! launch) and never renders the wizard — that lives in a separate window.
//! The zero-connection empty state is owned by the connections home
//! (`views/connections_home.rs`, React `ConnectionWorkspaceHome` state 1), so
//! a user who skips the wizard or deletes every connection lands straight in
//! the workspace with create/import actions.
//!
//! ## Branches (`MainPage.tsx:110-142`)
//!
//! | React | Rune |
//! |---|---|
//! | `if (!connectionsLoaded) return null` | degenerate: the bootstrap consults the adapter before the first frame (`app.rs`'s `init_state` / `app_with_adapter` both run `sync_desktop_connections` at init), so no frame exists with the first load still in flight. The never-synced state keeps the memory port's list, exactly as `AppState::default()` always has. |
//! | `connections.length === 0 && loadError` → the `main-load-error` screen with `main-load-retry` | [`main_load_failed`]: the store's list fetch failed with a real error ([`AppState::connections_load_error`], written by `sync_desktop_connections`) and the store's list is empty. The store keeps its previous list when a fetch fails — React's zustand `connectionStore` writes only the error on failure — so an empty list means the list never answered. |
//! | `<ConnectionPage />` + the two dialog hosts + `RunningExecutionCloseHost` | [`main_shell_body`]: the titlebar, the rail + navigator + page row, and the root modal layer (the two dialog hosts). |
//!
//! The failure branch keeps the same chrome slot and the same two dialog
//! hosts as React (`MainPage.tsx:115-131`): the titlebar names the app
//! (`t('menu.appName')`) instead of the section, and the dialog layer still
//! mounts over the failure screen.
//!
//! ## Honest seams (dependencies this port does not include; each belongs to
//! its own later task in the migration ledger)
//!
//! - `MenuBar` — the chrome slot's left content (`components/MenuBar.tsx` is
//!   T197). The titlebar keeps the window-title label in the lead position
//!   until the menu bar lands.
//! - `ThemeToggle` — the chrome slot's right content
//!   (`components/ThemeToggle.tsx` is T284). The binary DARK/LIGHT toggle the
//!   shell already carries stands in.
//! - `RunningExecutionCloseHost` — `windows/main/RunningExecutionCloseHost.tsx`
//!   is T071.
//! - the `listenCrossWindow` effect family (`datazen:connections-changed`
//!   refresh, `menu:new-connection`, `menu:export-connections` and the
//!   import-connections sources) — `lib/crossWindowBus.ts` is T437; the share
//!   dialog the menu events open lives in `lib/connectionShare.ts` (T412).
//!   Rune is a single window for now, so no second window can signal it.
//! - `startUpdateReminderSchedule()` — `lib/updateReminderSchedule.ts` is
//!   T639.
//! - `hideSplash(...)` — `lib/splash.ts` is T603; the Rune process owns no
//!   splash layer to hide.
//! - `ConnectionPage` itself is the partially-ported body (`windows/connection/
//!   ConnectionPage.tsx` is T110, split) and the two dialog hosts are the
//!   existing root modal layer (`views/modals.rs`; the React
//!   `NewConnectionDialog` is T239 and `ConnectionShareDialogHost` is T167).
//!
//! ## Keys
//!
//! | part | key |
//!|---|---|
//! | shell root | `datazen-root` |
//! | titlebar band / label / toggle | `datazen-titlebar`, `datazen-titlebar-label`, `datazen-theme-toggle` |
//! | shell body row / page column | `datazen-body`, `datazen-page` |
//! | failure screen / sentence / retry | `main-load-error`, `main-load-error-message`, `main-load-retry` |

use crate::constants::*;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::types::Section;
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::views::{
    content_status_bar, content_toolbar, modal_layer, navigator_panel, page_body, query_selected,
    rail_view, tab_strip,
};
use crate::splitter;
use datazen_ui_api::Setting;
use rune::tok::{self, type_};
use rune::{col, row, text, Align, Cx, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

/// The main window's root view: the React `MainPage` composition, branch by
/// branch, over the chrome and dialog layer the shell already owns.
pub(crate) fn main_page(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let system_dark = cx.system_dark();
    state.system_dark.set(system_dark);
    let dark = match state.desktop.read_ui_settings() {
        Ok(settings) => settings.into_iter().find_map(|setting| match setting {
            Setting::Theme { dark } => Some(dark),
            _ => None,
        }),
        Err(_) => None,
    }
    .unwrap_or(system_dark);
    let theme = tok::theme(dark);

    let load_failed = main_load_failed(state);
    // React's two chrome titles: the failure branch names the app
    // (`t('menu.appName')`, `MainPage.tsx:116`); the healthy branch carries
    // the connection page's `titleBarTitle` — the section label.
    let title_label = if load_failed {
        t("menu.appName").to_owned()
    } else {
        state.active.label().to_owned()
    };
    let titlebar = main_titlebar(dark, cx, title_label);
    let body = if load_failed {
        main_load_error_body(state, dark)
    } else {
        main_shell_body(state, theme, dark)
    };

    col()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .on_paste(|state: &mut AppState, text, _| {
            // T1: OS paste lands here when no platform text field is focused
            // (the grid's inline editor and every plain input keep their own
            // paste path). The editable table grid takes TSV at the focus
            // cell; the focused editor takes the text.
            if state.active == Section::Workspace
                && !query_selected(state)
                && state.grid.has_page()
                && state.grid.is_editable()
            {
                state.grid.paste_tsv(text);
            } else if query_selected(state) && state.sql.focused() {
                state.sql.insert(text);
            }
        })
        .child(titlebar)
        .child(body)
        .children(modal_layer(state, theme, dark))
}

/// React's load-failure gate (`MainPage.tsx:112`): an empty store list plus
/// the list fetch's own error. The store's list is the desktop adapter's
/// answer ([`AppState::desktop_connections`]); a failed fetch keeps the
/// previous list in React, so this reads the list fetch's error
/// ([`AppState::connections_load_error`]), not the shared adapter-notice
/// slot an open-job failure also lands in — a failed open keeps the
/// connections page and its banner, exactly as before.
fn main_load_failed(state: &AppState) -> bool {
    state.connections_load_error.is_some()
        && state
            .desktop_connections
            .as_ref()
            .map_or(true, |list| list.is_empty())
}

/// The self-drawn 40pt titlebar (L3): the chrome slot's Rune equivalent. The
/// band moves the window; the leading inset keeps clear of the platform's
/// window controls (macOS traffic lights). The theme toggle is a hit target
/// inside the drag band and keeps its press. The label is the title the
/// calling branch chose (the section title, or the app name on the failure
/// screen).
fn main_titlebar(dark: bool, cx: &mut Cx<AppState>, label: String) -> El {
    let theme = tok::theme(dark);
    row()
        .key("datazen-titlebar")
        .window_drag()
        .w_full()
        .h(TITLEBAR_HEIGHT)
        .center_y()
        .pl(cx.titlebar_inset() + 12.0)
        .pr(12.0)
        .gap(12.0)
        .fill(theme.bg)
        .child(
            // The label takes the leftover room, not the whole band: a
            // `w_full` label measures the padding box and pushes the trailing
            // controls past the window's right edge.
            text(label)
                .key("datazen-titlebar-label")
                .font(type_::TITLE)
                .color(theme.text)
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            row()
                .key("datazen-theme-toggle")
                .h(28.0)
                .px(10.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .on_press(|state: &mut AppState| state.toggle_theme())
                .child(
                    text(if dark { "DARK" } else { "LIGHT" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
}

/// React's `main-load-error` (`MainPage.tsx:120-128`): the centered danger
/// sentence and its retry, over the same chrome and the same dialog hosts
/// (the root modal layer, which mounts nothing while both dialogs are
/// closed).
fn main_load_error_body(state: &AppState, dark: bool) -> El {
    let sentence = state.connections_load_error.clone().unwrap_or_default();
    col()
        .key("main-load-error")
        // React: `flex flex-1 flex-col items-center justify-center gap-4 px-6`.
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(16.0)
        .px(24.0)
        .child(
            // React: `select-text text-center text-sm text-danger` — one
            // wrapped, centered paragraph in the danger ink. The nearest
            // built-in type is BODY (13pt/400) for React's 14px/400.
            text(sentence)
                .key("main-load-error-message")
                .w_full()
                .wrap()
                .align(Align::Center)
                .font(type_::BODY)
                .color(palette::danger_text(dark)),
        )
        .child(
            // React's `Button` defaults to the primary variant (accent
            // surface + on-accent ink) at md size (`h-9` = 36pt) — the kit's
            // Contrast skin and ExtraLarge control, the mapping the ported
            // dialog footers already use.
            Button::new(t("common.retry"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                // React: `onClick={() => void fetchConnections()}` — the
                // store's list fetch is the adapter sync, which clears the
                // list-fetch error on success and re-reads the list.
                .on_press(|state: &mut AppState| state.sync_desktop_connections())
                .build(kit(dark))
                .key("main-load-retry"),
        )
}

/// The healthy branch: React's `<ConnectionPage />` — the rail, navigator and
/// page column the shell already composes, plus the root modal layer's two
/// dialog hosts (mounted by [`main_page`]).
fn main_shell_body(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let rail = rail_view(state, dark);
    let navigator = navigator_panel(state, theme, dark);
    let nav_splitter = splitter::splitter_handle(
        state,
        NAVIGATOR_WIDTH_KEY,
        splitter::SplitAxis::Vertical,
        NAVIGATOR_WIDTH_MIN,
        NAVIGATOR_WIDTH_MAX,
        NAVIGATOR_WIDTH_DEFAULT,
    );

    let page = col()
        .key("datazen-page")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .px(24.0)
        .py(24.0)
        .gap(16.0)
        .fill(theme.bg)
        .child(content_toolbar(state, theme))
        .child(if state.active == Section::Workspace {
            tab_strip(state, theme)
        } else {
            row().key("tab-strip-absent").h(0.0)
        })
        .child(page_body(state, theme, dark))
        .child(content_status_bar(state, theme));

    row()
        .key("datazen-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(rail)
        .child(navigator)
        .child(nav_splitter)
        .child(page)
}
