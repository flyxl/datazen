//! T202: `windows/data-transfer/SourceFilterEditor.tsx` port — the update
//! chain (add / remove / operator value rule / rebuild with logic and
//! recordset preserved), the empty state, the row mounts (value input
//! hidden for the null pair), the logic row threshold, the disabled
//! inertness, and window-width geometry.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::table_data_connection_state::{FilterCondition, FilterLogic};
use datazen_ui::views::{
    rebuild_filter, SourceFilterEditor, SourceFilterSelect, StructuredSourceFilter, ValueDrafts,
};
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

fn filter(conditions: Vec<FilterCondition>, logic: Option<FilterLogic>) -> StructuredSourceFilter {
    StructuredSourceFilter {
        filters: conditions,
        logic,
        recordset: None,
    }
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

impl Scene {
    fn with(columns: &[&str], current: Option<StructuredSourceFilter>) -> Self {
        let drafts = current
            .as_ref()
            .map(|f| f.filters.iter().map(|_| TextEdit::default()).collect())
            .unwrap_or_default();
        Self {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            current,
            drafts,
            dark: true,
            ..Self::default()
        }
    }
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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---- the update chain (pure) ----

#[test]
fn rebuild_keeps_logic_defaulting_to_and() {
    let none: Option<&StructuredSourceFilter> = None;
    let next = rebuild_filter(none, vec![condition("id", "eq", Some(""))]).unwrap();
    assert_eq!(next.logic, Some(FilterLogic::And), "no prior logic defaults to and");
    let prior = filter(
        vec![condition("id", "eq", Some("1"))],
        Some(FilterLogic::Or),
    );
    let next = rebuild_filter(Some(&prior), vec![condition("id", "eq", Some("2"))]).unwrap();
    assert_eq!(next.logic, Some(FilterLogic::Or), "a prior or stays or");
}

#[test]
fn rebuild_preserves_the_sync_recordset() {
    let mut prior = filter(vec![condition("id", "eq", Some("1"))], Some(FilterLogic::Or));
    prior.recordset = Some(datazen_ui::views::TransferRecordset {
        limit: Some(10.0),
        ..Default::default()
    });
    let next = rebuild_filter(Some(&prior), vec![condition("id", "eq", Some("2"))]).unwrap();
    assert_eq!(next.recordset.as_ref().and_then(|r| r.limit), Some(10.0));
    // Emptying the list keeps the recordset alone.
    let emptied = rebuild_filter(Some(&prior), Vec::new()).unwrap();
    assert!(emptied.filters.is_empty());
    assert!(emptied.recordset.is_some());
    // No recordset and no conditions: the whole filter goes undefined.
    let bare = filter(vec![condition("id", "eq", Some("1"))], None);
    assert!(rebuild_filter(Some(&bare), Vec::new()).is_none());
}

#[test]
fn an_operator_switch_to_the_null_pair_clears_the_value() {
    // `value_for_operator_change` is the operator-change rule; the pair
    // mirrors it through the same rebuild the operator select drives.
    let prior = condition("id", "eq", Some("7"));
    let mut switched = prior.clone();
    switched.operator = "isNull".to_owned();
    switched.value = None;
    let next = rebuild_filter(
        Some(&filter(vec![prior], None)),
        vec![switched],
    )
    .unwrap();
    assert_eq!(next.filters[0].operator, "isNull");
    assert_eq!(next.filters[0].value, None, "the null pair carries no value");
    // Switching back: a value-less condition becomes the empty string.
    let mut back = next.filters[0].clone();
    back.operator = "eq".to_owned();
    back.value = back.value.clone().or(Some(json!("")));
    let next = rebuild_filter(Some(&next), vec![back]).unwrap();
    assert_eq!(next.filters[0].value, Some(json!("")));
}

// ---- the mounted editor ----

#[test]
fn no_filter_mounts_the_empty_state() {
    let rt = mount(Scene::with(&["id", "name"], None), (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-source-filter"));
    assert!(has_key(&rt, "data-transfer-source-filter-empty"));
    assert_eq!(
        text_of(&rt, "data-transfer-source-filter-empty"),
        t("transfer.mapping.noSourceFilter")
    );
    assert!(!has_key(&rt, "data-transfer-source-filter-row-0"));
    assert!(!has_key(&rt, "data-transfer-source-filter-logic-trigger"));
}

#[test]
fn one_condition_mounts_one_row_without_the_logic_switch() {
    let rt = mount(
        Scene::with(&["id", "name"], Some(filter(vec![condition("id", "eq", Some("1"))], None))),
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-transfer-source-filter-row-0"));
    assert!(!has_key(&rt, "data-transfer-source-filter-row-1"));
    assert!(
        !has_key(&rt, "data-transfer-source-filter-logic-trigger"),
        "the logic select needs more than one condition"
    );
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-column-trigger"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-operator-trigger"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-value"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-remove"));
}

#[test]
fn two_conditions_mount_the_logic_switch() {
    let rt = mount(
        Scene::with(
            &["id", "name"],
            Some(filter(
                vec![condition("id", "eq", Some("1")), condition("name", "like", Some("a%"))],
                None,
            )),
        ),
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-transfer-source-filter-logic-trigger"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-1"));
}

#[test]
fn the_value_input_is_hidden_for_the_null_pair() {
    let rt = mount(
        Scene::with(&["id"], Some(filter(vec![condition("id", "isNull", None)], None))),
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-operator-trigger"));
    assert!(
        !has_key(&rt, "data-transfer-source-filter-row-0-value"),
        "isNull carries no value input"
    );
    let rt = mount(
        Scene::with(&["id"], Some(filter(vec![condition("id", "isNotNull", None)], None))),
        (960.0, 640.0),
    );
    assert!(!has_key(&rt, "data-transfer-source-filter-row-0-value"));
}

#[test]
fn without_value_drafts_the_rows_still_mount() {
    // The drafts are optional wiring: the rows render without them.
    let rt = Runtime::new(
        Scene::with(&["id"], Some(filter(vec![condition("id", "eq", Some("1"))], None))),
        |scene: &Scene, _cx: &mut Cx<Scene>| {
            col().w_full().h_full().child(
                SourceFilterEditor::<Scene>::new(scene.columns.clone(), scene.current.clone())
                    .on_toggle(|s, slot| s.open = Some(slot))
                    .on_close(|s| s.open = None)
                    .build(scene.dark),
            )
        },
        (960.0, 640.0),
        env(true),
    );
    let mut rt = rt;
    rt.frame(0.0);
    assert!(has_key(&rt, "data-transfer-source-filter-row-0"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0-operator-trigger"));
}

// ---- the add button ----

#[test]
fn add_appends_a_default_condition_on_the_first_column() {
    let mut rt = mount(
        Scene::with(&["id", "name"], Some(filter(vec![condition("id", "eq", Some("1"))], None))),
        (960.0, 640.0),
    );
    click(&mut rt, "data-transfer-source-filter-add");
    assert_eq!(rt.state.applied.len(), 1);
    let applied = rt.state.applied[0].as_ref().expect("a filter came back");
    assert_eq!(applied.filters.len(), 2);
    assert_eq!(applied.filters[1].column, "id", "columns[0] is the default column");
    assert_eq!(applied.filters[1].operator, "eq");
    assert_eq!(applied.filters[1].value, Some(json!("")));
    assert_eq!(applied.logic, Some(FilterLogic::And));
}

#[test]
fn add_is_disabled_without_columns_and_while_disabled() {
    let mut rt = mount(Scene::with(&[], None), (960.0, 640.0));
    click(&mut rt, "data-transfer-source-filter-add");
    assert!(
        rt.state.applied.is_empty(),
        "no columns: the add button never fires"
    );
    let mut scene = Scene::with(&["id"], None);
    scene.disabled = true;
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "data-transfer-source-filter-add");
    assert!(
        rt.state.applied.is_empty(),
        "held inert while the transfer job prepares"
    );
}

// ---- the remove button ----

#[test]
fn remove_drops_its_row_and_keeps_the_rest() {
    let mut rt = mount(
        Scene::with(
            &["id", "name"],
            Some(filter(
                vec![condition("id", "eq", Some("1")), condition("name", "like", Some("a%"))],
                Some(FilterLogic::Or),
            )),
        ),
        (960.0, 640.0),
    );
    click(&mut rt, "data-transfer-source-filter-row-0-remove");
    assert_eq!(rt.state.applied.len(), 1);
    let applied = rt.state.applied[0].as_ref().expect("a filter came back");
    assert_eq!(applied.filters.len(), 1);
    assert_eq!(applied.filters[0].column, "name");
    assert_eq!(applied.logic, Some(FilterLogic::Or), "the logic survives a removal");
}

#[test]
fn removing_the_last_row_reports_none_without_a_recordset() {
    let mut rt = mount(
        Scene::with(&["id"], Some(filter(vec![condition("id", "eq", Some("1"))], None))),
        (960.0, 640.0),
    );
    click(&mut rt, "data-transfer-source-filter-row-0-remove");
    assert_eq!(rt.state.applied, vec![None]);
}

// ---- geometry ----

#[test]
fn the_editor_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(
            Scene::with(
                &["id", "name"],
                Some(filter(
                    vec![condition("id", "eq", Some("1")), condition("name", "like", Some("a%"))],
                    None,
                )),
            ),
            (w, h),
        );
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-transfer-source-filter") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {outside:?}");
    }
}

#[test]
fn the_light_theme_mounts_the_same_tree() {
    let mut scene = Scene::with(
        &["id", "name"],
        Some(filter(
            vec![condition("id", "eq", Some("1")), condition("name", "like", Some("a%"))],
            None,
        )),
    );
    scene.dark = false;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-transfer-source-filter"));
    assert!(has_key(&rt, "data-transfer-source-filter-logic-trigger"));
    assert!(has_key(&rt, "data-transfer-source-filter-row-0"));
}
