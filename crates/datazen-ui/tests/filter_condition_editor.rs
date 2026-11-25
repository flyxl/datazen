//! T056: `components/filter/FilterConditionEditor.tsx` ported to Rune.
//!
//! The probes below mount one expanded condition row the way the filter panel
//! will, and observe it through the runtime tree (keys, text, element rects),
//! never through a value the component writes into state for the test to read.
//! Coverage: the type ∩ driver operator list with the out-of-list current
//! operator kept, value/operator conversions, value-kind dispatch, remove and
//! collapse, and the non-wrapping row inside both window sizes and themes.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::components::filter::{
    EditorSelect, FilterColumn, FilterCondition, FilterConditionEditor, FilterValue,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

/// The host state around one editor row: the draft, the driver rules, the four
/// text drafts, and the actions the row reports upward.
struct Probe {
    filter: FilterCondition,
    index: usize,
    columns: Vec<FilterColumn>,
    supported: Vec<&'static str>,
    like_case_insensitive: bool,
    disabled: bool,
    open: Option<EditorSelect>,
    single: TextEdit,
    tokens: TextEdit,
    lower: TextEdit,
    upper: TextEdit,
    changes: Vec<FilterCondition>,
    removes: Vec<usize>,
    collapses: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Probe {
            filter: FilterCondition::new("name", "eq", FilterValue::Single("alice".into())),
            index: 2,
            columns: vec![
                FilterColumn::new("name", Some("varchar(64)")),
                FilterColumn::new("age", Some("int4")),
            ],
            supported: vec!["eq", "ne", "like", "between", "isNull", "in"],
            like_case_insensitive: false,
            disabled: false,
            open: None,
            single: TextEdit::new("alice"),
            tokens: TextEdit::new(""),
            lower: TextEdit::new("18"),
            upper: TextEdit::new("30"),
            changes: Vec::new(),
            removes: Vec::new(),
            collapses: 0,
        }
    }
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let row = FilterConditionEditor::new("edge", state.index, &state.filter, &state.columns)
        .supported_operators(&state.supported)
        .like_case_insensitive(state.like_case_insensitive)
        .disabled(state.disabled)
        .open(state.open)
        .single_draft(&state.single, |s: &mut Probe| &mut s.single)
        .token_draft(&state.tokens, |s: &mut Probe| &mut s.tokens)
        .between_drafts(
            &state.lower,
            &state.upper,
            |s: &mut Probe| &mut s.lower,
            |s: &mut Probe| &mut s.upper,
        )
        .current(|s: &Probe| &s.filter)
        .on_change(|s: &mut Probe, _index, next| {
            s.changes.push(next.clone());
            s.filter = next;
            s.open = None; // React's Select closes on a choice.
        })
        .on_remove(|s: &mut Probe, index| s.removes.push(index))
        .on_collapse(|s: &mut Probe| s.collapses += 1)
        .on_toggle(|s: &mut Probe, which| {
            s.open = if s.open == Some(which) { None } else { Some(which) };
        })
        .on_close(|s: &mut Probe| s.open = None)
        .build(dark, &mut layer);
    rune_kit::root(col().w_full().h_full().padding(PAD).child(row), layer)
}

fn runtime(state: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env(dark));
    rt.frame(0.0);
    rt
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    element(rt, key).text
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

// ---- 1. the operator list is type ∩ driver, plus the current out-of-list id --------

#[test]
fn operator_list_is_the_type_and_driver_intersection_with_the_current_operator_kept() {
    let mut rt = runtime(
        Probe {
            filter: FilterCondition::new("age", "like", FilterValue::Single("ann".into())),
            // A numeric column offers eq / between / isNull from this driver;
            // `like` is not in the intersection and must be appended.
            supported: vec!["eq", "between", "like", "isNull"],
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "edge-operator-trigger");

    let labels: Vec<Option<String>> = (0..4)
        .map(|i| element(&rt, &format!("option{i}")).label)
        .collect();
    assert_eq!(
        labels,
        vec![
            Some(t("filter.eq").to_owned()),
            Some(t("filter.between").to_owned()),
            Some(t("filter.isNull").to_owned()),
            Some(t("filter.like").to_owned()),
        ],
        "exactly the number∩driver list, then the draft's out-of-list operator"
    );
    assert!(
        rt.find("option4").is_none(),
        "a fifth choice would violate the intersection"
    );
    assert!(
        element(&rt, "option3").item_selected,
        "the kept `like` choice is the current one"
    );
}

#[test]
fn a_current_operator_inside_the_intersection_is_not_duplicated() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    click(&mut rt, "edge-operator-trigger");
    let labels: Vec<String> = (0..8)
        .filter_map(|i| rt.find(&format!("option{i}")).map(|_| {
            element(&rt, &format!("option{i}"))
                .label
                .unwrap_or_default()
        }))
        .collect();
    let eq = t("filter.eq");
    assert_eq!(
        labels.iter().filter(|label| label.as_str() == eq).count(),
        1,
        "`eq` is in the list already, so React does not push it again"
    );
    assert_eq!(
        labels.first().map(String::as_str),
        Some(eq),
        "the shared order is kept"
    );
}

// ---- 2. changeOperator / changeColumn convert values and normalise operators --------

#[test]
fn change_operator_converts_a_single_value_to_a_pair_and_keeps_editing() {
    let mut rt = runtime(
        Probe {
            filter: FilterCondition::new("age", "like", FilterValue::Single("alice".into())),
            supported: vec!["eq", "between", "like", "isNull"],
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "edge-operator-trigger");
    // eq, between, isNull, like — option1 is `between`.
    click(&mut rt, "option1");

    assert_eq!(rt.state.changes.len(), 1, "one report to the parent");
    let changed = &rt.state.changes[0];
    assert_eq!(changed.operator, "between");
    assert_eq!(
        changed.value,
        FilterValue::Pair("alice".into(), String::new()),
        "single → pair keeps the literal as the lower bound"
    );
    assert_eq!(rt.state.filter.operator, "between", "the draft is updated");
    assert_eq!(
        rt.state.collapses, 0,
        "`between` still has a value to edit, so the row stays expanded"
    );
}

#[test]
fn choosing_a_value_less_operator_converts_nothing_and_collapses_immediately() {
    let mut rt = runtime(
        Probe {
            filter: FilterCondition::new("age", "like", FilterValue::Single("alice".into())),
            supported: vec!["eq", "between", "like", "isNull"],
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "edge-operator-trigger");
    // After choosing between, the list is eq / between / isNull (no out-of-list push).
    click(&mut rt, "option1");
    click(&mut rt, "edge-operator-trigger");
    click(&mut rt, "option2"); // isNull

    let last = rt.state.changes.last().expect("a change was reported");
    assert_eq!(last.operator, "isNull");
    assert_eq!(
        last.value,
        FilterValue::Pair("alice".into(), String::new()),
        "none keeps the old value, so switching back restores it"
    );
    assert_eq!(
        rt.state.collapses, 1,
        "the synchronous none-operator collapse fires once"
    );
}

#[test]
fn change_column_normalises_the_operator_and_converts_the_value() {
    let mut rt = runtime(
        Probe {
            filter: FilterCondition::new(
                "age",
                "between",
                FilterValue::Pair("18".into(), "30".into()),
            ),
            supported: vec!["eq", "between", "like", "isNull"],
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "edge-column-trigger");
    click(&mut rt, "option0"); // `name`, a text column

    assert_eq!(rt.state.changes.len(), 1);
    let changed = &rt.state.changes[0];
    assert_eq!(changed.column, "name");
    assert_eq!(
        changed.operator, "eq",
        "`between` is not a text operator, so it normalises to `eq`"
    );
    assert_eq!(
        changed.value,
        FilterValue::Single("18".into()),
        "pair → single keeps the lower bound"
    );
    assert_eq!(
        rt.state.collapses, 0,
        "switching columns alone never collapses the row"
    );
}

// ---- 3. value-kind dispatch and the LIKE hint -------------------------------------

#[test]
fn single_operator_renders_the_value_field_and_only_like_shows_the_hint() {
    let rt = runtime(Probe::default(), DESIGN, true);
    assert!(rt.find("edge-value").is_some(), "single → value field");
    assert!(rt.find("edge-tokens").is_none());
    assert!(rt.find("edge-between").is_none());
    assert!(
        rt.find("edge-like-case-insensitive").is_none(),
        "the hint is gated on both the flag and the `like` operator"
    );

    let mut hinted = runtime(
        Probe {
            filter: FilterCondition::new("name", "like", FilterValue::Single("%a%".into())),
            like_case_insensitive: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert_eq!(
        element_text(&hinted, "edge-like-case-insensitive").as_deref(),
        Some(t("filter.likeCaseInsensitiveHint")),
        "flag + `like` shows the exact React key"
    );
    // The flag alone is not enough: an `eq` condition must stay clean.
    let not_like = runtime(
        Probe {
            like_case_insensitive: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(not_like.find("edge-like-case-insensitive").is_none());
    assert!(hinted.find("edge-value").is_some());
}

#[test]
fn tokens_operator_renders_the_token_list_with_its_remove_controls() {
    let rt = runtime(
        Probe {
            filter: FilterCondition::new(
                "status",
                "in",
                FilterValue::Tokens(vec!["paid".into(), "sent".into()]),
            ),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(rt.find("edge-tokens").is_some(), "tokens → token list");
    assert!(rt.find("edge-token-field").is_some(), "pending-token field");
    assert!(rt.find("edge-token-0").is_some());
    assert!(rt.find("edge-token-1").is_some());
    assert!(rt.find("edge-token-2").is_none(), "two values, two chips");
    assert!(rt.find("edge-token-0-remove").is_some());
    assert!(rt.find("edge-token-1-remove").is_some());
    let texts: Vec<String> = rt
        .elements()
        .into_iter()
        .filter_map(|e| e.text)
        .collect();
    assert!(
        texts.iter().any(|text| text == "paid") && texts.iter().any(|text| text == "sent"),
        "both token values are drawn: {texts:?}"
    );
    assert_eq!(
        element(&rt, "edge-token-0-remove").help.as_deref(),
        Some(t("filter.tokenRemove")),
        "the chip's remove control carries the exact React label"
    );
}

#[test]
fn pair_operator_renders_the_between_control_and_inverted_warning() {
    let rt = runtime(
        Probe {
            filter: FilterCondition::new(
                "age",
                "between",
                FilterValue::Pair("18".into(), "30".into()),
            ),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(rt.find("edge-between").is_some(), "pair → between control");
    assert!(rt.find("edge-between-lower").is_some());
    assert!(rt.find("edge-between-upper").is_some());
    assert_eq!(
        element_text(&rt, "edge-between-connector").as_deref(),
        Some(t("filter.and")),
        "the connector is filter.and"
    );
    assert!(rt.find("edge-between-inverted").is_none());

    let inverted = runtime(
        Probe {
            filter: FilterCondition::new(
                "age",
                "between",
                FilterValue::Pair("30".into(), "18".into()),
            ),
            // The ported between control reads its two text drafts (React's
            // `onCommit` fires each keystroke), so the inverted bounds live
            // there as well as in the condition.
            lower: TextEdit::new("30"),
            upper: TextEdit::new("18"),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert_eq!(
        element_text(&inverted, "edge-between-inverted").as_deref(),
        Some(t("filter.betweenInverted")),
        "an inverted range shows the exact React key"
    );
}

// ---- 4. remove, collapse and the focus-adaptation --------------------------------

#[test]
fn remove_button_reports_the_row_index() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    assert_eq!(
        element(&rt, "edge-remove").label.as_deref(),
        Some(t("filter.remove")),
        "the button keeps React's aria-label"
    );
    click(&mut rt, "edge-remove");
    assert_eq!(
        rt.state.removes, vec![2],
        "onRemove(index) is reported with the row's index"
    );
    assert_eq!(rt.state.collapses, 0, "removing is not collapsing");
}

#[test]
fn enter_on_a_complete_value_commits_and_collapses() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    click(&mut rt, "edge-value");
    assert_eq!(
        focused(&rt).as_deref(),
        Some("input"),
        "the kit text input inside `edge-value` takes focus"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.changes.len(), 1);
    assert_eq!(
        rt.state.changes[0].value,
        FilterValue::Single("alice".into())
    );
    assert_eq!(
        rt.state.collapses, 1,
        "React's onEnterComplete collapses when the condition is complete"
    );
}

#[test]
fn enter_on_an_empty_value_commits_but_does_not_collapse() {
    let mut rt = runtime(
        Probe {
            single: TextEdit::new(""),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "edge-value");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.changes.len(), 1, "the empty literal is still committed");
    assert_eq!(rt.state.changes[0].value, FilterValue::Single(String::new()));
    assert_eq!(
        rt.state.collapses, 0,
        "an incomplete filter must not collapse (React isCompleteFilter)"
    );
}

/// Rune has no per-element blur or deferred timer. The expressible adaptation
/// is: an open listbox keeps the row expanded until a real choice lands, and
/// only a complete value commit collapses. This test pins that observable part.
#[test]
fn an_open_listbox_does_not_collapse_before_a_choice() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Tab);
    click(&mut rt, "edge-operator-trigger");
    assert!(
        rt.find("edge-operator-content").is_some(),
        "the operator listbox is mounted"
    );
    assert_eq!(rt.state.collapses, 0, "opening the list is not a completion");
    click(&mut rt, "option0"); // eq, a value-taking operator
    assert_eq!(rt.state.collapses, 0, "picking a value operator stays expanded");
    assert_eq!(rt.state.changes.last().map(|c| c.operator), Some("eq"));
}

// ---- 5. non-wrapping geometry in both windows and themes --------------------------

fn geometry_probe(filter: FilterCondition) -> Probe {
    Probe {
        filter,
        ..Probe::default()
    }
}

fn geometry_filters() -> Vec<FilterCondition> {
    vec![
        FilterCondition::new("name", "eq", FilterValue::Single("alice".into())),
        FilterCondition::new(
            "status",
            "in",
            FilterValue::Tokens(vec!["paid".into(), "sent".into()]),
        ),
        FilterCondition::new(
            "age",
            "between",
            FilterValue::Pair("18".into(), "30".into()),
        ),
    ]
}

#[test]
fn the_row_stays_inside_both_windows_and_never_wraps() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            for filter in geometry_filters() {
                let rt = runtime(geometry_probe(filter.clone()), size, dark);
                let (window_w, window_h) = size;
                let info = rt.elements();

                // Independent channel: every rect comes from the laid-out tree.
                for element in &info {
                    let [x, y, w, h] = element.rect;
                    let key = element
                        .key
                        .clone()
                        .unwrap_or_else(|| "<unnamed>".to_owned());
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= window_w + 0.5 && y + h <= window_h + 0.5,
                        "{key} rect [{x}, {y}, {w}, {h}] leaves {window_w}x{window_h} \
                         (dark={dark}, operator={})",
                        filter.operator
                    );
                }

                let root_index = info
                    .iter()
                    .position(|e| e.key.as_deref() == Some("edge"))
                    .expect("the row root is laid out");
                let root = &info[root_index];
                let children: Vec<&rune::ElementInfo> = info
                    .iter()
                    .filter(|e| e.parent == Some(root_index))
                    .collect();
                assert!(
                    children.len() >= 3,
                    "column + operator + remove are direct children"
                );
                let tops: Vec<f32> = children.iter().map(|e| e.rect[1]).collect();
                let bottoms: Vec<f32> = children
                    .iter()
                    .map(|e| e.rect[1] + e.rect[3])
                    .collect();
                let band = bottoms
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max)
                    - tops.iter().copied().fold(f32::INFINITY, f32::min);
                assert!(
                    band <= 40.0,
                    "children share one inline row, not two lines: band={band} \
                     (operator={}, size={window_w}x{window_h}, dark={dark})",
                    filter.operator
                );
                assert!(
                    root.rect[3] <= 44.0,
                    "the row hugs one line: height={} (operator={})",
                    root.rect[3],
                    filter.operator
                );
                assert!(
                    root.rect[2] <= window_w - 2.0 * PAD + 1.0,
                    "the row stays inside the padded page: width={}",
                    root.rect[2]
                );
            }
        }
    }
}
