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
//! * The host's `Section` enum combines React's workspace modes and Settings.
//!   Settings still occupies the entire body, with no mode rail or
//!   database chrome. Entering it records the previous Section, so Back
//!   restores that mode and its existing state.
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
pub fn connection_page(state: &AppState, cx: &mut Cx<AppState>) -> El {
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

    let body = row()
        .key("datazen-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0);

    // Keep the one window-level overlay layer for either body.
    let mut layer = modal_layer(state, theme, dark);
    let body = if state.active == Section::Settings {
        body.child(settings_page(state, theme, dark).h_full())
    } else {
        // React mounts ConnectionNavigatorTree in its database workspace only;
        // Rune splits that workspace between the landing and the live tabs.
        let mut body = body.child(rail_view(state, dark));
        if matches!(state.active, Section::Connections | Section::Workspace) {
            body = body
                .child(navigator_panel(state, theme, dark))
                .child(splitter::splitter_handle(
                    state,
                    NAVIGATOR_WIDTH_KEY,
                    splitter::SplitAxis::Vertical,
                    NAVIGATOR_WIDTH_MIN,
                    NAVIGATOR_WIDTH_MAX,
                    NAVIGATOR_WIDTH_DEFAULT,
                ));
        }
        // These React modes mount their page directly, outside the database
        // ContentView. Each existing Rune page owns its own controls and
        // any padding, with no generic database title or status footer.
        let embedded = match state.active {
            Section::Workflows | Section::Dashboards | Section::Wapps => {
                Some(page_body(state, theme, dark, &mut layer))
            }
            _ => None,
        };
        if let Some(page) = embedded {
            body.child(
                col()
                    .key("datazen-page")
                    .grow(1.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .h_full()
                    .child(page.h_full()),
            )
        } else {
            body.child(connection_content(state, theme, dark, &mut layer))
        }
    };

    col()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .on_paste(|state: &mut AppState, text, _| {
            // Same routing as the React page's focused editor/grid: an OS paste
            // lands here when no platform text field owns the focus.
            if state.active == Section::Settings {
                return;
            }
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
    connection_content_with_vm(state, theme, dark, &vm, layer)
}

/// The same content column with an explicit view model, so a host that owns an
/// object selection can compose the real `DatabaseObjectView` into the slot.
#[allow(dead_code)] // composition entry point; remove when the host owns object selection.
pub fn connection_content_with_vm(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    vm: &crate::views::content_view::ContentViewVm,
    layer: &mut Vec<El>,
) -> El {
    let mut slot = row().key("db-object-host").w_full().min_w(0.0);
    if let Some(object) = vm.object_vm.as_ref() {
        let mut layer = Vec::new();
        crate::views::database_object_view::build::<AppState>(
            &object.identity,
            &object.state,
            None,
            dark,
            &mut layer,
        );
        slot = slot.grow(1.0).min_h(0.0).children(layer);
    } else {
        slot = slot.size(0.0, 0.0);
    }
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
        .child(crate::views::content_view::build(state, theme, dark, vm, layer))
        // seg1's object/document view slots. Their local `Vec<El>`s are output
        // containers for the views' root elements (each builder pushes one
        // `root`), not modal layers — no scrim is mounted from them. The one
        // real overlay remains `connection_page`'s `modal_layer`, mounted once
        // at the root.
        .child(slot)
        // T337: the document browser composes here when a host supplies its VM.
        .child({
            let mut doc_slot = row().key("doc-view-host").w_full().min_w(0.0);
            if let Some(document) = vm.document_vm.as_ref() {
                let mut doc_layer = Vec::new();
                crate::views::document_connection_view::build::<AppState>(
                    document,
                    None,
                    dark,
                    &mut doc_layer,
                );
                doc_slot = doc_slot.grow(1.0).min_h(0.0).children(doc_layer);
            } else {
                doc_slot = doc_slot.size(0.0, 0.0);
            }
            doc_slot
        })
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
                .role("button")
                .label(t("common.back"))
                .focusable(true)
                .on_press(AppState::leave_settings)
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
