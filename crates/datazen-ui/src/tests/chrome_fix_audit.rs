//! Independent re-audit for tabs fix a93526334 (selection state + panel label).

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, text};

use crate::ui::tabs::{TabItem, Tabs};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone, Default)]
struct S {
    active: String,
}

impl S {
    fn new() -> Self {
        Self { active: "a".to_owned() }
    }
}

fn items() -> Vec<TabItem> {
    vec![
        TabItem::new("a", "Alpha"),
        TabItem::new("b", "Bravo"),
        TabItem::new("c", "Charlie"),
    ]
}

fn view(s: &S, _cx: &mut Cx<S>) -> El {
    Tabs::new("tabs", items(), s.active.clone())
        .aria_label("Demo tabs")
        .panel(text(format!("panel:{}", s.active)).key("panel-content"))
        .on_change(|s: &mut S, id| s.active = id)
        .build(true)
}

fn find_node<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if v.get("key").and_then(|x| x.as_str()) == Some(key) {
        return Some(v);
    }
    if let Some(arr) = v.get("children").and_then(|x| x.as_array()) {
        for c in arr {
            if let Some(f) = find_node(c, key) {
                return Some(f);
            }
        }
    }
    None
}

fn states(v: &serde_json::Value) -> Vec<String> {
    v.get("states")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn is_selected(v: &serde_json::Value) -> bool {
    states(v).iter().any(|s| s == "selected")
}

fn is_unselected(v: &serde_json::Value) -> bool {
    // Rune a11y: inactive tabs may omit states or mark unselected; "not selected" is required.
    let st = states(v);
    st.iter().any(|s| s == "unselected") || !st.iter().any(|s| s == "selected")
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
}

#[test]
fn a11y_selected_states_flip_on_tab_switch() {
    let mut rt = Runtime::new(S::new(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let tree = rt.accessibility_json();
    let a = find_node(&tree, "tabs-tab-a").expect("tab a");
    let b = find_node(&tree, "tabs-tab-b").expect("tab b");
    let c = find_node(&tree, "tabs-tab-c").expect("tab c");
    assert!(is_selected(a), "a selected initially: states={:?}", states(a));
    assert!(is_unselected(b), "b unselected initially: states={:?}", states(b));
    assert!(is_unselected(c), "c unselected initially: states={:?}", states(c));

    click(&mut rt, "tabs-tab-b");
    let tree = rt.accessibility_json();
    let a = find_node(&tree, "tabs-tab-a").expect("tab a");
    let b = find_node(&tree, "tabs-tab-b").expect("tab b");
    let c = find_node(&tree, "tabs-tab-c").expect("tab c");
    assert!(is_unselected(a), "a unselected after switch: states={:?}", states(a));
    assert!(is_selected(b), "b selected after switch: states={:?}", states(b));
    assert!(is_unselected(c), "c unselected after switch: states={:?}", states(c));
}

#[test]
fn panel_label_matches_active_tab_and_updates() {
    let mut rt = Runtime::new(S::new(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let tree = rt.accessibility_json();
    let p = find_node(&tree, "tabs-panel").expect("panel");
    assert_eq!(p.get("name").and_then(|x| x.as_str()), Some("Alpha"), "panel name/label");

    click(&mut rt, "tabs-tab-b");
    let tree = rt.accessibility_json();
    let p = find_node(&tree, "tabs-panel").expect("panel");
    assert_eq!(p.get("name").and_then(|x| x.as_str()), Some("Bravo"));

    click(&mut rt, "tabs-tab-c");
    let tree = rt.accessibility_json();
    let p = find_node(&tree, "tabs-panel").expect("panel");
    assert_eq!(p.get("name").and_then(|x| x.as_str()), Some("Charlie"));
}

#[test]
fn roving_tabindex_and_keyboard_still_work() {
    let mut rt = Runtime::new(S::new(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    let tree = rt.accessibility_json();
    let a = find_node(&tree, "tabs-tab-a").unwrap();
    let b = find_node(&tree, "tabs-tab-b").unwrap();
    let c = find_node(&tree, "tabs-tab-c").unwrap();
    // Roving tabindex: only active has focus action/focusable semantics in a11y.
    let has_focus = |v: &serde_json::Value| {
        v.get("actions")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().any(|s| s.as_str() == Some("focus")))
            .unwrap_or(false)
    };
    assert!(has_focus(a), "active tab focusable");
    assert!(!has_focus(b), "inactive tab b not focusable");
    assert!(!has_focus(c), "inactive tab c not focusable");

    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "b");
    click(&mut rt, "tabs-tab-b");
    press(&mut rt, Key::End);
    assert_eq!(rt.state.active, "c");
    click(&mut rt, "tabs-tab-c");
    press(&mut rt, Key::Home);
    assert_eq!(rt.state.active, "a");
    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.active, "c");
}
