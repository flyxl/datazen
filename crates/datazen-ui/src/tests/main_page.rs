//! T020: the `MainPage` port — the three React branches, the load-error
//! screen (keys, message, chrome title, dialog host), the retry path, and the
//! seams that must stay honest: the unwired marker keeps the port's page, and
//! an open-job sentence never gates the window.
//!
//! The observation channel is the a11y tree (keys, rects, roles, labels and
//! text) of the real `crate::view` over a live list host — nothing is asked
//! of the module under test directly.

use super::*;
use crate::i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

/// A live host whose connection list answers the sentence it is handed, or
/// the one-connection list when the sentence is gone: MainPage's gate against
/// a real host, not the memory port.
struct ListHostAdapter {
    sentence: Rc<RefCell<Option<String>>>,
}

impl ListHostAdapter {
    fn failing(sentence: &str) -> (Self, Rc<RefCell<Option<String>>>) {
        let sentence = Rc::new(RefCell::new(Some(sentence.to_owned())));
        (Self { sentence: Rc::clone(&sentence) }, sentence)
    }
}

impl DesktopAdapter for ListHostAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        match self.sentence.borrow().clone() {
            Some(sentence) => Err(sentence),
            None => Ok(vec![ConnectionProfile {
                id: "live-1".into(),
                name: "Live one".into(),
                engine: "postgres".into(),
            }]),
        }
    }
    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

fn main_runtime(state: AppState, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_healthy_branch_keeps_the_shell_the_move_must_not_change() {
    // The never-synced default state keeps the memory port's list, exactly
    // as `AppState::default()` always has — the composition move from
    // `app.rs` must not drop a keyed part of the shell.
    let rt = main_runtime(AppState::default(), (1440.0, 900.0), true);
    for key in [
        "datazen-root",
        "datazen-titlebar",
        "datazen-titlebar-label",
        "datazen-theme-toggle",
        "datazen-body",
        "datazen-page",
        "datazen-rail",
    ] {
        assert!(has_key(&rt, key), "{key} present: the shell is whole");
    }
    assert!(!has_key(&rt, "main-load-error"), "no failure screen");
}

#[test]
fn a_failed_first_list_load_swaps_the_window_for_the_react_error_screen() {
    // React: `connections.length === 0 && loadError` renders the chrome plus
    // the centered sentence and the retry (MainPage.tsx:112-132).
    let (adapter, _sentence) = ListHostAdapter::failing("the host's store is unreachable");
    let mut state = AppState::default();
    state.desktop = Box::new(adapter);
    state.sync_desktop_connections();

    let rt = main_runtime(state, (1440.0, 900.0), true);

    // The workspace body is gone; the failure screen took the window.
    assert!(has_key(&rt, "main-load-error"), "the failure screen mounts");
    assert!(
        !has_key(&rt, "datazen-body"),
        "the connection page left with the gate"
    );

    // The sentence, verbatim, through the a11y tree.
    let message = element(&rt, "main-load-error-message");
    assert_eq!(
        message.text.as_deref(),
        Some("the host's store is unreachable"),
        "the load error is the displayed sentence"
    );

    // The retry button: a real button, i18n, React's testid key.
    let retry = element(&rt, "main-load-retry");
    assert_eq!(retry.role, "button");
    assert_eq!(
        retry.label.as_deref(),
        Some(t("common.retry")),
        "retry reads the zh-CN table, not a hard-coded label"
    );

    // The failure branch keeps the chrome: the titlebar names the app
    // (t('menu.appName')) and the theme toggle stays a hit target.
    let title = element(&rt, "datazen-titlebar-label");
    assert_eq!(
        title.text.as_deref(),
        Some(t("menu.appName")),
        "the failure chrome titles the app, not the section"
    );
    assert!(has_key(&rt, "datazen-titlebar"));
    assert!(has_key(&rt, "datazen-theme-toggle"));
}

#[test]
fn the_error_screen_still_mounts_the_connection_dialog_host() {
    // React's failure branch renders `ConnectionEditorDialogHost` and
    // `ConnectionShareDialogHost` under the notice (MainPage.tsx:129-130);
    // the port's dialog layer rides the same branch.
    let (adapter, _sentence) = ListHostAdapter::failing("no store today");
    let mut state = AppState::default();
    state.desktop = Box::new(adapter);
    state.sync_desktop_connections();
    state.connection_dialog_open = true;
    state.connection_dialog_epoch = 1;

    let rt = main_runtime(state, (1440.0, 900.0), true);
    assert!(
        has_key(&rt, "main-load-error"),
        "the failure screen is the body"
    );
    assert!(
        has_key(&rt, "connection-dialog-title"),
        "the connection dialog host still mounts over the failure screen"
    );
}

#[test]
fn retry_refetches_the_list_and_brings_the_workspace_back() {
    // React: `main-load-retry` runs `fetchConnections`; a successful fetch
    // clears the store's error and the window becomes the connection page.
    let (adapter, sentence) = ListHostAdapter::failing("the host's store is unreachable");
    let mut state = AppState::default();
    state.desktop = Box::new(adapter);
    state.sync_desktop_connections();

    let mut rt = main_runtime(state, (1440.0, 900.0), true);
    assert!(has_key(&rt, "main-load-error"));

    // The host answers from now on.
    *sentence.borrow_mut() = None;
    click(&mut rt, "main-load-retry");

    assert!(
        has_key(&rt, "datazen-body"),
        "the connection page came back with the refetched list"
    );
    assert!(!has_key(&rt, "main-load-error"), "the failure screen left");
    assert!(
        !has_key(&rt, "desktop-adapter-message"),
        "a successful fetch clears the banner sentence"
    );
    assert_eq!(
        rt.state.desktop_connections.as_deref().map(<[ConnectionProfile]>::len),
        Some(1),
        "the refetched list is the store's list"
    );
    assert!(
        rt.state.connections_load_error.is_none(),
        "the store's error cleared with the fetch"
    );
}

#[test]
fn the_unwired_marker_keeps_the_ports_page_and_banner() {
    // The fixed not-implemented sentence is the unwired seam, not a load
    // failure: the memory port answers by design (app.rs H3) and the page
    // keeps its banner — the wasm shell keeps its demo.
    let mut state = AppState::default();
    state.desktop = Box::new(UnimplementedAdapter);
    state.sync_desktop_connections();

    let rt = main_runtime(state, (1440.0, 900.0), true);
    assert!(
        has_key(&rt, "datazen-body"),
        "the unwired marker does not gate the window"
    );
    assert!(
        !has_key(&rt, "main-load-error"),
        "no failure screen for the unwired seam"
    );
    assert!(
        has_key(&rt, "desktop-adapter-message"),
        "the page keeps its banner sentence"
    );
}

#[test]
fn an_open_job_sentence_does_not_gate_the_window() {
    // A failed open lands in the shared notice slot (`desktop_error`, as
    // `deliver_host_job` writes it) while the store's list fetch never
    // failed; React gates on the store's `error` only, so the connections
    // page and its banner stay.
    let mut state = AppState::default();
    state.desktop_error = Some("cannot open 'local'".into());

    let rt = main_runtime(state, (1440.0, 900.0), true);
    assert!(
        has_key(&rt, "datazen-body"),
        "an open failure keeps the connections page"
    );
    assert!(!has_key(&rt, "main-load-error"), "no failure screen");
    assert!(
        has_key(&rt, "desktop-adapter-message"),
        "the open sentence shows as the page's banner"
    );
}

#[test]
fn a_later_list_failure_keeps_the_previous_list_and_the_page() {
    // The zustand store keeps its `connections` when a fetch fails
    // (connectionStore.ts writes only the error), so the window keeps the
    // workspace and the page shows the new sentence as its banner.
    let (adapter, sentence) = ListHostAdapter::failing("flaky host");
    // The first fetch answers the one-connection list; the next one fails.
    *sentence.borrow_mut() = None;
    let mut state = AppState::default();
    state.desktop = Box::new(adapter);
    state.sync_desktop_connections();

    let mut rt = main_runtime(state, (1440.0, 900.0), true);
    assert!(has_key(&rt, "datazen-body"));

    *sentence.borrow_mut() = Some("flaky host".into());
    rt.state.sync_desktop_connections();
    // The store changed outside a handler, so ask for the next frame's
    // rebuild explicitly (`settle_all` marks the runtime dirty).
    rt.settle_all();
    rt.frame(0.0);

    assert!(
        has_key(&rt, "datazen-body"),
        "a later fetch failure keeps the connection page"
    );
    assert!(!has_key(&rt, "main-load-error"), "no failure screen");
    let banner = element(&rt, "desktop-adapter-message");
    assert_eq!(
        banner.text.as_deref(),
        Some("flaky host"),
        "the page shows the new sentence as its banner"
    );
    assert_eq!(
        rt.state.desktop_connections.as_deref().map(<[ConnectionProfile]>::len),
        Some(1),
        "the store's previous list survived the failed fetch"
    );
}

#[test]
fn the_error_screen_fits_both_window_sizes_and_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let (adapter, _sentence) = ListHostAdapter::failing("the host's store is unreachable");
            let mut state = AppState::default();
            state.desktop = Box::new(adapter);
            state.sync_desktop_connections();

            let rt = main_runtime(state, (w, h), dark);
            for key in [
                "main-load-error",
                "main-load-error-message",
                "main-load-retry",
                "datazen-titlebar",
                "datazen-theme-toggle",
            ] {
                let e = element(&rt, key);
                assert!(
                    e.rect[0] >= -0.5 && e.rect[0] + e.rect[2] <= w + 0.5,
                    "{key} out of {w}x{h} (dark={dark}): {:?}",
                    e.rect
                );
            }

            // React's md button (`h-9` = 36pt) and the 40pt chrome band.
            let retry = element(&rt, "main-load-retry");
            assert!(
                (retry.rect[3] - 36.0).abs() < 0.5,
                "the retry is React's md button: {:?}",
                retry.rect
            );
            let titlebar = element(&rt, "datazen-titlebar");
            assert!((titlebar.rect[3] - 40.0).abs() < 0.5, "chrome band 40pt");

            // `items-center justify-center`: the sentence and the retry share
            // the window's horizontal center (the titlebar keeps the inset).
            for key in ["main-load-error-message", "main-load-retry"] {
                let e = element(&rt, key);
                let center = e.rect[0] + e.rect[2] / 2.0;
                assert!(
                    (center - w / 2.0).abs() < 0.5,
                    "{key} centered at {center}, want {}: {:?}",
                    w / 2.0,
                    e.rect
                );
            }

            // The failure screen sits below the 40pt chrome band, filling
            // the rest of the window (React `flex-1`).
            let failure = element(&rt, "main-load-error");
            assert!(
                (failure.rect[1] - 40.0).abs() < 0.5
                    && (failure.rect[1] + failure.rect[3] - h).abs() < 0.5,
                "the failure pane fills the window under the chrome: {:?}",
                failure.rect
            );
        }
    }
}
