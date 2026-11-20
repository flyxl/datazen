//! T014: the `FilterBar` port.
//!
//! Observations come from `rt.elements()` (keys, text, geometry, paint) and
//! from the callbacks the probe records — never from a self-report by the
//! component. The probe rebuilds the bar from its state each frame, the way a
//! query view would, so a press drives the next frame like a real view.

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use crate::components::filter_bar::{
    badge_key, label_for, remove_key, summary_key, FilterBar, FilterCondition, CLEAR_KEY,
    FILTERS_KEY, LABEL_KEY, ROOT_KEY,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Probe {
    filters: Vec<FilterCondition>,
    loading: bool,
    dark: bool,
    removed: Vec<usize>,
    cleared: u32,
}

fn conditions() -> Vec<FilterCondition> {
    vec![
        FilterCondition::new("name", "eq", Some(json!("alice"))),
        FilterCondition::new("age", "between", Some(json!([18, 30]))),
    ]
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut host = col().key("host").w_full().h_full();
    if let Some(bar) = FilterBar::new(s.filters.clone())
        .loading(s.loading)
        .on_remove(|state: &mut Probe, idx: usize| state.removed.push(idx))
        .on_clear(|state: &mut Probe| state.cleared += 1)
        .build(s.dark)
    {
        host = host.child(bar);
    }
    host
}

fn runtime(filters: Vec<FilterCondition>, loading: bool, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe { filters, loading, dark, removed: Vec::new(), cleared: 0 },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
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
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: out of window: {outside:?}");
}

#[test]
fn an_empty_list_renders_nothing_at_all() {
    assert!(FilterBar::<Probe>::new(Vec::new()).build(true).is_none());
    let rt = runtime(Vec::new(), false, DESIGN, true);
    let keys = vec![
        ROOT_KEY.to_owned(),
        LABEL_KEY.to_owned(),
        FILTERS_KEY.to_owned(),
        CLEAR_KEY.to_owned(),
        badge_key(0),
    ];
    for key in &keys {
        assert!(!has_key(&rt, key), "{key} must not mount for an empty list");
    }
}

#[test]
fn label_for_matches_react_column_operator_and_formatted_value() {
    assert_eq!(label_for(&FilterCondition::new("name", "eq", None)), "name eq");
    assert_eq!(
        label_for(&FilterCondition::new("name", "eq", Some(json!("alice")))),
        "name eq alice"
    );
    assert_eq!(
        label_for(&FilterCondition::new("deleted_at", "isNull", Some(json!(null)))),
        "deleted_at isNull NULL"
    );
    assert_eq!(
        label_for(&FilterCondition::new("age", "between", Some(json!([18, 30])))),
        "age between [18,30]"
    );
    assert_eq!(
        label_for(&FilterCondition::new("total", "gte", Some(json!(3.5)))),
        "total gte 3.5"
    );
}

#[test]
fn every_chip_shows_its_own_summary_and_the_label_and_clear_copy_is_translated() {
    let rt = runtime(conditions(), false, DESIGN, true);
    assert_eq!(element(&rt, LABEL_KEY).text.as_deref(), Some(t("filter.filter")));
    assert_eq!(element(&rt, CLEAR_KEY).text.as_deref(), Some(t("filter.clear")));
    let filters = conditions();
    for (idx, f) in filters.iter().enumerate() {
        let key = summary_key(idx);
        assert_eq!(
            element(&rt, &key).text.as_deref(),
            Some(label_for(f).as_str()),
            "chip {idx} shows its own formatted summary"
        );
    }
    let accent = kit(true).c.text_accent;
    assert_eq!(
        element(&rt, &badge_key(0)).fill,
        kit(true).alpha(accent, 0.10),
        "React Badge tone=accent's 10% wash"
    );
}

#[test]
fn pressing_a_chips_x_reports_that_chips_index() {
    let mut rt = runtime(conditions(), false, DESIGN, true);
    assert!(rt.find(&remove_key(0)).is_some());
    click(&mut rt, &remove_key(1));
    assert_eq!(rt.state.removed, vec![1], "the second chip's X reports index 1");
    click(&mut rt, &remove_key(0));
    assert_eq!(rt.state.removed, vec![1, 0], "the first chip's X reports index 0");
    assert_eq!(rt.state.cleared, 0);
}

#[test]
fn pressing_clear_reports_once() {
    let mut rt = runtime(conditions(), false, DESIGN, true);
    click(&mut rt, CLEAR_KEY);
    assert_eq!(rt.state.cleared, 1);
    assert!(rt.state.removed.is_empty(), "clear does not remove a single chip");
}

#[test]
fn loading_disables_remove_and_clear_without_dispatching() {
    let mut rt = runtime(conditions(), true, DESIGN, true);
    assert!(!element(&rt, &remove_key(0)).focusable, "a loading X is not focusable");
    click(&mut rt, &remove_key(0));
    click(&mut rt, CLEAR_KEY);
    assert!(rt.state.removed.is_empty(), "no removal while loading");
    assert_eq!(rt.state.cleared, 0, "no clear while loading");
}

#[test]
fn the_bar_stays_inside_both_window_sizes_and_both_themes() {
    for (size, dark) in [(MIN, true), (MIN, false), (DESIGN, true), (DESIGN, false)] {
        let rt = runtime(conditions(), false, size, dark);
        assert_in_window(&rt, size.0, size.1, &format!("{size:?} dark={dark}"));
        assert!(has_key(&rt, ROOT_KEY), "the bar is mounted");
        let root = element(&rt, ROOT_KEY).rect;
        assert!(root[2] > 0.0 && root[3] > 0.0, "the bar has a real box");
    }
}
