//! The React `windows/connection/ConnectionPage.tsx` port: the connection
//! window's root container.
//!
//! React's `ConnectionPage` is the whole desktop window: a `WindowChromeSlot`
//! title bar (menu bar + optional back + theme toggle), a workspace-mode
//! sidebar, a resizable navigator + content column (toolbar, tab strip, page
//! body, status bar) and a set of dialog hosts. Rune's root view is exactly
//! that window, so this module is the 1:1 home for the glue that belongs to
//! `ConnectionPage` itself; every visible piece is *composed* from the existing
//! views rather than re-implemented:
//!
//! * `views::rail_view` — React's `WorkspaceModeSidebar`.
//! * `views::navigator_panel` + `splitter::splitter_handle` — the navigator
//!   aside and its resize handle (React `useResizable`).
//! * `views::content_toolbar` / `tab_strip` / `page_body` /
//!   `content_status_bar` — the content workspace (React `ContentView` plus
//!   its toolbar / tabs / status bar).
//! * `views::modal_layer` — the confirm / result dialog hosts.
//!
//! Adaptations forced by the host contract (recorded as leftovers, not silent
//! drops):
//!
//! * React's `MenuBar` and `WindowChromeSlot` have no Rune counterpart. The
//!   title bar here is Rune's self-drawn 40pt chrome band; an empty
//!   `menu-bar-host` slot is kept for structural parity, and the theme toggle
//!   is the existing `datazen-theme-toggle`. Removal stage: when a Rune menu-bar
//!   base lands, replace the empty host with it (no key changes needed).
//! * React's `mainView` (`workspace` | `settings`) and `workspaceMode`
//!   (`connections` | `workflow` | `dashboard` | `workspace` | `extension`) are
//!   flattened into the single `Section` enum (`crate::types::Section`). Every
//!   React mode maps to a Section, so all of them stay reachable from the rail;
//!   the settings "back" button returns to `Section::Connections` (React
//!   restores the previous workspace mode, which Rune's flat Section state
//!   does not keep). Removal stage: when a workspace-mode slice exists, restore
//!   the previous mode instead of hard-coding Connections.
//! * React's per-tab connection state machine (`connecting` / `connected` /
//!   `error`), panel store and cross-window bus are not modelled in Rune's
//!   `AppState`; the connecting spinner and the error/retry overlay are
//!   therefore not drawn. The session state the host does expose is
//!   `session_open()`, which the navigator and status bar already consume.
//!   Removal stage: when tab connection status and the panel store are ported,
//!   render those two overlays in `connection_content` with React's
//!   `common.retry` / `common.close` actions.
//!
//! No new colours: every surface and ink comes from `rune::tok::theme`. Icons
//! come from the registry (`crate::icons`).

use super::*;

use crate::constants::*;
use crate::i18n::t;
use crate::state_def::AppState;
use crate::types::Section;
use crate::splitter;
use rune::tok::{self, type_};
use rune::{col, row, text, Cx, El};

/// Title band height, matching the native `Titlebar` chrome and the old
/// `app::TITLEBAR_HEIGHT`. The port owns its own geometry now.
const TITLEBAR_HEIGHT: f32 = 40.0;

/// The connection page: Rune's root view. See the module docs for the compose
/// map and the honest host-contract adaptations.
pub(crate) fn connection_page(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let system_dark = cx.system_dark();
    state.system_dark.set(system_dark);
    let dark = match state.desktop.read_ui_settings() {
        Ok(settings) => settings.into_iter().find_map(|setting| match setting {
            datazen_ui_api::Setting::Theme { dark } => Some(dark),
            _ => None,
        }),
        Err(_) => None,
    }
    .unwrap_or(system_dark);
    let theme = tok::theme(dark);
    let titlebar_inset = cx.titlebar_inset();

    let titlebar = connection_titlebar(state, theme, dark, titlebar_inset);

    // WorkspaceModeSidebar (React mode rail) + the navigator aside with its
    // resize handle, then the content workspace.
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

    // Single window-level overlay layer: dialogs and select listboxes mount
    // here (page_body takes the same vec). Appended once at the root below.
    let mut layer = modal_layer(state, theme, dark);

    let body = row()
        .key("datazen-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(rail)
        .child(navigator)
        .child(nav_splitter)
        .child(connection_content(state, theme, dark, &mut layer));

    col()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .on_paste(|state: &mut AppState, text, _| {
            // Same routing as the React page's focused editor/grid: an OS paste
            // lands here when no platform text field owns the focus.
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
        .child(
            col()
                .key("rune-connection-page")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0)
                .child(titlebar)
                .child(body),
        )
        .children(layer)
}

/// The content workspace: toolbar, tab strip (Workspace only), page body and
/// status bar. Composed from the existing chrome/view functions so the keys
/// existing tests pin (`content-toolbar`, `tab-strip`, `content-status-bar` and
/// `page_body`'s per-section keys) do not move.
fn connection_content(state: &AppState, theme: tok::Theme, dark: bool, layer: &mut Vec<El>) -> El {
    // T199 (ContentView port): the componentized view owns the toolbar / tab
    // strip / body / status-bar composition and the VM's conditional gates.
    let vm = crate::views::content_view::ContentViewVm::from_host(state);
    col()
        .key("datazen-page")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .px(24.0)
        .py(24.0)
        .gap(16.0)
        .fill(theme.bg)
        // Thread the single overlay layer through the ContentView port down to
        // `page_body`, so workspace select listboxes mount into the same layer
        // `connection_page` mounts once at the root (no second scrim).
        .child(crate::views::content_view::build(state, theme, dark, &vm, layer))
}

/// The title band: React's `titleBarLeft` (back + menu bar), the centred title,
/// and the theme toggle. All chrome keys existing tests pin are kept verbatim.
fn connection_titlebar(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    titlebar_inset: f32,
) -> El {
    let mut left = row()
        .key("rune-titlebar-left")
        .gap(4.0)
        .center_y();

    // React shows the back arrow only on the settings view. Rune's settings is
    // a Section, so the same control appears when that Section is active.
    if state.active == Section::Settings {
        left = left.child(
            row()
                .key("settings-back")
                .h(28.0)
                .w(28.0)
                .center()
                .radius(6.0)
                .hover(theme.hover)
                .label(t("common.back"))
                .on_press(|state: &mut AppState| state.select(Section::Connections))
                .child(
                    rune::icon(&crate::icons::ICON_CHEVRON_LEFT, 16.0)
                        .color(theme.text_secondary),
                ),
        );
    }
    // Structural slot for React's MenuBar; empty until a Rune menu base lands.
    left = left.child(row().key("menu-bar-host").h(28.0).min_w(0.0));

    row()
        .key("datazen-titlebar")
        .window_drag()
        .w_full()
        .h(TITLEBAR_HEIGHT)
        .center_y()
        .pl(titlebar_inset + 12.0)
        .pr(12.0)
        .gap(12.0)
        .fill(theme.bg)
        .child(left)
        .child(
            text(state.active.label())
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
