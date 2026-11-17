use std::collections::BTreeSet;

use rune::{Cx, El, Env, Runtime};

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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
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
struct Probe {
    mode: JsonDisplayMode,
    toggled: BTreeSet<String>,
}

const RAW_SAMPLE: &str = "  { \"a\" :  1 } \n";
const FLAT_SRC: &str = r#"{"a":{"b":1}}"#;
const INVALID_SRC: &str = "{not json";
const SIMPLE_SRC: &str = r#"{"a":1}"#;
const NESTED: &str = r#"{"a":{"b":{"c":1}},"x":2}"#;

fn raw_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", RAW_SAMPLE)
        .mode(JsonDisplayMode::Raw)
        .build(true)
}

fn pretty_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", FLAT_SRC)
        .mode(JsonDisplayMode::Pretty)
        .build(true)
}

fn minify_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", FLAT_SRC)
        .mode(JsonDisplayMode::Minify)
        .build(true)
}

fn nested_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", NESTED)
        .mode(JsonDisplayMode::Tree)
        .toggled(state.toggled.iter().cloned())
        .on_mode(|st: &mut Probe, m| st.mode = m)
        .on_toggle(|st: &mut Probe, path| {
            if !st.toggled.insert(path.clone()) {
                st.toggled.remove(&path);
            }
        })
        .build(true)
}

fn invalid_view(_s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", INVALID_SRC).build(true)
}

fn modes_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", SIMPLE_SRC)
        .mode(s.mode)
        .on_mode(|st: &mut Probe, m| st.mode = m)
        .build(true)
}

fn size_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    JsonViewer::new("jv", NESTED)
        .mode(JsonDisplayMode::Tree)
        .toggled(s.toggled.iter().cloned())
        .build(true)
}

#[test]
fn raw_mode_text_is_identical_to_input() {
    let mut rt = Runtime::new(Probe::default(), raw_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let found = rt
        .elements()
        .into_iter()
        .any(|e| e.text.as_deref() == Some(RAW_SAMPLE));
    assert!(found, "raw text must be present exactly as input");
    assert!(rt.find("jv-text").is_some(), "text body present");
}

#[test]
fn pretty_indents_each_level_by_two_spaces_and_minify_has_no_newline() {
    let mut prt = Runtime::new(Probe::default(), pretty_view, (960.0, 640.0), env());
    prt.frame(0.0);
    let ptexts: Vec<_> = prt.elements().into_iter().filter_map(|e| e.text).collect();
    let pretty = ptexts
        .iter()
        .find(|t| t.contains('\n') && t.contains("\"a\""))
        .cloned()
        .unwrap_or_else(|| panic!("pretty text not found in {ptexts:?}"));
    assert!(pretty.contains("\n  \"a\""), "level 1 indented 2: {pretty:?}");
    assert!(pretty.contains("\n    \"b\""), "level 2 indented 4: {pretty:?}");

    let mut mrt = Runtime::new(Probe::default(), minify_view, (960.0, 640.0), env());
    mrt.frame(0.0);
    let mtexts: Vec<_> = mrt.elements().into_iter().filter_map(|e| e.text).collect();
    let mini = mtexts
        .iter()
        .find(|t| t.contains("\"a\"") && t.contains("\"b\"") && !t.contains('\n'))
        .cloned()
        .unwrap_or_else(|| panic!("minify text not found in {mtexts:?}"));
    assert!(!mini.contains('\n'), "minify must have no newline: {mini:?}");
}

#[test]
fn tree_default_shows_root_and_first_level_and_hides_deep_level_and_toggle_works() {
    let mut rt = Runtime::new(Probe::default(), nested_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("jv-node-root").is_some(), "root present");
    assert!(rt.find("jv-node-root.a").is_some(), "first-level child present");
    // depth 3 node (child of depth-2 collapsed node root.a.b) must not appear
    assert!(
        rt.find("jv-node-root.a.b.c").is_none(),
        "3rd/deep level must not appear by default"
    );

    let before: Vec<_> = rt
        .elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with("jv-node-"))
        .collect();
    // Toggle root: collapse hides its descendants (not generated).
    click(&mut rt, "jv-toggle-root");
    rt.frame(1.0 / 60.0);
    assert!(
        rt.find("jv-node-root.a").is_none(),
        "after collapsing root, children disappear"
    );
    let after: Vec<_> = rt
        .elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with("jv-node-") && rt.find(k).is_some())
        .collect();
    assert!(
        after.len() < before.len(),
        "collapsed descendants not generated: before={before:?} after={after:?}"
    );
    // Toggle root again: children reappear.
    click(&mut rt, "jv-toggle-root");
    rt.frame(1.0 / 60.0);
    assert!(
        rt.find("jv-node-root.a").is_some(),
        "after expanding root, children reappear"
    );
}

#[test]
fn unparseable_text_shows_parse_error_mode_bar_and_raw_text() {
    let mut rt = Runtime::new(Probe::default(), invalid_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("jv-parse-error").is_some(), "parse-error marker present");
    assert!(rt.find("jv-mode-bar").is_some(), "mode bar present");
    assert!(rt.find("jv-mode-raw").is_some(), "raw mode available");
    assert!(rt.find("jv-mode-tree").is_none(), "tree mode not available on parse error");
    assert!(rt.find("jv-mode-pretty").is_none(), "pretty mode not available on parse error");
    assert!(rt.find("jv-mode-minify").is_none(), "minify mode not available on parse error");
    assert!(rt.find("jv-text").is_some(), "raw text body present");
    let has_original = rt
        .elements()
        .into_iter()
        .any(|e| e.text.as_deref() == Some(INVALID_SRC));
    assert!(has_original, "original unparseable text must remain visible");
}

#[test]
fn mode_keys_exist_and_are_focusable() {
    let mut rt = Runtime::new(Probe::default(), modes_view, (960.0, 640.0), env());
    rt.frame(0.0);
    for id in ["tree", "raw", "pretty", "minify"] {
        let key = format!("jv-mode-{id}");
        let e = element(&rt, &key);
        assert!(e.focusable, "{key} focusable");
    }
    assert!(rt.find("jv-mode-bar").is_some(), "mode bar present");
}

#[test]
fn json_viewer_stays_in_window_at_two_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(Probe::default(), size_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("json {w}x{h}"));
        let root = element(&rt, "jv");
        eprintln!("json {w}x{h} root rect={:?}", root.rect);
    }
}
