use rune::{Cx, El, Env, Runtime};

use crate::ui::copyable::CopyableError;

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

#[derive(Default)]
struct Probe {
    copied: usize,
}

fn copyable_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    let w = CopyableError::new("probe-copy", "exact message")
        .copy_button(true)
        .on_copy(|s: &mut Probe| s.copied += 1)
        .build::<Probe>(k);
    rune::col().w(480.0).h(240.0).padding(8.0).child(w)
}

fn copyable_disconnected(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    // No copy_button: control absent, so copy cannot happen via UI.
    let w = CopyableError::new("probe-copy", "exact message").build::<Probe>(k);
    rune::col().w(480.0).h(240.0).padding(8.0).child(w)
}

fn mono_face_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = crate::ui::kit::kit(true);
    // "i" is narrow in proportional faces but fixed-width in monospace.
    let sample = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";
    let plain = CopyableError::new("probe-plain", sample).build::<Probe>(k);
    let mono = CopyableError::new("probe-mono", sample)
        .mono(true)
        .build::<Probe>(k);
    // Same structure as CopyableError (non-button): col(w_full) > text(mono).
    let ref_mono = rune::col().key("ref-mono-wrap").w_full().child(
        rune::text(sample)
            .key("ref-mono")
            .font(k.ty.mono)
            .color(k.c.danger.fg)
            .selectable(true)
            .wrap_anywhere(),
    );
    let ref_plain = rune::col().key("ref-plain-wrap").w_full().child(
        rune::text(sample)
            .key("ref-plain")
            .font(k.ty.text_xsmall)
            .color(k.c.danger.fg)
            .selectable(true)
            .wrap_anywhere(),
    );
    rune::col()
        .w(720.0)
        .h(320.0)
        .padding(8.0)
        .gap(8.0)
        .child(ref_plain)
        .child(ref_mono)
        .child(plain)
        .child(mono)
}

#[test]
fn copy_control_copies_exact_message_and_reaches_probe() {
    let clipboard = rune::clipboard::memory();
    let mut rt = Runtime::new(Probe::default(), copyable_view, (640.0, 320.0), env());
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    let root = element(&rt, "probe-copy");
    assert_eq!(root.role, "alert", "non-mono is an alert");
    let text = element(&rt, "probe-copy-text");
    assert_eq!(text.text.as_deref(), Some("exact message"), "message text");

    let btn = element(&rt, "probe-copy-copy");
    assert!(btn.focusable, "copy control focusable");
    assert!(
        btn.label.as_deref().is_some_and(|l| !l.is_empty()),
        "copy control has accessible label: {:?}",
        btn.label
    );

    click(&mut rt, "probe-copy-copy");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("exact message"),
        "exact message copied to runtime clipboard"
    );
    assert_eq!(rt.state.copied, 1, "on_copy callback reached probe");
}

#[test]
fn copy_reverse_verification_disconnects_control() {
    // Reverse: without copy_button, no copy control is mounted and clipboard/probe unchanged.
    let clipboard = rune::clipboard::memory();
    clipboard.set_text("kept");
    let mut rt = Runtime::new(Probe::default(), copyable_disconnected, (640.0, 320.0), env());
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    assert!(
        rt.find("probe-copy-copy").is_none(),
        "no copy control without copy_button"
    );
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("kept"),
        "no clipboard write without control"
    );
    assert_eq!(rt.state.copied, 0, "callback cannot fire without control");
}

#[test]
fn mono_variant_uses_mono_face_and_is_not_an_alert() {
    let mut rt = Runtime::new(Probe::default(), mono_face_view, (720.0, 320.0), env());
    rt.frame(0.0);

    let mono_root = element(&rt, "probe-mono");
    assert_ne!(
        mono_root.role, "alert",
        "mono/technical block is not announced as alert"
    );
    let plain_root = element(&rt, "probe-plain");
    assert_eq!(plain_root.role, "alert", "non-mono is alert");

    let plain_text = element(&rt, "probe-plain-text");
    let mono_text = element(&rt, "probe-mono-text");
    let sample = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";
    assert_eq!(plain_text.text.as_deref(), Some(sample));
    assert_eq!(mono_text.text.as_deref(), Some(sample));

    let ref_mono = element(&rt, "ref-mono").rect;
    let ref_plain = element(&rt, "ref-plain").rect;
    // Mono copyable text must match mono-styled reference metrics.
    assert!(
        (mono_text.rect[2] - ref_mono[2]).abs() <= 1.0
            && (mono_text.rect[3] - ref_mono[3]).abs() <= 1.0,
        "mono variant should use mono face metrics: mono_text={:?} ref_mono={ref_mono:?}",
        mono_text.rect
    );
}

