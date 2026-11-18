//! Root view and application bootstrap.

use crate::constants::{APP_FONTS, APP_FONT_FAMILIES};
use crate::state_def::AppState;
use crate::types::Section;
use crate::sql_editor;
use crate::views::{self, query_selected};

use datazen_ui_api::UnimplementedAdapter;
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use datazen_ui_api::DesktopAdapter;
use rune::{App, AppSpec, Cx, El, Ime, Key, KeyEvent, Titlebar, Tx};

/// The app's single view entry: the main window's composition root
/// (T020, React `MainPage.tsx`) — the chrome slot, the connection page, the
/// dialog hosts and the store's first load, branch by branch.
pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    views::main_page(state, cx)
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
