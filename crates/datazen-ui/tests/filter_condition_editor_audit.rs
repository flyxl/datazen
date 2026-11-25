//! T056 independent audit — `FilterConditionEditor` port.
//!
//! This file is written by the auditor, not the porter. It purposefully avoids
//! the porter's `Probe`/`view` helpers and asserts the *contracts* the port is
//! supposed to keep: the locally-ported operator table and value conversions,
//! the completeness predicate, the exact i18n keys, and the observable
//! behaviour of the token field's comma / Enter / Backspace / remove paths
//! (which the porter's own tests only check for presence, not behaviour).
//!
//! Every assertion here is an observable: a returned value, a translated key's
//! existence, or a change reported to the host after a real key/pointer event.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::components::filter::{
    cell_types_for_operator, convert_filter_value, filter_operator_meta, is_complete_filter, normalize_operator_for_column,
    operators_for_column, to_pair, to_tokens, value_input_for, EditorSelect, FilterColumn,
    FilterCondition, FilterConditionEditor, FilterOperatorGroup, FilterValue, FilterValueInputKind,
};
use datazen_ui::i18n::has;
use datazen_ui::support::cell_types::CellType;

// ---------------------------------------------------------------------------
// 1. The locally-ported contracts must match the React tables exactly.
// ---------------------------------------------------------------------------

#[test]
fn operator_meta_table_matches_the_react_table_for_all_22_operators() {
    // (operator, value kind, group, escapes_like_pattern) — transcribed from
    // packages/frontend/src/lib/filterOperators.ts FILTER_OPERATOR_META.
    use FilterOperatorGroup as G;
    let expected: &[(&str, FilterValueInputKind, FilterOperatorGroup, bool)] = &[
        ("eq", FilterValueInputKind::Single, G::Compare, false),
        ("ne", FilterValueInputKind::Single, G::Compare, false),
        ("gt", FilterValueInputKind::Single, G::Compare, false),
        ("lt", FilterValueInputKind::Single, G::Compare, false),
        ("gte", FilterValueInputKind::Single, G::Compare, false),
        ("lte", FilterValueInputKind::Single, G::Compare, false),
        ("between", FilterValueInputKind::Pair, G::Compare, false),
        ("notBetween", FilterValueInputKind::Pair, G::Compare, false),
        ("like", FilterValueInputKind::Single, G::Pattern, false),
        ("contains", FilterValueInputKind::Single, G::Pattern, true),
        ("notContains", FilterValueInputKind::Single, G::Pattern, true),
        ("startsWith", FilterValueInputKind::Single, G::Pattern, true),
        ("endsWith", FilterValueInputKind::Single, G::Pattern, true),
        ("in", FilterValueInputKind::Tokens, G::Set, false),
        ("notIn", FilterValueInputKind::Tokens, G::Set, false),
        ("isNull", FilterValueInputKind::None, G::Null, false),
        ("isNotNull", FilterValueInputKind::None, G::Null, false),
        ("isEmpty", FilterValueInputKind::None, G::Empty, false),
        ("isNotEmpty", FilterValueInputKind::None, G::Empty, false),
        ("regex", FilterValueInputKind::Single, G::Dialect, false),
        ("notRegex", FilterValueInputKind::Single, G::Dialect, false),
        ("jsonContains", FilterValueInputKind::Single, G::Dialect, false),
    ];
    assert_eq!(expected.len(), 22, "React ships 22 operators");
    for (id, kind, group, escapes) in expected {
        let meta = filter_operator_meta(id).unwrap_or_else(|| panic!("{id} has a table row"));
        assert_eq!(meta.id, *id);
        assert_eq!(meta.value_input, *kind, "{id} value control");
        assert_eq!(meta.group, *group, "{id} group");
        assert_eq!(meta.escapes_like_pattern, *escapes, "{id} like flag");
        assert_eq!(meta.dialect, *group == G::Dialect, "{id} dialect flag");
        assert_eq!(value_input_for(id), *kind, "{id} value_input_for");
    }
    assert!(filter_operator_meta("notAnOperator").is_none());
    assert_eq!(
        value_input_for("notAnOperator"),
        FilterValueInputKind::Single,
        "unknown operators degrade to a single field, like React"
    );
}

#[test]
fn operators_for_column_is_the_type_and_driver_intersection_with_dialects_last() {
    let supported = ["eq", "like", "regex", "jsonContains", "between"];
    // text: eq, like (shared) then regex (dialect); jsonContains is JSON only;
    // `between` is not a text operator.
    assert_eq!(
        operators_for_column(Some("varchar(64)"), &supported, None),
        vec!["eq", "like", "regex"]
    );
    // number: only `eq` and `between` of the declared set; no like/regex.
    assert_eq!(
        operators_for_column(Some("int4"), &supported, None),
        vec!["eq", "between"]
    );
    // json: `eq` plus the json dialect; `like`/`regex` are not offered.
    assert_eq!(
        operators_for_column(Some("jsonb"), &supported, None),
        vec!["eq", "jsonContains"]
    );
    // an empty driver whitelist yields an empty intersection (the panel owns
    // the "empty means shared" rule, exactly as in React).
    assert!(operators_for_column(Some("varchar(64)"), &[], None).is_empty());
}

#[test]
fn normalize_operator_prefers_current_then_eq_then_first_then_keeps_current() {
    // current still allowed → unchanged.
    assert_eq!(
        normalize_operator_for_column("like", Some("varchar(64)"), &["eq", "like"], None),
        "like"
    );
    // current dropped, `eq` available → `eq`.
    assert_eq!(
        normalize_operator_for_column("like", Some("int4"), &["eq", "between"], None),
        "eq"
    );
    // neither current nor `eq` available → first of the allowed list.
    assert_eq!(
        normalize_operator_for_column("eq", Some("varchar(64)"), &["like", "contains"], None),
        "like"
    );
    // nothing allowed → keep the current operator instead of inventing one.
    assert_eq!(
        normalize_operator_for_column("like", Some("int4"), &["regex"], None),
        "like"
    );
}

#[test]
fn cell_types_for_operator_is_the_inverse_of_the_matrix() {
    let text_like = cell_types_for_operator("like");
    assert!(text_like.contains(&CellType::Text));
    assert!(text_like.contains(&CellType::Unknown));
    assert!(!text_like.contains(&CellType::Enum), "no LIKE on an enum column");
    assert!(!text_like.contains(&CellType::Uuid), "no LIKE on a uuid column");
    assert!(cell_types_for_operator("between").contains(&CellType::Number));
    assert!(!cell_types_for_operator("between").contains(&CellType::Text));
    assert_eq!(
        cell_types_for_operator("jsonContains"),
        vec![CellType::Json],
        "jsonContains is a JSON-only dialect operator"
    );
}

// ---------------------------------------------------------------------------
// 2. convertFilterValue — exact value conversions, including `none` collapse.
// ---------------------------------------------------------------------------

#[test]
fn convert_filter_value_keeps_the_value_when_kinds_match_or_one_side_is_none() {
    use FilterValueInputKind as K;
    let tokens = FilterValue::Tokens(vec!["a".into(), "b".into()]);
    assert_eq!(convert_filter_value(K::Tokens, K::Tokens, &tokens), tokens);
    // to `none`: kept so switching back restores the value.
    assert_eq!(
        convert_filter_value(K::Single, K::None, &FilterValue::Single("alice".into())),
        FilterValue::Single("alice".into())
    );
    // from `none`: kept, so switching from isNull to eq restores the value.
    assert_eq!(
        convert_filter_value(K::None, K::Single, &FilterValue::Single("alice".into())),
        FilterValue::Single("alice".into())
    );
}

#[test]
fn convert_filter_value_uses_the_exact_react_conversions() {
    use FilterValueInputKind as K;
    // single → pair: literal becomes the lower bound, upper empty.
    assert_eq!(
        convert_filter_value(K::Single, K::Pair, &FilterValue::Single("alice".into())),
        FilterValue::Pair("alice".into(), String::new())
    );
    // single → tokens: comma split + trim + drop empties.
    assert_eq!(
        convert_filter_value(K::Single, K::Tokens, &FilterValue::Single(" a, ,b ".into())),
        FilterValue::Tokens(vec!["a".into(), "b".into()])
    );
    // pair → single: lower bound only.
    assert_eq!(
        convert_filter_value(K::Pair, K::Single, &FilterValue::Pair("18".into(), "30".into())),
        FilterValue::Single("18".into())
    );
    // pair → tokens: both bounds, empty ones filtered out.
    assert_eq!(
        convert_filter_value(
            K::Pair,
            K::Tokens,
            &FilterValue::Pair("18".into(), String::new())
        ),
        FilterValue::Tokens(vec!["18".into()])
    );
    // tokens → single: comma-joined with a space.
    assert_eq!(
        convert_filter_value(
            K::Tokens,
            K::Single,
            &FilterValue::Tokens(vec!["a".into(), "b".into()])
        ),
        FilterValue::Single("a, b".into())
    );
    // tokens → pair: first two tokens, missing upper empty.
    assert_eq!(
        convert_filter_value(
            K::Tokens,
            K::Pair,
            &FilterValue::Tokens(vec!["18".into()])
        ),
        FilterValue::Pair("18".into(), String::new())
    );
}

#[test]
fn to_pair_and_to_tokens_match_the_react_shapes() {
    assert_eq!(to_pair(&FilterValue::Single("a, b".into())), ("a".into(), "b".into()));
    assert_eq!(to_pair(&FilterValue::Single("x".into())), ("x".into(), String::new()));
    assert_eq!(
        to_pair(&FilterValue::Tokens(vec!["a".into(), "b".into()])),
        ("a".into(), "b".into())
    );
    assert_eq!(
        to_tokens(&FilterValue::Pair("lo".into(), "hi".into())),
        vec!["lo".to_string(), "hi".to_string()]
    );
    assert_eq!(to_tokens(&FilterValue::None), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// 3. isCompleteFilter — ported from stores/tableData/filterUtils.ts.
// ---------------------------------------------------------------------------

#[test]
fn is_complete_filter_matches_the_react_predicate() {
    let cond = |op, value| FilterCondition::new("name", op, value);
    // no column is never complete.
    assert!(!is_complete_filter(&FilterCondition::new(
        "",
        "eq",
        FilterValue::Single("a".into())
    )));
    // value-less operators are complete on their own.
    assert!(is_complete_filter(&cond("isNull", FilterValue::None)));
    // single: only the empty string is missing (React does not trim here).
    assert!(!is_complete_filter(&cond("eq", FilterValue::Single(String::new()))));
    assert!(is_complete_filter(&cond("eq", FilterValue::Single(" ".into()))));
    assert!(!is_complete_filter(&cond("eq", FilterValue::None)));
    // pair: exactly two filled bounds.
    assert!(is_complete_filter(&cond(
        "between",
        FilterValue::Pair("1".into(), "2".into())
    )));
    assert!(!is_complete_filter(&cond(
        "between",
        FilterValue::Pair("1".into(), String::new())
    )));
    assert!(is_complete_filter(&cond("between", FilterValue::Single("1,2".into()))));
    assert!(!is_complete_filter(&cond("between", FilterValue::Single("1".into()))));
    // tokens: at least one non-empty value.
    assert!(!is_complete_filter(&cond("in", FilterValue::Tokens(vec![]))));
    assert!(is_complete_filter(&cond("in", FilterValue::Tokens(vec!["a".into()]))));
    assert!(is_complete_filter(&cond("in", FilterValue::Single("a,,b".into()))));
    assert!(!is_complete_filter(&cond("in", FilterValue::Single(String::new()))));
}

// ---------------------------------------------------------------------------
// 4. i18n: every key the editor resolves must exist in the React-derived table.
// ---------------------------------------------------------------------------

#[test]
fn every_i18n_key_used_by_the_editor_exists() {
    let operators = [
        "eq", "ne", "gt", "lt", "gte", "lte", "between", "notBetween", "like", "contains",
        "notContains", "startsWith", "endsWith", "in", "notIn", "isNull", "isNotNull",
        "isEmpty", "isNotEmpty", "regex", "notRegex", "jsonContains",
    ];
    for op in operators {
        let key = format!("filter.{op}");
        assert!(has(&key), "missing i18n key {key}");
    }
    for key in [
        "filter.value",
        "filter.tokensPlaceholder",
        "filter.and",
        "filter.betweenInverted",
        "filter.likeCaseInsensitiveHint",
        "filter.tokenRemove",
        "filter.remove",
    ] {
        assert!(has(key), "missing i18n key {key}");
    }
}

// ---------------------------------------------------------------------------
// 5. Observable token-field behaviour (the porter tests only check presence).
// ---------------------------------------------------------------------------

struct Probe {
    filter: FilterCondition,
    supported: Vec<&'static str>,
    single: TextEdit,
    tokens: TextEdit,
    lower: TextEdit,
    upper: TextEdit,
    changes: Vec<FilterCondition>,
    removes: Vec<usize>,
    collapses: usize,
    open: Option<EditorSelect>,
}

impl Default for Probe {
    fn default() -> Self {
        Probe {
            filter: FilterCondition::new("name", "eq", FilterValue::Single("alice".into())),
            supported: vec!["eq", "in", "between", "isNull"],
            single: TextEdit::new("alice"),
            tokens: TextEdit::new(""),
            lower: TextEdit::new("18"),
            upper: TextEdit::new("30"),
            changes: Vec::new(),
            removes: Vec::new(),
            collapses: 0,
            open: None,
        }
    }
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    let row = FilterConditionEditor::new("edge", 0, &state.filter, &[FilterColumn::new("name", Some("varchar(64)"))])
        .supported_operators(&state.supported)
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
        .on_change(|s: &mut Probe, _, next| {
            s.changes.push(next.clone());
            s.filter = next;
        })
        .on_remove(|s: &mut Probe, index| s.removes.push(index))
        .on_collapse(|s: &mut Probe| s.collapses += 1)
        .on_toggle(|s: &mut Probe, which| {
            s.open = if s.open == Some(which) { None } else { Some(which) };
        })
        .on_close(|s: &mut Probe| s.open = None)
        .build(dark, &mut layer);
    rune_kit::root(col().w_full().h_full().child(row), layer)
}

fn runtime(state: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        state,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<Probe>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn token_probe(tokens: Vec<&str>, pending: &str) -> Probe {
    Probe {
        filter: FilterCondition::new(
            "status",
            "in",
            FilterValue::Tokens(tokens.into_iter().map(str::to_owned).collect()),
        ),
        tokens: TextEdit::new(pending),
        ..Probe::default()
    }
}

#[test]
fn comma_key_commits_the_pending_token_and_clears_the_field() {
    let mut rt = runtime(token_probe(vec![], "paid"));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Text(",".into()));
    assert_eq!(rt.state.changes.len(), 1, "comma commits one token");
    assert_eq!(
        rt.state.changes[0].value,
        FilterValue::Tokens(vec!["paid".into()]),
        "the pending text becomes a chip"
    );
    assert_eq!(rt.state.tokens.text, "", "the pending field is cleared");
    assert_eq!(rt.state.collapses, 0, "comma alone never collapses");
}

#[test]
fn backspace_on_an_empty_token_field_removes_the_last_chip() {
    let mut rt = runtime(token_probe(vec!["paid", "sent"], ""));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Backspace);
    assert_eq!(rt.state.changes.len(), 1);
    assert_eq!(
        rt.state.changes[0].value,
        FilterValue::Tokens(vec!["paid".into()]),
        "Backspace drops the last chip, keeping order"
    );
}

#[test]
fn clicking_a_token_remove_control_updates_the_condition() {
    let mut rt = runtime(token_probe(vec!["paid", "sent"], ""));
    click(&mut rt, "edge-token-0-remove");
    assert_eq!(rt.state.changes.len(), 1);
    assert_eq!(
        rt.state.changes[0].value,
        FilterValue::Tokens(vec!["sent".into()]),
        "the clicked chip is removed by position"
    );
    assert_eq!(rt.state.collapses, 0, "removing a token is not a collapse");
}

#[test]
fn enter_commits_a_pending_token_and_collapses_a_complete_set() {
    let mut rt = runtime(token_probe(vec!["paid"], "sent"));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.changes.len(), 1);
    assert_eq!(
        rt.state.changes[0].value,
        FilterValue::Tokens(vec!["paid".into(), "sent".into()])
    );
    assert_eq!(rt.state.collapses, 1, "a committed, non-empty set collapses");
}

// ---------------------------------------------------------------------------
// 6. The out-of-list current operator, observed in a different window/theme.
// ---------------------------------------------------------------------------

#[test]
fn operator_changes_round_trip_single_and_tokens_values() {
    let mut rt = runtime(Probe {
        filter: FilterCondition::new("name", "eq", FilterValue::Single("a, b".into())),
        supported: vec!["eq", "in", "isNull"],
        ..Probe::default()
    });
    // text intersection is [eq, in, isNull]; option1 is `in` (tokens).
    click(&mut rt, "edge-operator-trigger");
    click(&mut rt, "option1");
    assert_eq!(rt.state.changes.len(), 1);
    let first = &rt.state.changes[0];
    assert_eq!(first.operator, "in");
    assert_eq!(
        first.value,
        FilterValue::Tokens(vec!["a".into(), "b".into()]),
        "single → tokens splits and trims the comma string (observable handler path)"
    );

    // Back to `eq` (single); option0 after the change is still `eq`.
    rt.state.open = Some(EditorSelect::Operator);
    rt.frame(0.0);
    click(&mut rt, "option0");
    assert_eq!(rt.state.changes.len(), 2);
    let second = &rt.state.changes[1];
    assert_eq!(second.operator, "eq");
    assert_eq!(
        second.value,
        FilterValue::Single("a, b".into()),
        "tokens → single rejoins with comma-space"
    );
    assert_eq!(
        rt.state.collapses, 0,
        "both targets carry a value, so the row stays expanded"
    );
}

#[test]
fn none_value_kind_renders_no_value_control() {
    let rt = runtime(Probe {
        filter: FilterCondition::new("name", "isNull", FilterValue::None),
        supported: vec!["eq", "isNull", "in", "between"],
        ..Probe::default()
    });
    assert!(rt.find("edge-value").is_none(), "no single field for a none operator");
    assert!(rt.find("edge-tokens").is_none(), "no token list for a none operator");
    assert!(rt.find("edge-between").is_none(), "no range control for a none operator");
    assert!(rt.find("edge-like-case-insensitive").is_none());
    assert!(rt.find("edge-remove").is_some(), "the row still has its remove button");
}

#[test]
fn current_operator_stays_visible_in_the_small_light_window() {
    let mut rt = Runtime::new(
        Probe {
            filter: FilterCondition::new("name", "like", FilterValue::Single("ann".into())),
            // text intersection: eq, contains, isNull; `like` is pushed.
            supported: vec!["eq", "contains", "isNull"],
            ..Probe::default()
        },
        view,
        (960.0, 640.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    click(&mut rt, "edge-operator-trigger");
    // three text-intersection choices + the out-of-list `like`, in that order.
    let labels: Vec<String> = (0..4)
        .map(|i| {
            rt.elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(&format!("option{i}")))
                .and_then(|e| e.label)
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(labels.len(), 4, "the current operator is appended");
    assert!(
        rt.find("option4").is_none(),
        "nothing beyond the intersection + current operator"
    );
    let like_label = datazen_ui::i18n::t("filter.like");
    assert_eq!(labels[3], like_label, "the last choice is the draft's `like`");
}
