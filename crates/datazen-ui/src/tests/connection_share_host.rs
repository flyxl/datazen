//! T167: `components/connection/ConnectionShareDialogHost.tsx` ported to Rune.
//!
//! Probes drive the host-owned state directly (React drives a zustand store)
//! and observe the mounted dialogs through the runtime element tree. Callback
//! wiring is pinned by feeding T137 `ShareEvent`s through the host port and
//! reading the resulting message dialog / refresh count — no fake IPC.

use rune::{col, Cx, El, Env, Runtime, Tx};

use crate::components::connection::connection_share_dialog::{
    ImportResult, ImportSource, ShareAction, ShareEvent, ShareMode,
};
use crate::components::connection::connection_share_dialog_host::{
    format_export_success, format_import_success, ConnectionShareDialogHost, HostView, MessageKind,
    ShareHostPort,
};
use crate::i18n::t;

const SHARE: &str = "connection-share-dialog";
const MESSAGE: &str = "result-message-dialog";
const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Default)]
struct RefreshPort {
    count: usize,
}

impl ShareHostPort for RefreshPort {
    fn refresh_connections(&mut self) {
        self.count += 1;
    }
}

#[derive(Clone, Default)]
struct Scene {
    host: ConnectionShareDialogHost,
    dark: bool,
    actions: Vec<ShareAction>,
    message_closes: usize,
    copies: usize,
}

impl Scene {
    fn opened(mode: ShareMode, source: ImportSource, dark: bool) -> Self {
        let mut host = ConnectionShareDialogHost::new();
        host.open(mode, source);
        Self { host, dark, ..Self::default() }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let dark = s.dark;
    let mut layer = Vec::new();
    HostView::<Scene>::new(
        &s.host,
        |st: &mut Scene| &mut st.host.share.password,
        |st: &mut Scene| &mut st.host.share.confirm_password,
        |st: &mut Scene| &mut st.host.share.import.password,
        |st: &mut Scene| &mut st.host.share.import.data_path,
    )
    .on_action(|st: &mut Scene, action: ShareAction| {
        st.actions.push(action.clone());
        st.host.handle_action(action);
    })
    .on_message_close(|st: &mut Scene| {
        st.message_closes += 1;
        st.host.close_message();
    })
    .on_copy(|st: &mut Scene, _tx: &mut Tx| st.copies += 1)
    .build(dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn import_result(skipped: bool) -> ImportResult {
    ImportResult {
        imported: 2,
        overwritten: 1,
        groups_added: 3,
        skipped: if skipped { vec!["unsupported".to_owned()] } else { Vec::new() },
        source_format: None,
    }
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {:?}: {outside:?}", outside.len(), size);
}

// --- 1. open / mode transitions ---------------------------------------------

#[test]
fn open_uses_the_requested_mode_and_source_and_cancel_closes_it() {
    let unopened = runtime(Scene::default(), DESIGN);
    assert!(!has_key(&unopened, SHARE), "the host mounts nothing while the store is closed");

    let mut rt = runtime(
        Scene::opened(ShareMode::Import, ImportSource::Dbx, true),
        DESIGN,
    );
    assert!(has_key(&rt, SHARE));
    assert_eq!(element(&rt, &format!("{SHARE}-title")).text.as_deref(), Some(
        t("connShare.importFromAppTitle").replace("{app}", "DBX").as_str()
    ));

    click(&mut rt, &format!("{SHARE}-cancel"));
    assert!(rt.state.actions.contains(&ShareAction::Close), "cancel emits the close action");
    assert!(!rt.state.host.share.open, "the host translates Close into store close");
    let after = runtime(rt.state.clone(), DESIGN);
    assert!(!has_key(&after, SHARE), "the closed dialog unmounts on the next mount");
}

#[test]
fn re_opening_with_a_new_mode_resets_the_dialog_state() {
    let mut scene = Scene::opened(ShareMode::Export, ImportSource::File, true);
    scene.host.share.password = rune::TextEdit::new("stale".to_owned());
    scene.host.open(ShareMode::Import, ImportSource::Navicat);

    assert_eq!(scene.host.share.mode, ShareMode::Import);
    assert_eq!(scene.host.share.source, ImportSource::Navicat);
    assert!(scene.host.share.password.text.is_empty(), "a fresh open has fresh fields");
    let rt = runtime(scene, DESIGN);
    assert_eq!(element(&rt, &format!("{SHARE}-title")).text.as_deref(), Some(
        t("connShare.importFromAppTitle").replace("{app}", "Navicat").as_str()
    ));
}

// --- 2. callback wiring ------------------------------------------------------

#[test]
fn export_success_closes_and_shows_the_success_message_without_refresh() {
    let mut scene = Scene::opened(ShareMode::Export, ImportSource::File, true);
    scene.host.share.epoch = 1;
    let mut port = RefreshPort::default();
    scene.host.apply_event(ShareEvent::Exported { epoch: 1, result: Ok(3) }, "fallback", &mut port);

    assert!(!scene.host.share.open);
    assert_eq!(scene.host.message.kind, MessageKind::Success);
    assert_eq!(scene.host.message.text, format_export_success(3));
    assert_eq!(port.count, 0, "an export does not refresh the connection list");

    let rt = runtime(scene, DESIGN);
    assert!(!has_key(&rt, SHARE));
    assert_eq!(text_of(&rt, "result-message-text"), format_export_success(3));
    assert_eq!(text_of(&rt, &format!("{MESSAGE}-title")), t("common.success"));
    assert!(has_key(&rt, "result-message-ok"));
    assert!(!has_key(&rt, "result-message-copy"), "the copy control is error-only");
}

#[test]
fn import_success_shows_the_formatted_message_and_refreshes_once_per_success() {
    let mut scene = Scene::opened(ShareMode::Import, ImportSource::File, true);
    let mut port = RefreshPort::default();
    let plain = import_result(false);
    scene.host.share.epoch = 1;
    scene.host.apply_event(
        ShareEvent::Imported { epoch: 1, result: Ok(plain.clone()) },
        "fallback",
        &mut port,
    );
    assert_eq!(scene.host.message.text, format_import_success(&plain));
    assert_eq!(port.count, 1, "fetchConnections + fetchGroups are one host action");
    assert_eq!(scene.host.message.kind, MessageKind::Success);

    let skipped = import_result(true);
    scene.host.share.epoch = 2;
    scene.host.apply_event(
        ShareEvent::Imported { epoch: 2, result: Ok(skipped.clone()) },
        "fallback",
        &mut port,
    );
    assert_ne!(format_import_success(&skipped), format_import_success(&plain));
    assert_eq!(scene.host.message.text, format_import_success(&skipped));
    assert_eq!(port.count, 2);
}

#[test]
fn an_error_closes_shows_the_error_copy_control_and_does_not_refresh() {
    let mut scene = Scene::opened(ShareMode::Import, ImportSource::Dbx, true);
    scene.host.share.epoch = 1;
    let mut port = RefreshPort::default();
    scene.host.apply_event(
        ShareEvent::Imported { epoch: 1, result: Err("boom".to_owned()) },
        "fallback",
        &mut port,
    );
    assert!(!scene.host.share.open);
    assert_eq!(scene.host.message.kind, MessageKind::Error);
    assert_eq!(scene.host.message.text, "boom");
    assert_eq!(port.count, 0);

    // A stale epoch must produce no output at all.
    scene.host.share.epoch = 2;
    scene.host.apply_event(ShareEvent::Exported { epoch: 1, result: Ok(9) }, "fallback", &mut port);
    assert_eq!(scene.host.message.text, "boom", "a late answer cannot overwrite the message");
    assert_eq!(port.count, 0);

    let mut rt = runtime(scene, DESIGN);
    assert_eq!(text_of(&rt, &format!("{MESSAGE}-title")), t("common.error"));
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    click(&mut rt, "result-message-copy");
    assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("boom"));
    assert_eq!(rt.state.copies, 1, "the copy callback runs once");
    assert!(rt.wake_deadline().is_some(), "copy schedules real 1500ms feedback wake");
}

#[test]
fn the_message_ok_button_dismisses_only_the_message_dialog() {
    let mut scene = Scene::opened(ShareMode::Export, ImportSource::File, true);
    scene.host.show_message("done", MessageKind::Success);
    let mut rt = runtime(scene, DESIGN);
    click(&mut rt, "result-message-ok");

    assert_eq!(rt.state.message_closes, 1);
    assert!(!rt.state.host.message.open);
    let after = runtime(rt.state.clone(), DESIGN);
    assert!(!has_key(&after, MESSAGE), "the message dialog unmounts on the next mount");
}

// --- 3. geometry -------------------------------------------------------------

#[test]
fn share_and_message_dialogs_fit_both_window_sizes_and_both_palettes() {
    let mut error_scene = Scene::default();
    error_scene.host.show_message("a short failure", MessageKind::Error);
    let mut success_scene = Scene::default();
    success_scene.host.show_message(format_import_success(&import_result(true)), MessageKind::Success);

    let scenes: Vec<(&str, Scene)> = vec![
        ("export", Scene::opened(ShareMode::Export, ImportSource::File, true)),
        ("import-app", Scene::opened(ShareMode::Import, ImportSource::Dbx, true)),
        ("error", error_scene),
        ("success", success_scene),
    ];
    for (name, scene) in scenes {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let s = Scene { dark, ..scene.clone() };
                let rt = runtime(s, size);
                let root = if name == "error" || name == "success" { MESSAGE } else { SHARE };
                assert!(has_key(&rt, root), "{name} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}
