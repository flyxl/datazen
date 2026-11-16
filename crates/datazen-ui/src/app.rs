//! Root view and application bootstrap.

use crate::constants::*;
use crate::state_def::AppState;
use crate::types::Section;
use crate::views::{
    content_status_bar, content_toolbar, modal_layer, navigator_panel, page_body, query_selected,
    rail_view, tab_strip,
};
use crate::{splitter, sql_editor};

use datazen_ui_api::UnimplementedAdapter;
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use datazen_ui_api::{DesktopAdapter, Setting};
use rune::tok::{self, type_};
use rune::{col, row, text, App, AppSpec, Cx, El, Ime, Key, KeyEvent, Titlebar, Tx};

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state
        .ui_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or_else(|| cx.system_dark());
    let theme = tok::theme(dark);

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

    // L3: self-drawn titlebar (40pt). The whole band moves the window; the
    // leading inset keeps clear of the platform's window controls (macOS
    // traffic lights). The theme toggle is a hit target inside the drag band
    // and keeps its press.
    let titlebar = row()
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

    let body = row()
        .key("datazen-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(rail)
        .child(navigator)
        .child(nav_splitter)
        .child(page);

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

/// The app's window chrome (L3): self-drawn 40pt titlebar and a minimum size
/// aligned with the React/Tauri main window.
const TITLEBAR_HEIGHT: f32 = 40.0;
const WINDOW_MIN_SIZE: (f32, f32) = (960.0, 640.0);

/// Launch the same Rune view on native desktop and WebAssembly.
pub fn run() {
    App::new(AppSpec {
        on_key: Some(app_key),
        poll: Some(app_poll),
        fonts: APP_FONTS,
        families: APP_FONT_FAMILIES,
        min_size: Some(WINDOW_MIN_SIZE),
        ..AppSpec::new("DataZen", (1440.0, 900.0), init_state, view)
    })
    .titlebar(Titlebar::new(TITLEBAR_HEIGHT))
    .on_ime(app_ime)
    .run();
}

/// Launch the native process with the host's own adapter (N2): the live
/// host's entry. `run()` stays the demo process — the in-memory world and
/// its seeded connections; here the adapter the host built owns the real
/// use cases, and the first state consults it before the first frame, as
/// `init_state` does for the demo.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_with_adapter(adapter: Box<dyn DesktopAdapter>) {
    app_with_adapter(adapter).run();
}

/// The same app `run_with_adapter` runs, before it runs (T9): the host
/// installs its own process capabilities — a system tray, say — between the
/// construction and `run`, over the handle the app hands out.
pub fn app_with_adapter(adapter: Box<dyn DesktopAdapter>) -> App<AppState> {
    App::new(AppSpec {
        on_key: Some(app_key),
        poll: Some(app_poll),
        fonts: APP_FONTS,
        families: APP_FONT_FAMILIES,
        min_size: Some(WINDOW_MIN_SIZE),
        ..AppSpec::new("DataZen", (1440.0, 900.0), init_state, view)
    })
    .titlebar(Titlebar::new(TITLEBAR_HEIGHT))
    // The spec's `init` is a plain fn pointer; `App::init` takes the closure
    // that captured the host's adapter instead.
    .init(move || {
        let mut state = AppState::default();
        state.desktop = adapter;
        state.sync_desktop_connections();
        state
    })
    .on_ime(app_ime)
}

/// The desktop process starts on the connections page, so the adapter is
/// consulted before the first frame: its answer (or its fixed error) is on
/// the page from the start. H3: the native process lists connections through
/// the connection use case, over the adapter's in-memory store; the wasm
/// host starts on the unwired adapter and the port answers.
pub(crate) fn init_state() -> AppState {
    let mut state = AppState::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        state.desktop = Box::new(UseCaseAdapter::with_demo_connections());
    }
    state.sync_desktop_connections();
    state
}

pub(crate) fn app_key(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if state.active != Section::Workspace {
        return state.active == Section::Workflows
            && state.workflow_editing.is_some()
            && sql_editor::on_key(&mut state.workflow_yaml, ev, tx);
    }
    if query_selected(state) {
        if matches!(ev.key, Key::Enter) && ev.mods.cmd {
            state.run_query();
            return true;
        }
        // T1: after a result cell is pressed the result grid holds the
        // keyboard: arrows walk its selection, Command-C copies TSV. Typing
        // or Escape hands the keys back to the editor, and so does any key
        // the grid does not answer.
        if state.result_focused {
            if matches!(ev.key, Key::Text(_) | Key::Escape) {
                state.result_focused = false;
            } else if state
                .result_grid
                .as_mut()
                .is_some_and(|grid| crate::data_grid::on_key(grid, ev, tx))
            {
                return true;
            } else {
                state.result_focused = false;
            }
        }
        return sql_editor::on_key(&mut state.sql, ev, tx);
    }
    if state.grid.has_page() {
        return crate::data_grid::on_key(&mut state.grid, ev, tx);
    }
    false
}

pub(crate) fn app_poll(state: &mut AppState, _: f64, _: &mut Tx) -> bool {
    // N1: the host's finished jobs land first, whatever section is showing;
    // a delivery redraws (it wrote state or cleared its marker), and stale
    // generations were dropped inside `deliver_host_job`.
    let mut dirty = false;
    for delivery in state.desktop.poll_jobs() {
        state.deliver_host_job(delivery);
        dirty = true;
    }
    if state.active != Section::Workspace {
        return dirty;
    }
    if query_selected(state) {
        return dirty
            || state
                .result_grid
                .as_mut()
                .is_some_and(|grid| grid.sync_scroll());
    }
    dirty || (state.grid.has_page() && state.grid.sync_scroll())
}

pub(crate) fn app_ime(state: &mut AppState, ev: &Ime, _: &mut Tx) -> bool {
    if state.active == Section::Workflows && state.workflow_editing.is_some() {
        return crate::sql_editor::on_ime(&mut state.workflow_yaml, ev);
    }
    state.active == Section::Workspace
        && query_selected(state)
        && crate::sql_editor::on_ime(&mut state.sql, ev)
}

/// WebAssembly entrypoint consumed by wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    run();
}
