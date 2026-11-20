//! T202 independent audit (tester): fresh vectors — the Enter commit of a
//! typed value through the whole chain, three-row middle removal, double
//! add, a disabled remove, the `and` default display of the logic select,
//! and the short-drafts defence (rows render without value inputs).

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::table_data_connection_state::{FilterCondition, FilterLogic};
use crate::views::{SourceFilterEditor, SourceFilterSelect, StructuredSourceFilter, ValueDrafts};
use serde_json::json;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn condition(column: &str, operator: &str, value: Option<&str>) -> FilterCondition {
    FilterCondition {
        column: column.to_owned(),
        operator: operator.to_owned(),
        value: value.map(|v| serde_json::Value::String(v.to_owned())),
        connector: None,
    }
}

fn filter(conditions: Vec<FilterCondition>) -> Option<StructuredSourceFilter> {
    Some(StructuredSourceFilter {
        filters: conditions,
        logic: None,
        recordset: None,
    })
}

#[derive(Default)]
struct Scene {
    columns: Vec<String>,
    current: Option<StructuredSourceFilter>,
    disabled: bool,
    drafts: Vec<TextEdit>,
    applied: Vec<Option<StructuredSourceFilter>>,
    open: Option<SourceFilterSelect>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        SourceFilterEditor::<Scene>::new(scene.columns.clone(), scene.current.clone())
            .disabled(scene.disabled)
            .open(scene.open)
            .on_toggle(|s, slot| s.open = Some(slot))
            .on_close(|s| s.open = None)
            .on_change(|s, next| {
                s.current = next.clone();
                s.applied.push(next);
            })
            .value_drafts(ValueDrafts {
                edits: scene.drafts.clone(),
                bind: Rc::new(|s: &mut Scene, index| &mut s.drafts[index]),
            })
            .build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn scene(columns: &[&str], current: Option<StructuredSourceFilter>) -> Scene {
    let drafts = current
        .as_ref()
        .map(|f| f.filters.iter().map(|_| TextEdit::default()).collect())
        .unwrap_or_default();
    Scene {
        columns: columns.iter().map(|c| c.to_string()).collect(),
        current,
        drafts,
        dark: true,
        ..Scene::default()
    }
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn a_typed_value_commits_through_enter() {
    let mut rt = mount(
        scene(&["id", "name"], filter(vec![condition("id", "eq", Some("1"))])),
        (960.0, 640.0),
    );
    click(&mut rt, "data-transfer-source-filter-row-0-value");
    rt.paste("42");
    rt.settle_all();
    rt.frame(0.0);
    let typed = rt.state.drafts[0].text.clone();
    assert_eq!(typed, "42", "the paste reached the draft");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.applied.len(), 1, "Enter committed once");
    let applied = rt.state.applied[0].as_ref().expect("a filter came back");
    assert_eq!(applied.filters[0].value, Some(json!("42")));
    assert_eq!(applied.filters[0].column, "id");
    assert_eq!(applied.logic, Some(FilterLogic::And));
}

#[test]
fn a_disabled_editor_never_commits_a_typed_value() {
    let mut base = scene(&["id"], filter(vec![condition("id", "eq", Some("1"))]));
    base.disabled = true;
    let mut rt = mount(base, (960.0, 640.0));
    // The value field is present but disabled: pressing Enter with the
    // window focus still reaches no handler (the field swallows the key).
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.applied.is_empty(),
        "held inert while the transfer job prepares"
    );
    click(&mut rt, "data-transfer-source-filter-row-0-remove");
    assert!(rt.state.applied.is_empty(), "a disabled remove stays inert");
}

#[test]
fn the_middle_of_three_rows_is_removed_alone() {
    let mut rt = mount(
        scene(
            &["id", "name", "email"],
            filter(vec![
                condition("id", "eq", Some("1")),
                condition("name", "like", Some("a%")),
                condition("email", "isNull", None),
            ]),
        ),
        (960.0, 640.0),
    );
    click(&mut rt, "data-transfer-source-filter-row-1-remove");
    assert_eq!(rt.state.applied.len(), 1);
    let applied = rt.state.applied[0].as_ref().expect("a filter came back");
    assert_eq!(applied.filters.len(), 2);
    assert_eq!(applied.filters[0].column, "id");
    assert_eq!(applied.filters[1].column, "email", "the tail keeps its order");
    assert_eq!(applied.filters[1].operator, "isNull");
}

#[test]
fn two_adds_in_a_row_accumulate() {
    let mut rt = mount(scene(&["id", "name"], filter(vec![condition("id", "eq", Some("1"))])), (960.0, 640.0));
    click(&mut rt, "data-transfer-source-filter-add");
    click(&mut rt, "data-transfer-source-filter-add");
    assert_eq!(rt.state.applied.len(), 2);
    let second = rt.state.applied[1].as_ref().expect("the second add reports");
    assert_eq!(second.filters.len(), 3, "1 + two adds");
    assert_eq!(second.filters[1].column, "id");
    assert_eq!(second.filters[2].column, "id");
    // Both appended conditions default to eq/''.
    for appended in &second.filters[1..] {
        assert_eq!(appended.operator, "eq");
        assert_eq!(appended.value, Some(json!("")));
    }
}

#[test]
fn the_logic_select_defaults_to_all_conditions_and() {
    let rt = mount(
        scene(
            &["id", "name"],
            filter(vec![condition("id", "eq", Some("1")), condition("name", "like", Some("a"))]),
        ),
        (960.0, 640.0),
    );
    // `filter?.logic ?? 'and'`: with no logic field the trigger shows the
    // "all conditions" label.
    let trigger = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-transfer-source-filter-logic-trigger") && !e.exiting)
        .expect("the logic trigger is mounted");
    // The trigger is a button: its shown text lives on a text element
    // inside its rectangle.
    let [tx, ty, tw, th] = trigger.rect;
    let shown = rt
        .elements()
        .into_iter()
        .filter(|e| e.role == "text" && !e.exiting)
        .find(|e| {
            let [x, y, w, h] = e.rect;
            x >= tx - 0.5 && x + w <= tx + tw + 0.5 && y >= ty - 0.5 && y + h <= ty + th + 0.5
        })
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(shown.as_str(), crate::i18n::t("transfer.mapping.filterAll"));
}

#[test]
fn short_drafts_render_the_rows_without_panicking() {
    // The host's drafts are one short of the condition list: the rows
    // still mount, the first value input still mounts, the last row
    // simply carries none.
    let mut base = scene(
        &["id", "name"],
        filter(vec![condition("id", "eq", Some("1")), condition("name", "eq", Some("2"))]),
    );
    base.drafts.truncate(1);
    let rt = mount(base, (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-1"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-value"));
    assert!(
        !has_key(&rt, "data-transfer-source-filter-row-1-value"),
        "no draft, no value input — never an index panic"
    );
}
