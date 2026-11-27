//! Root view and application bootstrap.
//!
//! Port of `@datazen/frontend/app` (`packages/frontend/src/app.ts`, T049).
//! That file is a one-line barrel — it only re-exports `startSharedApp` and
//! `StartSharedAppOptions` from `./bootstrap/startSharedApp` — and it is
//! separate from the light `.` entry on purpose: `startSharedApp` transitively
//! owns the whole shared app graph (App, stores, PRO host-module table,
//! global CSS), while the desktop platform adapters consume only the light
//! surface (ports, contracts, capability bits). Keeping the bootstrap out of
//! `.` means a light consumer's test graph never has to know the app graph
//! exists. The same split holds here: the launch surface below is re-exported
//! at the crate root and nothing else in the crate needs it to render.
//!
//! The barrel dissolves into this module (an X-category pure re-export shell
//! — there is no `bootstrap` module to forward to yet):
//!
//! * `startSharedApp` itself has **no** Rune equivalent yet. Its port is
//!   T047 (`bootstrap/startSharedApp.tsx`, segment-4, split): the settings
//!   preload, locale/icon/PRO init, E2E store bridge and React-root mounting
//!   it performs do not exist in Rune. Do not invent a `start_shared_app`
//!   here; when T047 lands, this doc points at it.
//! * `StartSharedAppOptions` likewise arrives with T047. Its host half
//!   already exists in another shape: `configureHost` (backend binding,
//!   platform port injection, chrome registration) is the host building a
//!   [`datazen_ui_api::DesktopAdapter`] and handing it to
//!   [`run_with_adapter`] / [`app_with_adapter`]; `onSettingsPreloaded`
//!   (the settings preload settling before first paint) is T047's bootstrap
//!   step, not a launch-surface option.
//! * The crate-root re-exports in `lib.rs` (`run`, `run_with_adapter`,
//!   `app_with_adapter`) are this barrel's Rune counterpart: the shared-app
//!   launch surface, kept apart from the light port/contract surface.

use crate::constants::{APP_FONTS, APP_FONT_FAMILIES};
use crate::sql_editor;
use crate::state_def::AppState;
use crate::types::Section;
use crate::views::query_selected;

#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::DemoAdapter;
use datazen_ui_api::DesktopAdapter;
use rune::{App, AppSpec, Cx, El, Ime, Key, KeyEvent, Titlebar, Tx};

/// The app's one view entry is the root router (React `App.tsx`: `App` →
/// `WindowContent` → `MainPage`). The router's reachable arm composes
/// `MainPage`, which mounts the T110 `ConnectionPage` port as its sub-page
/// together with the connection dialog hosts and the running-execution close
/// host. `ConnectionPage` is *not* the app root: React's `MainPage.tsx:137`
/// renders `<ConnectionPage />` beside those hosts, and owns the load-failure
/// branch the router has to be able to show.
pub fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    crate::i18n::with_language(state.builtin_language(), || {
        crate::views::app_root(state, cx)
    })
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
    .init(move || crate::bootstrap::start_shared_app_state(Some(adapter)))
    .on_ime(app_ime)
}

/// The native demo starts with its in-memory adapter. The web build uses
/// the shared bootstrap's default adapter until a host injects one.
pub fn init_state() -> AppState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::bootstrap::start_shared_app_state(Some(Box::new(DemoAdapter::default())))
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::bootstrap::start_shared_app_state(None)
    }
}

pub fn app_key(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if crate::views::shell_menu::shortcut(state, ev, tx) {
        return true;
    }
    if state.active != Section::Workspace {
        // T145: the workflow page's yaml document takes the keyboard while
        // the edit pane's yaml mode is the active panel (the lite editor's
        // `workflow_editing` gate this replaces).
        return state.active == Section::Workflows
            && state.workflow_yaml_focused()
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
            if matches!(ev.key, Key::Escape) {
                if let Some(grid) = state.result_grid.as_mut() {
                    grid.clear_selection();
                }
                state.result_focused = false;
                return true;
            } else if matches!(ev.key, Key::Text(_)) {
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
        if !state.catalog_completion_current() {
            state.sql.clear_catalog_completion();
        }
        let consumed = sql_editor::on_key(&mut state.sql, ev, tx);
        if consumed && (matches!(ev.key, Key::Backspace)
            || (matches!(ev.key, Key::Text(_)) && !ev.mods.cmd && !ev.mods.ctrl))
        {
            state.refresh_native_from_completion();
        }
        return consumed;
    }
    if state.grid.has_page() {
        return crate::data_grid::on_key(&mut state.grid, ev, tx);
    }
    false
}

pub fn app_poll(state: &mut AppState, now: f64, tx: &mut Tx) -> bool {
    // T144: the home's per-row copy marker carries a real deadline; when the
    // runtime wakes at expiry the label must revert on the next frame.
    let mut dirty = crate::views::home_copy_tick(state, now);
    dirty |= state.query_copy_feedback_tick(now);
    dirty |= state.tick_native_prompts(now);
    // N1: the host's finished jobs land first, whatever section is showing;
    // a delivery redraws (it wrote state or cleared its marker), and stale
    // generations were dropped inside `deliver_host_job`.
    for delivery in state.desktop.poll_jobs() {
        state.deliver_host_job(delivery);
        dirty = true;
    }
    // T009-S7: the AI panel's host sync runs on every section — its
    // mount tracking has to see the workspace being left, so the
    // unmount cancel fires on the transition.
    dirty = state.sync_ai_chat_panel(tx) || dirty;
    // T012: the write-proposal host's expiry tick runs on every section —
    // the dialog is mounted at the app root, not inside a section.
    dirty = state.sync_ai_write_proposals() || dirty;
    dirty |= state.restore_query_editor_scroll(tx);
    // A captured cell might not receive Up once its virtual row or table
    // subtree is gone. Cancel the app's gesture before it can affect a newer grid.
    let table_grid_mounted = state.active == Section::Workspace
        && state.native_kv.is_none()
        && state.er_schema.is_none()
        && state.table_read_error.is_none()
        && state.table_loading.is_none()
        && state.table_view == crate::types::TableView::Data
        && state.tabs.selected().is_some_and(|tab| tab.kind == datazen_ui_api::TabKind::Table);
    dirty |= state.grid.cancel_table_drag_if_needed(table_grid_mounted);
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

pub fn app_ime(state: &mut AppState, ev: &Ime, _: &mut Tx) -> bool {
    if state.active == Section::Workflows && state.workflow_yaml_focused() {
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
