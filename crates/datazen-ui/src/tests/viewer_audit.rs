//! Independent audit for JsonViewer/BinaryViewer bases (T0-11 batch 3).

use rune::{Cx, El, Env, Runtime};

use crate::ui::binary_viewer::BinaryViewer;
use crate::ui::json_viewer::{JsonDisplayMode, JsonViewer};

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

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
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
struct P;

// ---- JsonViewer boundaries ---------------------------------------------------------------

fn empty_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", "").mode(JsonDisplayMode::Tree).build(true)
}

fn whitespace_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", "   \n\t  ").mode(JsonDisplayMode::Raw).build(true)
}

const SIX: &str = r#"{"l1":{"l2":{"l3":{"l4":{"l5":{"v":1}}}}}}"#;
fn six_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", SIX).build(true)
}

const BIG_NUM_SRC: &str = r#"{"big":9007199254740993,"neg":-9007199254740993,"float":1.2345678901234567}"#;
fn big_raw_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", BIG_NUM_SRC).mode(JsonDisplayMode::Raw).build(true)
}
fn big_pretty_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", BIG_NUM_SRC).mode(JsonDisplayMode::Pretty).build(true)
}

static LONG_LINE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
fn long_line_src() -> &'static str {
    LONG_LINE.get_or_init(|| {
        let long = "x".repeat(240);
        format!("{{\"k\":\"{long}\"}}")
    })
}
fn long_pretty_view(_s: &P, _cx: &mut Cx<P>) -> El {
    JsonViewer::new("jv", long_line_src()).mode(JsonDisplayMode::Pretty).build(true)
}

#[test]
fn empty_input_is_unparseable_shows_parse_error_raw_only_and_original_text() {
    let mut rt = Runtime::new(P, empty_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "jv-parse-error"), "empty input => parse-error marker");
    assert!(has_key(&rt, "jv-mode-bar"), "mode bar present");
    assert!(has_key(&rt, "jv-mode-raw"), "raw mode available");
    assert!(!has_key(&rt, "jv-mode-tree"), "tree unavailable");
    assert!(!has_key(&rt, "jv-mode-pretty"), "pretty unavailable");
    assert!(!has_key(&rt, "jv-mode-minify"), "minify unavailable");
    assert!(has_key(&rt, "jv-text"), "original text body present");
    let found_empty = texts(&rt).iter().any(|t| t.is_empty() || t == "");
    // original is empty string; body text element exists and contains it
    assert!(found_empty || element(&rt, "jv-text").text.as_deref() == Some(""), "empty original visible");
}

#[test]
fn whitespace_only_input_is_unparseable_shows_parse_error_raw_only_and_original_text() {
    let mut rt = Runtime::new(P, whitespace_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "jv-parse-error"), "whitespace-only => parse-error marker");
    assert!(has_key(&rt, "jv-mode-bar"), "mode bar present");
    assert!(has_key(&rt, "jv-mode-raw"), "raw mode available");
    assert!(!has_key(&rt, "jv-mode-tree"), "tree unavailable");
    assert!(!has_key(&rt, "jv-mode-pretty"), "pretty unavailable");
    assert!(!has_key(&rt, "jv-mode-minify"), "minify unavailable");
    assert!(has_key(&rt, "jv-text"), "original text body present");
    let has_original = texts(&rt).iter().any(|t| t == "   \n\t  ");
    assert!(has_original, "whitespace original visible");
}

#[test]
fn nested_six_levels_respects_default_depth() {
    // default_open = depth < 2: root(0) open, l1 node at depth 1 open, l2 node at depth 2 closed.
    let mut rt = Runtime::new(P, six_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "jv-node-root"), "root present");
    assert!(has_key(&rt, "jv-node-root.l1"), "level 1 present");
    assert!(has_key(&rt, "jv-node-root.l1.l2"), "level 2 node present (child of open depth 1)");
    assert!(!has_key(&rt, "jv-node-root.l1.l2.l3"), "level 3 hidden (depth 2 collapsed by default)");
}

#[test]
fn large_integer_and_float_render_without_panic_and_preserve_text_in_raw() {
    let mut rr = Runtime::new(P, big_raw_view, (960.0, 640.0), env());
    rr.frame(0.0);
    assert!(has_key(&rr, "jv-text"), "raw text body present");
    let raw_joined = texts(&rr).join("\n");
    assert!(raw_joined.contains("9007199254740993"), "raw preserves big int literal text: {raw_joined}");

    let mut rp = Runtime::new(P, big_pretty_view, (960.0, 640.0), env());
    rp.frame(0.0);
    assert!(has_key(&rp, "jv-text"), "pretty text body present");
    let pretty_joined = texts(&rp).join("\n");
    // serde_json may render large ints as number; observe representation (no panic required).
    assert!(
        pretty_joined.contains("9007199254740993")
            || pretty_joined.contains("9007199254740992")
            || pretty_joined.contains("9.00719925474099"),
        "pretty represents big int (serde_json formatting): {pretty_joined}"
    );
}

#[test]
fn long_single_line_json_200_chars_stays_in_window() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(P, long_pretty_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("json long line {w}x{h}"));
        assert!(has_key(&rt, "jv-text"));
        let body = element(&rt, "jv-text").rect;
        assert!(body[3] <= 360.0 + 1.0, "text body clamped at {w}x{h}: {body:?}");
    }
}

// ---- BinaryViewer boundaries -------------------------------------------------------------

static EMPTY: [u8; 0] = [];
static B32: [u8; 32] = [0x41; 32];
static NONPRINT_ROW: [u8; 16] = [0x00, 0x01, 0x1f, 0x20, 0x7f, 0x80, 0xff, 0x0a, 0x0d, 0x09, 0x1b, 0x02, 0x03, 0x04, 0x05, 0x06];
static B4K: [u8; 4096] = [0x5Au8; 4096];

fn empty_bin_view(_s: &P, _cx: &mut Cx<P>) -> El { BinaryViewer::new("bv", &EMPTY).build(true) }
fn b32_view(_s: &P, _cx: &mut Cx<P>) -> El { BinaryViewer::new("bv", &B32).build(true) }
fn nonprint_view(_s: &P, _cx: &mut Cx<P>) -> El { BinaryViewer::new("bv", &NONPRINT_ROW).build(true) }
fn b4k_view(_s: &P, _cx: &mut Cx<P>) -> El { BinaryViewer::new("bv", &B4K).build(true) }

#[test]
fn binary_zero_bytes_renders_zero_rows() {
    let mut rt = Runtime::new(P, empty_bin_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "bv-hex"), "hex body present");
    assert!(!has_key(&rt, "bv-line-0"), "no rows for 0 bytes");
    assert!(!has_key(&rt, "bv-hex-truncated"), "no truncation marker");
}

#[test]
fn binary_exact_multiple_of_16_has_complete_last_row() {
    let mut rt = Runtime::new(P, b32_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "bv-line-0"));
    assert!(has_key(&rt, "bv-line-1"));
    assert!(!has_key(&rt, "bv-line-2"), "exactly 2 rows for 32 bytes");
    assert_eq!(element(&rt, "bv-offset-1").text.as_deref(), Some("00000010"));
    let hex1 = element(&rt, "bv-hex-bytes-1").text.unwrap_or_default();
    assert!(
        hex1.ends_with("41"),
        "last byte of complete row present in hex string: {hex1:?}"
    );
    assert_eq!(hex1, "41 ".repeat(16).trim_end());
}

#[test]
fn binary_all_nonprintable_row_shows_dots_in_ascii() {
    let mut rt = Runtime::new(P, nonprint_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let ascii = element(&rt, "bv-ascii-0").text.unwrap_or_default();
    // printable only 0x21..0x7e, so 0x20(space),0x7f, controls, 0x80,0xff all become '.'
    assert_eq!(ascii, "................", "all non-printable -> dots (no pipes): {ascii:?}");
}

#[test]
fn binary_4kb_is_clamped_scrollable_not_truncated_and_stays_in_window_at_two_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(P, b4k_view, (w, h), env());
        rt.frame(0.0);
        // 4096/16 = 256 rows, below MAX_HEX_ROWS => not truncated.
        assert!(has_key(&rt, "bv-line-0"));
        assert!(has_key(&rt, "bv-line-255"), "last row of 4KB present");
        assert!(!has_key(&rt, "bv-line-256"), "no extra row");
        assert!(!has_key(&rt, "bv-hex-truncated"), "4KB must not be truncated");

        let root = element(&rt, "bv").rect;
        let body = element(&rt, "bv-hex").rect;
        assert!(
            root[0] >= -1.0 && root[1] >= -1.0 && root[0] + root[2] <= w + 1.0 && root[1] + root[3] <= h + 1.0,
            "root container must stay in window {w}x{h}: {root:?}"
        );
        assert!(
            body[0] >= -1.0 && body[1] >= -1.0 && body[0] + body[2] <= w + 1.0 && body[1] + body[3] <= h + 1.0,
            "hex body must stay in window {w}x{h}: {body:?}"
        );
        assert!(
            body[3] <= 360.0 + 1.0,
            "hex body must be clamped (max 360) at {w}x{h}: {body:?}"
        );
        let mut h_outside = Vec::new();
        for e in rt.elements() {
            let r = e.rect;
            if r[0] < -1.0 || r[0] + r[2] > w + 1.0 {
                h_outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
            }
        }
        assert!(
            h_outside.is_empty(),
            "4KB binary must not overflow horizontally at {w}x{h}: {h_outside:?}"
        );
    }
}

// ---- Invalid UTF-8 note (API boundary) ---------------------------------------------------
// JsonViewer::new takes &str (Rust UTF-8 string), so "非法 UTF-8" JSON source text cannot be
// represented through this API; binary arbitrary bytes are covered by BinaryViewer (&[u8]).
// This test documents parse failure behavior for obviously invalid JSON text: parse error,
// raw-only modes, and original text visible.
#[test]
fn invalid_utf8_not_representable_as_str_but_invalid_text_shows_parse_error_and_original() {
    // U+FFFD replacement-like text is valid UTF-8 string content but not valid JSON.
    fn v(_s: &P, _cx: &mut Cx<P>) -> El {
        JsonViewer::new("jv", "\u{FFFD}\u{FFFD}").build(true)
    }
    let mut rt = Runtime::new(P, v, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "jv-parse-error"));
    assert!(has_key(&rt, "jv-mode-bar"));
    assert!(has_key(&rt, "jv-mode-raw"));
    assert!(!has_key(&rt, "jv-mode-tree"));
    assert!(has_key(&rt, "jv-text"));
    let has_original = texts(&rt).iter().any(|t| t == "\u{FFFD}\u{FFFD}");
    assert!(has_original, "original invalid text must remain visible");
}
