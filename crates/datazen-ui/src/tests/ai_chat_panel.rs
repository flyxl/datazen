//! T009-S1: AiChatPanel leaf subcomponents (`formatMcpToolDisplayName`,
//! `hasSnapshotContent`, `AutoContextChip`, `QuestionBlock`).
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the components in a probe `Runtime` and read back the laid-out element
//! tree and the accessibility dump — never the component's own report of
//! itself. Geometry is asserted on `rt.elements()` rects, which come from
//! the layout engine, not from the code under test.

use std::collections::HashMap;

use rune::{col, Color, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::json;

use crate::components::ai::{
    format_mcp_tool_display_name, has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview,
    AiQuestion, Answer, AutoContextChip, QuestionBlock, QuestionBlockState,
};
use crate::i18n::{has, t};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn find_json<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node.get("key").and_then(|v| v.as_str()) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

fn a11y_label<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    let json = rt.accessibility_json();
    let node = find_json(&json, key)?;
    node.get("name").and_then(|v| v.as_str()).map(str::to_owned)
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn sample_questions() -> Vec<AiQuestion> {
    vec![
        AiQuestion::new("q1", "Which database?")
            .option("o1", "Postgres")
            .option("o2", "MySQL"),
        AiQuestion::new("q2", "What do you need?")
            .allow_multiple(true)
            .option("a", "Migrate")
            .option("b", "Inspect")
            .option("c", "Optimize"),
        AiQuestion::new("q3", "Anything else?"),
    ]
}

#[derive(Default)]
struct Probe {
    block: QuestionBlockState,
    removed: Vec<String>,
    submitted_payloads: Vec<String>,
}

fn probe() -> Probe {
    let mut state = Probe::default();
    state.block = QuestionBlockState::new(&sample_questions());
    state
}

fn block_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let questions = sample_questions();
    col()
        .key("probe-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            QuestionBlock::<Probe>::new(questions, &state.block)
                .on_toggle(|s: &mut Probe, qid, opt, multi| s.block.toggle(qid, opt, multi))
                .on_custom_edit(|s: &mut Probe, qid: &str| s.block.custom_edit(qid))
                .on_submit(|s: &mut Probe| {
                    if let Some(payload) = s.block.submit(&sample_questions()) {
                        s.submitted_payloads.push(payload);
                    }
                })
                .build(true),
        )
}

fn chip_view(_state: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .key("probe-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(8.0)
        .child(
            AutoContextChip::<Probe>::new(t("chat.autoContext.sql"), "ai-auto-context-sql")
                .on_remove(|s: &mut Probe, id| s.removed.push(id.to_string()))
                .build(true),
        )
        .child(
            AutoContextChip::<Probe>::new(
                t("chat.autoContext.selection"),
                "ai-auto-context-selection",
            )
            .on_remove(|s: &mut Probe, id| s.removed.push(id.to_string()))
            .build(true),
        )
}

// ---- formatMcpToolDisplayName ----------------------------------------------

#[test]
fn mcp_tool_names_show_from_the_third_segment_on() {
    assert_eq!(format_mcp_tool_display_name("a/b/c"), "c");
    assert_eq!(format_mcp_tool_display_name("a/b/c/d"), "c/d");
    assert_eq!(
        format_mcp_tool_display_name("server/tool/name/extra"),
        "name/extra"
    );
}

#[test]
fn mcp_tool_names_shorter_than_three_segments_show_as_is() {
    assert_eq!(format_mcp_tool_display_name("a/b"), "a/b");
    assert_eq!(format_mcp_tool_display_name("tool"), "tool");
    assert_eq!(format_mcp_tool_display_name(""), "");
    assert_eq!(format_mcp_tool_display_name("a/b/"), "");
}

// ---- hasSnapshotContent -----------------------------------------------------

fn snapshot() -> AiChatContextSnapshot {
    AiChatContextSnapshot::default()
}

#[test]
fn an_empty_or_missing_snapshot_has_no_content() {
    assert!(!has_snapshot_content(None));
    assert!(!has_snapshot_content(Some(&snapshot())));
    let mut blank = snapshot();
    blank.editor_sql = Some("   ".to_string());
    blank.selection = Some("\n\t ".to_string());
    assert!(
        !has_snapshot_content(Some(&blank)),
        "whitespace-only fields do not count"
    );
    let mut preview_only = snapshot();
    preview_only.result_preview = Some(AiChatResultPreview::default());
    assert!(
        !has_snapshot_content(Some(&preview_only)),
        "a preview with no columns or rows does not count"
    );
}

#[test]
fn any_non_empty_snapshot_field_counts() {
    let mut sql = snapshot();
    sql.editor_sql = Some("select 1".to_string());
    assert!(has_snapshot_content(Some(&sql)));

    let mut selection = snapshot();
    selection.selection = Some("  x ".to_string());
    assert!(has_snapshot_content(Some(&selection)));

    let mut error = snapshot();
    error.last_error = Some("boom".to_string());
    assert!(has_snapshot_content(Some(&error)));

    let mut columns = snapshot();
    columns.result_preview = Some(AiChatResultPreview {
        columns: vec!["id".to_string()],
        rows: vec![],
    });
    assert!(has_snapshot_content(Some(&columns)));

    let mut rows = snapshot();
    rows.result_preview = Some(AiChatResultPreview {
        columns: vec![],
        rows: vec![vec![json!(1)]],
    });
    assert!(has_snapshot_content(Some(&rows)));
}

// ---- QuestionBlockState: initial answers ------------------------------------

#[test]
fn initial_answers_are_empty_list_for_multi_and_empty_string_otherwise() {
    let state = QuestionBlockState::new(&sample_questions());
    assert_eq!(
        state.answer("q1"),
        Some(&Answer::Single(String::new())),
        "single-select starts as ''"
    );
    assert_eq!(
        state.answer("q2"),
        Some(&Answer::Multiple(vec![])),
        "multi-select starts as []"
    );
    assert_eq!(state.answer("q3"), Some(&Answer::Single(String::new())));
    assert!(!state.submitted());
    assert_eq!(state.payload(), None);
}

// ---- QuestionBlockState: toggling -------------------------------------------

#[test]
fn single_toggle_selects_then_clears_on_a_second_press() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.toggle("q1", "o1", false);
    assert_eq!(state.answer("q1"), Some(&Answer::Single("o1".to_string())));
    state.toggle("q1", "o1", false);
    assert_eq!(
        state.answer("q1"),
        Some(&Answer::Single(String::new())),
        "pressing the selected option clears it"
    );
    state.toggle("q1", "o2", false);
    assert_eq!(state.answer("q1"), Some(&Answer::Single("o2".to_string())));
}

#[test]
fn multi_toggle_adds_and_removes_in_pick_order() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.toggle("q2", "a", true);
    state.toggle("q2", "c", true);
    assert_eq!(
        state.answer("q2"),
        Some(&Answer::Multiple(vec!["a".to_string(), "c".to_string()]))
    );
    state.toggle("q2", "a", true);
    assert_eq!(
        state.answer("q2"),
        Some(&Answer::Multiple(vec!["c".to_string()])),
        "pressing a picked option removes it, keeping the rest in order"
    );
}

#[test]
fn toggling_an_option_clears_the_custom_draft() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.custom_edit("q1").text = "typed first".to_string();
    assert_eq!(state.custom_text("q1"), "typed first");
    state.toggle("q1", "o1", false);
    assert_eq!(
        state.custom_text("q1"),
        "",
        "React derives the input value from the answer, so it shows '' once an option is picked"
    );
    assert_eq!(state.answer("q1"), Some(&Answer::Single("o1".to_string())));
}

// ---- QuestionBlockState::format_submission -----------------------------------

fn answers(pairs: &[(&str, Answer)]) -> HashMap<String, Answer> {
    pairs
        .iter()
        .map(|(id, answer)| (id.to_string(), answer.clone()))
        .collect()
}

#[test]
fn submission_uses_the_option_label_for_a_picked_option() {
    let questions = sample_questions();
    let payload = QuestionBlockState::format_submission(
        &questions,
        &answers(&[("q1", Answer::Single("o1".to_string()))]),
    );
    assert_eq!(payload.as_deref(), Some("[q1] Which database?\nPostgres"));
}

#[test]
fn submission_falls_back_to_the_text_for_free_text_and_unknown_ids() {
    let questions = sample_questions();
    let payload = QuestionBlockState::format_submission(
        &questions,
        &answers(&[
            ("q1", Answer::Single("SQLite".to_string())),
            (
                "q2",
                Answer::Multiple(vec!["a".to_string(), "gone".to_string()]),
            ),
        ]),
    );
    assert_eq!(
        payload.as_deref(),
        Some("[q1] Which database?\nSQLite\n\n[q2] What do you need?\nMigrate, gone")
    );
}

#[test]
fn submission_skips_empty_answers_and_reports_none_when_all_empty() {
    let questions = sample_questions();
    assert_eq!(
        QuestionBlockState::format_submission(
            &questions,
            &answers(&[
                ("q1", Answer::Single("   ".to_string())),
                ("q2", Answer::Multiple(vec![])),
            ])
        ),
        None,
        "whitespace-only text and an empty multi list are both skipped"
    );
    assert_eq!(
        QuestionBlockState::format_submission(&questions, &HashMap::new()),
        None
    );
}

#[test]
fn submission_joins_answered_questions_with_a_blank_line() {
    let questions = sample_questions();
    let payload = QuestionBlockState::format_submission(
        &questions,
        &answers(&[
            ("q1", Answer::Single("o2".to_string())),
            (
                "q2",
                Answer::Multiple(vec!["b".to_string(), "c".to_string()]),
            ),
            ("q3", Answer::Single("later".to_string())),
        ]),
    );
    assert_eq!(
        payload.as_deref(),
        Some(
            "[q1] Which database?\nMySQL\n\n[q2] What do you need?\nInspect, Optimize\n\n[q3] Anything else?\nlater"
        )
    );
}

// ---- QuestionBlockState::submit ----------------------------------------------

#[test]
fn submit_marks_the_block_submitted_and_keeps_the_payload() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.toggle("q1", "o2", false);
    let payload = state.submit(&sample_questions());
    assert_eq!(payload.as_deref(), Some("[q1] Which database?\nMySQL"));
    assert!(state.submitted());
    assert_eq!(state.payload(), Some("[q1] Which database?\nMySQL"));
}

#[test]
fn submit_with_no_answers_does_nothing() {
    let mut state = QuestionBlockState::new(&sample_questions());
    assert_eq!(state.submit(&sample_questions()), None);
    assert!(!state.submitted(), "an all-empty block stays unsubmitted");
    assert_eq!(state.payload(), None);
}

#[test]
fn a_custom_draft_reaches_the_payload_on_submit() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.custom_edit("q3").text = "hello".to_string();
    let payload = state.submit(&sample_questions());
    assert_eq!(
        payload.as_deref(),
        Some("[q3] Anything else?\nhello"),
        "the draft is synced before formatting, so typed text is not lost"
    );
}

#[test]
fn a_custom_draft_replaces_a_multi_selection() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.toggle("q2", "a", true);
    state.toggle("q2", "b", true);
    state.custom_edit("q2").text = "something else".to_string();
    let payload = state.submit(&sample_questions());
    assert_eq!(
        payload.as_deref(),
        Some("[q2] What do you need?\nsomething else"),
        "React's setTextAnswer overwrites the array answer with the typed text"
    );
}

#[test]
fn typing_then_erasing_clears_the_answer() {
    let mut state = QuestionBlockState::new(&sample_questions());
    state.custom_edit("q1").text = "x".to_string();
    state.custom_edit("q1");
    assert_eq!(state.answer("q1"), Some(&Answer::Single("x".to_string())));
    state.custom_edit("q1").text = String::new();
    state.custom_edit("q1");
    assert_eq!(
        state.answer("q1"),
        Some(&Answer::Single(String::new())),
        "erasing the text writes '' back, as React's onChange does"
    );
    assert_eq!(state.submit(&sample_questions()), None);
}

// ---- i18n --------------------------------------------------------------------

#[test]
fn the_question_keys_exist_and_resolve_to_chinese() {
    assert!(has("chat.questions.submit"));
    assert!(has("chat.questions.customAnswer"));
    assert_eq!(t("chat.questions.submit"), "提交回答");
    assert_eq!(t("chat.questions.customAnswer"), "输入自定义回答…");
}

// ---- AutoContextChip runtime -------------------------------------------------

#[test]
fn chip_renders_its_label_and_a_remove_button_named_after_it() {
    let mut rt = Runtime::new(probe(), chip_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let _ = element(&rt, "ai-auto-context-sql");
    assert!(
        element(&rt, "ai-auto-context-sql-remove").focusable,
        "the remove button is a focus stop"
    );
    assert_eq!(
        a11y_label(&rt, "ai-auto-context-sql-remove").as_deref(),
        Some("remove 当前 SQL"),
        "aria-label is React's `remove {{label}}` template"
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("当前 SQL")),
        "the chip shows its label text"
    );
}

#[test]
fn chip_remove_click_reports_only_the_pressed_chip() {
    let mut rt = Runtime::new(probe(), chip_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "ai-auto-context-selection-remove");
    assert_eq!(
        rt.state.removed,
        vec!["ai-auto-context-selection".to_string()],
        "only the pressed chip reports"
    );
    click(&mut rt, "ai-auto-context-sql-remove");
    assert_eq!(
        rt.state.removed,
        vec![
            "ai-auto-context-selection".to_string(),
            "ai-auto-context-sql".to_string()
        ]
    );
}

// ---- QuestionBlock runtime ---------------------------------------------------

#[test]
fn question_block_renders_every_react_testid() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in [
        "question-block",
        "question-block-item-q1",
        "question-block-item-q2",
        "question-block-item-q3",
        "question-option-q1-o1",
        "question-option-q1-o2",
        "question-option-q2-a",
        "question-option-q2-b",
        "question-option-q2-c",
        "question-custom-input-q1",
        "question-custom-input-q2",
        "question-custom-input-q3",
        "question-submit",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    let texts: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect();
    for prompt in ["Which database?", "What do you need?", "Anything else?"] {
        assert!(
            texts.iter().any(|text| text == prompt),
            "prompt {prompt:?} is rendered: {texts:?}"
        );
    }
    assert!(
        texts.iter().any(|text| text == "提交回答"),
        "the submit button carries chat.questions.submit: {texts:?}"
    );
}

#[test]
fn option_click_selects_and_the_element_tree_reports_it_selected() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(
        !element(&rt, "question-option-q1-o1").item_selected,
        "unselected before the click"
    );
    click(&mut rt, "question-option-q1-o1");
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single("o1".to_string()))
    );
    // `focus_selected` surfaces as `item_selected` on the laid-out element
    // (a Button is not an a11y `selectable_item`, so the a11y tree carries
    // no `selected` state for it); the accent wash fill is the paint-level
    // proof that the selected styling actually landed.
    let selected = element(&rt, "question-option-q1-o1");
    assert!(
        selected.item_selected,
        "the pressed option reports selected"
    );
    assert_ne!(
        selected.fill,
        Color::TRANSPARENT,
        "the pressed option paints its accent wash"
    );
    let sibling = element(&rt, "question-option-q1-o2");
    assert!(!sibling.item_selected, "the sibling stays unselected");
    assert_eq!(
        sibling.fill,
        Color::TRANSPARENT,
        "the sibling paints no wash"
    );
}

#[test]
fn a_second_click_clears_a_single_select() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-option-q1-o2");
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single("o2".to_string()))
    );
    click(&mut rt, "question-option-q1-o2");
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single(String::new())),
        "pressing the selected option again clears it"
    );
    assert!(
        !element(&rt, "question-option-q1-o2").item_selected,
        "the element-tree state flips back with the answer"
    );
}

#[test]
fn multi_option_clicks_accumulate_and_a_repeat_removes() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-option-q2-a");
    click(&mut rt, "question-option-q2-c");
    assert_eq!(
        rt.state.block.answer("q2"),
        Some(&Answer::Multiple(vec!["a".to_string(), "c".to_string()]))
    );
    assert!(element(&rt, "question-option-q2-a").item_selected);
    assert!(element(&rt, "question-option-q2-c").item_selected);
    assert!(!element(&rt, "question-option-q2-b").item_selected);
    click(&mut rt, "question-option-q2-a");
    assert_eq!(
        rt.state.block.answer("q2"),
        Some(&Answer::Multiple(vec!["c".to_string()]))
    );
}

#[test]
fn typed_custom_text_submits_and_empties_the_block() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-custom-input-q3");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        rt.state.block.custom_text("q3"),
        "hello",
        "a click puts a caret in the field and typing reaches it"
    );
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.submitted_payloads,
        vec!["[q3] Anything else?\nhello".to_string()]
    );
    assert!(rt.state.block.submitted(), "the block records the submit");
    assert!(
        has_key(&rt, "question-block"),
        "the empty shell keeps its key after submit"
    );
    for key in [
        "question-block-item-q1",
        "question-block-item-q2",
        "question-block-item-q3",
        "question-option-q1-o1",
        "question-custom-input-q3",
        "question-submit",
    ] {
        assert!(!has_key(&rt, key), "{key} is gone after submit");
    }
}

#[test]
fn submit_with_no_answers_keeps_the_block_rendered() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-submit");
    assert!(
        rt.state.submitted_payloads.is_empty(),
        "nothing is forwarded when every answer is empty"
    );
    assert!(!rt.state.block.submitted());
    assert!(has_key(&rt, "question-block-item-q1"));
    assert!(has_key(&rt, "question-submit"));
}

#[test]
fn submit_forwards_multi_and_single_answers_joined_with_a_blank_line() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-option-q1-o2");
    click(&mut rt, "question-option-q2-a");
    click(&mut rt, "question-option-q2-c");
    click(&mut rt, "question-custom-input-q3");
    rt.paste("Nope");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.submitted_payloads,
        vec![
            "[q1] Which database?\nMySQL\n\n[q2] What do you need?\nMigrate, Optimize\n\n[q3] Anything else?\nNope"
                .to_string()
        ]
    );
}

#[test]
fn tab_reaches_an_option_and_enter_and_space_each_toggle_it_once() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("question-option-q1-o1"),
        "Tab reaches the first option button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single("o1".to_string())),
        "Enter toggles the focused option exactly once"
    );
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single(String::new())),
        "Space toggles it back exactly once"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.block.answer("q1"),
        Some(&Answer::Single("o1".to_string())),
        "a second Enter toggles exactly once more"
    );
}

#[test]
fn chip_remove_activates_on_enter() {
    let mut rt = Runtime::new(probe(), chip_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for _ in 0..4 {
        if focused_key(&rt).as_deref() == Some("ai-auto-context-sql-remove") {
            break;
        }
        press(&mut rt, Key::Tab);
    }
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("ai-auto-context-sql-remove"),
        "Tab reaches the chip remove button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.removed,
        vec!["ai-auto-context-sql".to_string()],
        "Enter on the focused remove button reports the chip"
    );
}

#[test]
fn submit_activates_on_enter() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-option-q1-o1");
    for _ in 0..12 {
        if focused_key(&rt).as_deref() == Some("question-submit") {
            break;
        }
        press(&mut rt, Key::Tab);
    }
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("question-submit"),
        "Tab reaches the submit button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.submitted_payloads,
        vec!["[q1] Which database?\nPostgres".to_string()]
    );
    assert!(rt.state.block.submitted(), "Enter submits the block");
}

// ---- geometry ----------------------------------------------------------------

#[test]
fn block_and_chip_stay_inside_both_window_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(probe(), block_view, (w, h), env(true));
        rt.frame(0.0);
        assert_in_window(&rt, w, h, "question block (dark)");
        let mut chips = Runtime::new(probe(), chip_view, (w, h), env(true));
        chips.frame(0.0);
        assert_in_window(&chips, w, h, "auto-context chips (dark)");
    }
}

#[test]
fn block_stays_inside_the_minimum_window_in_light_theme() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(false));
    rt.frame(0.0);
    assert_in_window(&rt, 960.0, 640.0, "question block (light)");
}
