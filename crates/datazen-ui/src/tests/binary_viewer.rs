use rune::{Cx, El, Env, Runtime};

use crate::ui::binary_viewer::BinaryViewer;

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    element(rt, key).text
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -1.0 || r[1] < -1.0 || r[0] + r[2] > w + 1.0 || r[1] + r[3] > h + 1.0 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[derive(Default)]
struct Probe;

const HELLO_BYTES: &[u8] = b"Hello, world!\x00\x01\x02";
const PAD_BYTES: &[u8] = &[0x41, 0x00, 0x7f];
static RANGE_BYTES: [u8; 64] = {
    let mut arr = [0u8; 64];
    let mut i = 0;
    while i < 64 {
        arr[i] = i as u8;
        i += 1;
    }
    arr
};

fn single_row_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    BinaryViewer::new("bv", HELLO_BYTES).build(true)
}

fn pad_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    BinaryViewer::new("bv", PAD_BYTES).row_bytes(4).build(true)
}

fn empty_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    BinaryViewer::new("bv", &[]).build(true)
}

fn size_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    BinaryViewer::new("bv", &RANGE_BYTES).build(true)
}

#[test]
fn single_row_has_offset_hex_and_ascii_sections() {
    let mut rt = Runtime::new(Probe, single_row_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "bv-line-0"), "row present");
    assert_eq!(
        text_of(&rt, "bv-offset-0").as_deref(),
        Some("00000000"),
        "offset"
    );
    assert_eq!(
        text_of(&rt, "bv-hex-bytes-0").as_deref(),
        Some("48 65 6c 6c 6f 2c 20 77 6f 72 6c 64 21 00 01 02"),
        "whole-row hex bytes"
    );
    let ascii = text_of(&rt, "bv-ascii-0").expect("ascii");
    assert_eq!(ascii, "Hello,.world!...", "bare ascii gutter (no pipes)");
}

#[test]
fn incomplete_row_pads_hex_but_not_ascii_placeholders_and_nonprintable_is_dot() {
    let mut rt = Runtime::new(Probe, pad_view, (960.0, 640.0), env());
    rt.frame(0.0);
    // present bytes "xx " + missing "   " then trimEnd() => trailing missing slots removed
    assert_eq!(text_of(&rt, "bv-hex-bytes-0").as_deref(), Some("41 00 7f"));
    assert_eq!(
        text_of(&rt, "bv-ascii-0").as_deref(),
        Some("A.."),
        "ascii has no placeholders, nonprintable -> dot, no pipes"
    );
}

#[test]
fn empty_input_renders_zero_data_rows() {
    let mut rt = Runtime::new(Probe, empty_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "bv"), "root present");
    assert!(has_key(&rt, "bv-hex"), "hex body present");
    assert!(!has_key(&rt, "bv-line-0"), "no data rows for empty input");
    assert!(!has_key(&rt, "bv-hex-truncated"), "no truncation marker");
}

#[test]
fn binary_viewer_stays_in_window_at_two_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(Probe, size_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("binary {w}x{h}"));
        let root = element(&rt, "bv");
        eprintln!("binary {w}x{h} root rect={:?}", root.rect);
    }
}
