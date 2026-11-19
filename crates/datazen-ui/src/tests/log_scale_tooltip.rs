//! T136: the `LogScaleTooltip` port — the `__orig__` formatter and the
//! `contentStyle` chrome.
//!
//! The formatter checks drive the pure function with programmable seams (the
//! payload view and `formatCompact` are injected; recording stubs assert what
//! React hands each arm, arm by arm). The chrome observations come from
//! `rt.elements()` — the paint channel (`fill`, `text_color`), `radius` and
//! the 12px token's line height — never from what the module reports about
//! itself. `ElementInfo` carries no border channel (the repo-wide precedent:
//! the toolbar's `border-b border-edge` is likewise pinned only by its doc),
//! so the border token decision lives in the module docs while the box's
//! fill/radius/ink/type-scale/geometry are what the tree pins here.

use std::cell::RefCell;

use rune::{col, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{ChartDataPoint, ChartValue};
use crate::components::chart::log_scale_tooltip::{
    chart_payload, format_entry, LogScaleTooltip, LogScaleTooltipEntry, CONTENT_KEY,
};
use crate::ui::kit::kit;

// ---- Harness ------------------------------------------------------------------

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// A `formatCompact` stub that echoes its argument back inside angle
/// brackets and records every call: assertions read both the string the arm
/// built and the exact number the arm handed the seam.
fn recording_compact(seen: &RefCell<Vec<f64>>) -> impl Fn(f64) -> String + '_ {
    move |n: f64| {
        seen.borrow_mut().push(n);
        format!("<{n}>")
    }
}

/// A payload view that records the keys it was asked for and always answers
/// with the same orig slot (`None` = the key holds no JS number).
fn asking_payload(
    asked: &RefCell<Vec<String>>,
    orig: Option<f64>,
) -> impl Fn(&str) -> Option<f64> + '_ {
    move |key: &str| {
        asked.borrow_mut().push(key.to_string());
        orig
    }
}

struct Probe {
    entries: Vec<LogScaleTooltipEntry>,
    dark: bool,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let tip = LogScaleTooltip::new(&s.entries).build(s.dark);
    col()
        .w_full()
        .h_full()
        .child(col().pin(Some(24.0), Some(24.0), None, None).child(tip))
}

fn chrome_runtime(
    entries: Vec<LogScaleTooltipEntry>,
    dark: bool,
    size: (f32, f32),
) -> Runtime<Probe> {
    let mut rt = Runtime::new(Probe { entries, dark }, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn entry(name: &str, value: &str) -> LogScaleTooltipEntry {
    LogScaleTooltipEntry {
        name: name.to_string(),
        value: value.to_string(),
    }
}

// ---- The formatter: the __orig__ arm -------------------------------------------

#[test]
fn a_positive_orig_shows_the_pretransform_number() {
    let seen = RefCell::new(Vec::new());
    let compact = recording_compact(&seen);
    // The payload view over a real family data point: __orig__count holds 1234.
    let point = ChartDataPoint::new().with("__orig__count", ChartValue::Number(1234.0));
    let formatted = format_entry(
        &ChartValue::Number(3.09), // the log10 value this arm must not use
        &ChartValue::Text("count".into()),
        &chart_payload(&point),
        &compact,
    );
    assert_eq!(formatted.value, "<1234>");
    assert_eq!(formatted.name, "count");
    assert_eq!(seen.borrow().len(), 1, "one formatCompact call");
    assert!(
        (seen.borrow()[0] - 1234.0).abs() < 1e-9,
        "the seam received the orig itself, not 10^logValue"
    );
}

#[test]
fn nonpositive_origs_render_the_literal_zero() {
    // orig <= 0 ? '0' : formatCompact(orig) — 0, -0 and the negatives render
    // the literal, and the seam is never called for them.
    for orig in [0.0, -0.0, -5.0] {
        let seen = RefCell::new(Vec::new());
        let compact = recording_compact(&seen);
        let point = ChartDataPoint::new().with("__orig__count", ChartValue::Number(orig));
        let formatted = format_entry(
            &ChartValue::Number(2.0),
            &ChartValue::Text("count".into()),
            &chart_payload(&point),
            &compact,
        );
        assert_eq!(
            formatted.value, "0",
            "orig {orig}: React renders the literal '0'"
        );
        assert!(
            seen.borrow().is_empty(),
            "orig {orig}: the '0' arm skips formatCompact entirely"
        );
    }
}

#[test]
fn a_nan_orig_is_a_number_and_reaches_the_seam_as_nan() {
    // typeof NaN === 'number' and NaN <= 0 is false in JS and Rust alike:
    // the arm hands the NaN to the seam untouched (React's formatCompact
    // renders 'NaN' via NaN.toFixed(2) — the real seam port's behaviour).
    let seen = RefCell::new(Vec::new());
    let compact = recording_compact(&seen);
    let point = ChartDataPoint::new().with("__orig__count", ChartValue::Number(f64::NAN));
    let formatted = format_entry(
        &ChartValue::Number(2.0),
        &ChartValue::Text("count".into()),
        &chart_payload(&point),
        &compact,
    );
    assert_eq!(formatted.value, "<NaN>");
    assert!(seen.borrow()[0].is_nan(), "the seam saw the NaN orig as-is");
}

#[test]
fn a_string_number_orig_is_not_a_number_and_falls_back() {
    // typeof '12' !== 'number': the payload view surfaces the slot as None
    // (the view's Some means exactly the typeof test), so the formatter takes
    // the fallback arm.
    let seen = RefCell::new(Vec::new());
    let compact = recording_compact(&seen);
    let point = ChartDataPoint::new().with("__orig__count", ChartValue::Text("12".into()));
    let formatted = format_entry(
        &ChartValue::Number(3.0),
        &ChartValue::Text("count".into()),
        &chart_payload(&point),
        &compact,
    );
    assert_eq!(formatted.value, "<1000>", "10^3, not the string '12'");
    assert!(
        (seen.borrow()[0] - 1000.0).abs() < 1e-9,
        "the fallback handed the seam 10^logValue"
    );
}

// ---- The formatter: the fallback arm --------------------------------------------

#[test]
fn the_fallback_arm_untransforms_the_log_value() {
    // value → 10^value; string numbers parse (Number trims whitespace and
    // parses the decimal grammar); null/undefined is `value ?? 0` → 10^0.
    let cases = [
        (&ChartValue::Number(3.0), 1000.0),
        (&ChartValue::Text("2".into()), 100.0),
        (&ChartValue::Text(" 2 ".into()), 100.0), // Number(' 2 ') === 2
        (&ChartValue::Null, 1.0),                 // Number(null ?? 0) === 0 → 10^0
    ];
    for (value, expected) in cases {
        let seen = RefCell::new(Vec::new());
        let compact = recording_compact(&seen);
        // No __orig__ slot at all: a payload the transform never touched.
        let point = ChartDataPoint::new().with("label", ChartValue::Text("a".into()));
        let formatted = format_entry(
            value,
            &ChartValue::Text("count".into()),
            &chart_payload(&point),
            &compact,
        );
        assert_eq!(
            formatted.value,
            format!("<{expected}>"),
            "the seam's echo came back as the display value"
        );
        assert!(
            (seen.borrow()[0] - expected).abs() < 1e-9,
            "the seam received 10^logValue, not logValue"
        );
    }
}

#[test]
fn a_nonnumeric_value_is_nan_all_the_way_through() {
    // Number('12abc') is NaN and 10^NaN is NaN: the seam sees the NaN (what
    // React's formatCompact would render as 'NaN'). A Number value holding
    // NaN is `typeof 'number'` and rides the same path unchanged.
    for value in [
        &ChartValue::Text("12abc".into()),
        &ChartValue::Number(f64::NAN),
    ] {
        let seen = RefCell::new(Vec::new());
        let compact = recording_compact(&seen);
        let point = ChartDataPoint::new();
        let formatted = format_entry(
            value,
            &ChartValue::Text("count".into()),
            &chart_payload(&point),
            &compact,
        );
        assert_eq!(formatted.value, "<NaN>");
        assert!(seen.borrow()[0].is_nan(), "10^NaN = NaN reached the seam");
    }
}

// ---- The formatter: the name and the __orig__ key ---------------------------------

#[test]
fn name_string_semantics_follow_react() {
    // String(name ?? ''): null → '', a string stays itself, a number takes JS
    // number-to-string — and the payload view is asked for __orig__ plus
    // exactly that name.
    let cases = [
        (&ChartValue::Null, ""),
        (&ChartValue::Text("sum".into()), "sum"),
        (&ChartValue::Number(42.0), "42"),
        (&ChartValue::Number(0.5), "0.5"),
        (&ChartValue::Number(-0.0), "0"), // JS String(-0) === '0'
        (&ChartValue::Number(f64::NAN), "NaN"),
        (&ChartValue::Number(f64::INFINITY), "Infinity"),
    ];
    for (name, name_str) in cases {
        let seen = RefCell::new(Vec::new());
        let compact = recording_compact(&seen);
        let asked = RefCell::new(Vec::new());
        let payload = asking_payload(&asked, None); // no orig slot anywhere
        let formatted = format_entry(&ChartValue::Number(2.0), name, &payload, &compact);
        assert_eq!(
            formatted.name, name_str,
            "String({name:?}) === {name_str:?}"
        );
        assert_eq!(
            *asked.borrow(),
            vec![format!("__orig__{name_str}")],
            "the payload view is asked for exactly __orig__ + the name string"
        );
    }
}

// ---- The chrome --------------------------------------------------------------------

#[test]
fn the_content_box_paints_reacts_chrome_tokens_in_both_themes() {
    let entries = vec![entry("count", "1.2K"), entry("sum", "34")];
    let expected_lines = ["count : 1.2K", "sum : 34"];
    for dark in [true, false] {
        let rt = chrome_runtime(entries.clone(), dark, (960.0, 640.0));
        let k = kit(dark);
        let tip = element(&rt, CONTENT_KEY);
        // background: var(--c-surface-alt, #111827) — the toolbar's
        // bg-surface-alt mapping.
        assert_eq!(
            tip.fill, k.c.layer_01,
            "dark={dark}: surface-alt paints layer_01"
        );
        // borderRadius: 6 — the kit's md radius token, whose built-in value
        // is React's literal 6.
        assert_eq!(
            tip.radius, k.radius.md,
            "dark={dark}: radius 6 is the md token"
        );
        assert!(
            (tip.radius - 6.0).abs() <= 0.5,
            "dark={dark}: the painted radius is React's 6"
        );
        // color: var(--c-fg, #eee) and fontSize: 12, line by line.
        for (index, expected) in expected_lines.iter().enumerate() {
            let line = element(&rt, &format!("{CONTENT_KEY}-item-{index}"));
            assert_eq!(
                line.text.as_deref(),
                Some(*expected),
                "dark={dark}: entry {index} renders name : value"
            );
            assert_eq!(
                line.text_color,
                Some(k.c.text_strong),
                "dark={dark}: var(--c-fg) is the strong foreground (the ChartView text-fg mapping)"
            );
            // fontSize: 12 — React's `text-xs`, the token the toolbar's
            // captions use; the 18pt line it lays out is the档 the tree
            // exposes, and it is not the 13pt档's 19.5pt line.
            assert!(
                (line.rect[3] - k.ty.text_xsmall.line_height).abs() <= 0.5,
                "dark={dark}: the 12px face lays out the xsmall token's line: {:?}",
                line.rect
            );
            assert!(
                (line.rect[3] - 18.0).abs() <= 0.5,
                "dark={dark}: fontSize 12 rides an 18pt line: {:?}",
                line.rect
            );
            assert!(
                line.rect[3] < k.ty.text_small.line_height - 0.5,
                "dark={dark}: the line is the 12px档, not the 13px档"
            );
        }
        // contentStyle sets no padding (recharts' own default-content padding
        // is the engine mount's concern): the lines sit flush at the box's
        // origin, stack, and the box hugs the widest line.
        let first = element(&rt, &format!("{CONTENT_KEY}-item-0"));
        let second = element(&rt, &format!("{CONTENT_KEY}-item-1"));
        assert!(
            (first.rect[0] - tip.rect[0]).abs() <= 0.5 && (first.rect[1] - tip.rect[1]).abs() <= 0.5,
            "dark={dark}: no padding: the first line starts at the box's origin: tip {:?} line {:?}",
            tip.rect,
            first.rect
        );
        assert!(
            (second.rect[1] - (first.rect[1] + first.rect[3])).abs() <= 0.5,
            "dark={dark}: the entries stack in payload order: {:?} {:?}",
            first.rect,
            second.rect
        );
        assert!(
            (tip.rect[3] - 36.0).abs() <= 0.5,
            "dark={dark}: two 18pt lines = a 36pt box: {:?}",
            tip.rect
        );
        assert!(
            (tip.rect[2] - first.rect[2]).abs() <= 0.5,
            "dark={dark}: the box hugs its widest line: {:?} {:?}",
            tip.rect,
            first.rect
        );
    }
}

#[test]
fn a_nameless_entry_renders_the_value_alone() {
    // recharts' default item skips the name and its ' : ' separator when the
    // name is empty, so the nameless entry paints the value alone.
    let rt = chrome_runtime(vec![entry("", "999")], true, (480.0, 240.0));
    let line = element(&rt, &format!("{CONTENT_KEY}-item-0"));
    assert_eq!(line.text.as_deref(), Some("999"));
    let tip = element(&rt, CONTENT_KEY);
    assert!(
        (tip.rect[3] - 18.0).abs() <= 0.5,
        "one nameless 18pt line: {:?}",
        tip.rect
    );
}
