//! T167 independent audit (tester, not coder).
//!
//! Precise probes of uncovered branches of the host port: the success messages
//! are checked against the real i18n text with placeholders filled independently
//! of the port's own formatter; the message copy's feedback wake is pinned to
//! 1500ms and its label flip/revert is observed; long messages are checked with
//! `visible` rects at both sizes and palettes; and an export IPC failure with an
//! empty raw message is checked to use the fallback and never refresh.

use rune::{col, clipboard, Cx, El, Env, Runtime, Tx};

use crate::components::connection::connection_share_dialog::{
    ImportResult, ImportSource, ShareEvent, ShareMode,
};
use crate::components::connection::connection_share_dialog_host::{
    format_export_success, format_import_success, ConnectionShareDialogHost, HostView, MessageKind,
    ShareHostPort,
};
use crate::i18n::t;

const MESSAGE: &str = "result-message-dialog";

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
struct AuditScene {
    host: ConnectionShareDialogHost,
    dark: bool,
    copied: bool,
    press_clock: Option<f64>,
    copy_calls: usize,
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let dark = s.dark;
    let mut layer = Vec::new();
    HostView::<AuditScene>::new(
        &s.host,
        |st: &mut AuditScene| &mut st.host.share.password,
        |st: &mut AuditScene| &mut st.host.share.confirm_password,
        |st: &mut AuditScene| &mut st.host.share.import.password,
        |st: &mut AuditScene| &mut st.host.share.import.data_path,
    )
    .copied(s.copied)
    .on_message_close(|st: &mut AuditScene| st.host.close_message())
    .on_copy(|st: &mut AuditScene, tx: &mut Tx| {
        st.copy_calls += 1;
        st.press_clock = Some(tx.clock());
    })
    .build(dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
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

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn import_result(skipped: usize) -> ImportResult {
    ImportResult {
        imported: 2,
        overwritten: 1,
        groups_added: 3,
        skipped: (0..skipped).map(|i| format!("kind-{i}")).collect(),
        source_format: None,
    }
}

// 1. The formatters must use the real i18n keys and fill every placeholder.
//    The shipped tests compare the message to the same helper, so a wrong key
//    or placeholder name would stay green; this assertion is independent.
#[test]
fn success_messages_are_the_real_i18n_text_with_placeholders_filled() {
    assert_eq!(format_export_success(7), t("connShare.exportSuccess").replace("{count}", "7"));

    let plain = import_result(0);
    let expected_plain = t("connShare.importSuccess")
        .replace("{imported}", "2")
        .replace("{overwritten}", "1")
        .replace("{groupsAdded}", "3");
    assert_eq!(format_import_success(&plain), expected_plain);

    let skipped = import_result(2);
    let expected_skipped = t("connShare.importSuccessWithSkipped")
        .replace("{imported}", "2")
        .replace("{overwritten}", "1")
        .replace("{groupsAdded}", "3")
        .replace("{skipped}", "2");
    assert_eq!(format_import_success(&skipped), expected_skipped);
    assert_ne!(format_import_success(&skipped), expected_plain);
}

// 2. The error message copy schedules a real 1500ms wake and the label flips to
//    common.copied while the caller's copied flag is set, reverting otherwise.
#[test]
fn message_copy_pins_the_1500ms_wake_and_the_copied_label_flip() {
    let mut scene = AuditScene::default();
    scene.host.show_message("boom", MessageKind::Error);
    let clip = clipboard::memory();
    let mut rt = runtime(scene, (1440.0, 816.0));
    rt.use_clipboard(&clip);
    rt.frame(0.0);
    click(&mut rt, "result-message-copy");

    assert_eq!(clip.text().as_deref().map(String::as_str), Some("boom"));
    assert_eq!(rt.state.copy_calls, 1);
    let press = rt.state.press_clock.expect("copy press clock recorded");
    let deadline = rt.wake_deadline().expect("message copy schedules a real wake");
    assert!(
        (deadline - press - 1.5).abs() <= 0.01,
        "feedback wake must be 1500ms after the press: press={press} deadline={deadline}"
    );

    let copied = runtime(
        AuditScene { copied: true, ..rt.state.clone() },
        (1440.0, 816.0),
    );
    assert_eq!(label_of(&copied, "result-message-copy"), t("common.copied"));
    let reverted = runtime(rt.state.clone(), (1440.0, 816.0));
    assert_eq!(label_of(&reverted, "result-message-copy"), t("common.copy"));
}

// 3. Long error and success messages must stay inside the window at both sizes
//    and palettes; observe `visible` (clipped) rects, not the layout rect.
#[test]
fn long_messages_stay_inside_the_window_at_both_sizes_and_palettes() {
    let long = "x".repeat(600);
    for kind in [MessageKind::Error, MessageKind::Success] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            for dark in [true, false] {
                let mut host = ConnectionShareDialogHost::new();
                host.show_message(long.clone(), kind);
                let rt = runtime(AuditScene { host, dark, ..AuditScene::default() }, size);
                assert!(has_key(&rt, MESSAGE), "{kind:?} {size:?} dark={dark}: message mounted");
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{kind:?} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path),
                        e.visible,
                        e.rect
                    );
                }
            }
        }
    }
}

// 4. An export IPC failure with an empty raw message falls back, closes the
//    share dialog, shows an error and never refreshes.
#[test]
fn an_export_failure_uses_the_fallback_and_does_not_refresh() {
    let mut scene = AuditScene::default();
    scene.host = ConnectionShareDialogHost::new();
    scene.host.open(ShareMode::Export, ImportSource::File);
    scene.host.share.epoch = 1;
    let mut port = RefreshPort::default();
    scene.host.apply_event(
        ShareEvent::Exported { epoch: 1, result: Err(String::new()) },
        "export failed",
        &mut port,
    );

    assert!(!scene.host.share.open, "the share dialog closes on error");
    assert_eq!(scene.host.message.kind, MessageKind::Error);
    assert_eq!(scene.host.message.text, "export failed", "empty raw message uses the fallback");
    assert_eq!(port.count, 0, "a failed export never refreshes");

    let rt = runtime(scene, (1440.0, 816.0));
    assert!(!has_key(&rt, "connection-share-dialog"));
    assert_eq!(text_of(&rt, "result-message-text"), "export failed");
}
