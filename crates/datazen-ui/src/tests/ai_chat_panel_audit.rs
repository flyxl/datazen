//! Independent audit of T009-S1 (the `AiChatPanel` leaf subcomponents ported
//! in `d6250f440`).
//!
//! Every assertion below re-derives its expectation from the React source
//! (`packages/frontend/src/components/ai/AiChatPanel.tsx`,
//! `packages/frontend/src/types/ai.ts`, `packages/frontend/src/locales/zh-CN/ai.ts`)
//! with fresh vectors — different question ids, labels and chip fields from
//! the coder's tests — so a value copied from the implementation under test
//! cannot pass by construction. Runtime facts are read off the laid-out
//! element tree (`rt.elements()` rects / `item_selected` / `fill`) and the
//! accessibility dump, never off the component's own report of itself.
//!
//! Known gaps pinned here (both recorded honestly in the port's module docs):
//!
//! * `aria-pressed` has no Rune hook: `vendor/rune/crates/rune/src/el/a11y.rs`
//!   exposes `label` / `checked` but no `pressed()`, and `Button` is not in
//!   `a11y::roles::selectable_item` (`ListBoxOption | ListItem | Tab |
//!   TreeItem | GridCell | Row | MenuListOption`), so a selected option
//!   button reports nothing in the accessibility tree. The audit pins the
//!   two channels that do carry the state — `ElementInfo::item_selected`
//!   (from `focus_selected`, `runtime/elements.rs`) and the accent wash
//!   `fill` — and pins the a11y absence, so a future Rune `pressed()`
//!   hook would flip that one assertion instead of silently changing
//!   behaviour.
//! * After submit React returns `null`; Rune keeps an empty keyed
//!   `question-block` shell. Measured, the shell does NOT become zero-size
//!   on the submit frame: its height animates down from 288.5pt (274.5pt
//!   on the submit frame, ~x0.74 per frame, 1.58pt after 20 frames, under
//!   0.5pt after ~30 frames) while its width stays full (928pt). The
//!   port's module docs call the shell "zero-size", which is only true in
//!   the limit — a doc inaccuracy, recorded as finding D1 (non-blocking:
//!   the collapse is transient, frees the space, and never leaves the
//!   window). The audit pins the measured collapse instead of the claim.
//!
//! Deliberate divergence found (not a defect): typing free text into a
//! **multi-select** question and then pressing an option. React's
//! `toggleOption` treats the string answer as an array — `[...current,
//! optionId]` spreads the typed text into characters (and
//! `current.filter(...)` throws when the text happens to contain the option
//! id as a substring). Rune replaces the draft with the single pick
//! (`Multiple([option])`) and clears the draft. The audit pins the Rune
//! behaviour; the React path is broken in that corner, so parity there
//! would be a bug, not a feature.

use super::*;

use rune::{col, Color, Cx, El, Key, KeyEvent, Mods, Runtime};
use serde_json::json;

use crate::components::ai::{
    format_mcp_tool_display_name, has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview,
    AiQuestion, Answer, AutoContextChip, QuestionBlock, QuestionBlockState,
};
use crate::i18n::{has, t};
use crate::icons::ICON_SEND;

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

/// The `states` array of the a11y node for `key`, if the node exists at all.
/// Empty when the node is absent or carries no states.
fn a11y_states<S>(rt: &Runtime<S>, key: &str) -> Vec<String> {
    let json = rt.accessibility_json();
    let Some(node) = find_json(&json, key) else {
        return Vec::new();
    };
    node.get("states")
        .and_then(|v| v.as_array())
        .map(|states| {
            states
                .iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
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

/// Fresh fixture: different ids, prompts and labels from the coder's, so the
/// key assertions prove derivation (`question-option-{q}-{opt}`) rather than
/// a hardcoded list.
fn audit_questions() -> Vec<AiQuestion> {
    vec![
        AiQuestion::new("d1", "Pick a dialect?")
            .option("pg", "Postgres")
            .option("my", "MySQL"),
        AiQuestion::new("d2", "Pick tasks?")
            .allow_multiple(true)
            .option("m", "Migrate")
            .option("i", "Inspect"),
        AiQuestion::new("d3", "Notes?"),
    ]
}

#[derive(Default)]
struct AuditProbe {
    block: QuestionBlockState,
    removed: Vec<String>,
    submitted_payloads: Vec<String>,
}

fn probe() -> AuditProbe {
    let mut state = AuditProbe::default();
    state.block = QuestionBlockState::new(&audit_questions());
    state
}

fn block_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let questions = audit_questions();
    col()
        .key("audit-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            QuestionBlock::<AuditProbe>::new(questions, &state.block)
                .on_toggle(|s: &mut AuditProbe, qid, opt, multi| s.block.toggle(qid, opt, multi))
                .on_custom_edit(|s: &mut AuditProbe, qid: &str| s.block.custom_edit(qid))
                .on_submit(|s: &mut AuditProbe| {
                    if let Some(payload) = s.block.submit(&audit_questions()) {
                        s.submitted_payloads.push(payload);
                    }
                })
                .build(true),
        )
}

fn chip_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let _ = state;
    col()
        .key("audit-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(8.0)
        .child(
            AutoContextChip::<AuditProbe>::new(t("chat.autoContext.error"), "ai-auto-context-error")
                .on_remove(|s: &mut AuditProbe, id| s.removed.push(id.to_string()))
                .build(true),
        )
        .child(
            AutoContextChip::<AuditProbe>::new(
                t("chat.autoContext.result"),
                "ai-auto-context-result",
            )
            .on_remove(|s: &mut AuditProbe, id| s.removed.push(id.to_string()))
            .build(true),
        )
}

fn long_chip_view(_state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    col()
        .key("audit-page")
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            AutoContextChip::<AuditProbe>::new(
                "x".repeat(200),
                "audit-long-chip",
            )
            .build(true),
        )
}

// ---- formatMcpToolDisplayName: fresh vectors -------------------------------

#[test]
fn audit_format_shows_from_the_third_segment_on() {
    // React: `parts.length >= 3 ? parts.slice(2).join('/') : qualifiedName`.
    assert_eq!(
        format_mcp_tool_display_name("server/tool/name"),
        "name"
    );
    assert_eq!(format_mcp_tool_display_name("a/b/c/d/e"), "c/d/e");
    assert_eq!(format_mcp_tool_display_name("a//c"), "c");
    assert_eq!(format_mcp_tool_display_name("//"), "");
    assert_eq!(format_mcp_tool_display_name("a/b"), "a/b");
    assert_eq!(format_mcp_tool_display_name(""), "");
}

// ---- hasSnapshotContent: fresh vectors --------------------------------------

fn snapshot() -> AiChatContextSnapshot {
    AiChatContextSnapshot::default()
}

#[test]
fn audit_snapshot_content_counts_trimmed_fields_and_preview_lengths() {
    // React trims each string field; a preview counts on columns OR rows.
    assert!(!has_snapshot_content(None));
    assert!(!has_snapshot_content(Some(&snapshot())));

    let mut nbsp = snapshot();
    nbsp.selection = Some("\u{a0} ".to_string());
    assert!(
        !has_snapshot_content(Some(&nbsp)),
        "whitespace-only (incl. NBSP) does not count"
    );

    let mut sel = snapshot();
    sel.selection = Some("  x ".to_string());
    assert!(has_snapshot_content(Some(&sel)));

    let mut err = snapshot();
    err.last_error = Some("boom".to_string());
    assert!(has_snapshot_content(Some(&err)));

    let mut cols = snapshot();
    cols.result_preview = Some(AiChatResultPreview {
        columns: vec!["a".to_string(), "b".to_string()],
        rows: vec![],
    });
    assert!(has_snapshot_content(Some(&cols)));

    let mut rows = snapshot();
    rows.result_preview = Some(AiChatResultPreview {
        columns: vec![],
        rows: vec![vec![json!("v")], vec![json!(2)]],
    });
    assert!(has_snapshot_content(Some(&rows)));

    // Micro-divergence, recorded not failed: JS `trim()` also strips U+FEFF,
    // Rust `str::trim` does not. No test pins FEFF either way.
    let mut feff = snapshot();
    feff.editor_sql = Some("\u{feff}".to_string());
    assert!(
        has_snapshot_content(Some(&feff)),
        "documented: Rust trim keeps U+FEFF, JS trim would drop it"
    );
}

// ---- keys: React data-testid values, derived from unseen ids ---------------

#[test]
fn audit_question_keys_match_react_testids_for_unseen_ids() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in [
        "question-block",
        "question-block-item-d1",
        "question-block-item-d2",
        "question-block-item-d3",
        "question-option-d1-pg",
        "question-option-d1-my",
        "question-option-d2-m",
        "question-option-d2-i",
        "question-custom-input-d1",
        "question-custom-input-d2",
        "question-custom-input-d3",
        "question-submit",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    let texts: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect();
    for prompt in ["Pick a dialect?", "Pick tasks?", "Notes?"] {
        assert!(texts.iter().any(|text| text == prompt), "prompt {prompt:?} rendered");
    }
    assert!(
        texts.iter().any(|text| text == "提交回答"),
        "submit button carries chat.questions.submit"
    );
}

// ---- i18n: keys exist in zh-CN/ai.ts with these exact values ---------------

#[test]
fn audit_question_i18n_keys_resolve_to_the_locale_values() {
    // Values read from packages/frontend/src/locales/zh-CN/ai.ts:168-169.
    assert!(has("chat.questions.submit"));
    assert!(has("chat.questions.customAnswer"));
    assert_eq!(t("chat.questions.submit"), "提交回答");
    assert_eq!(t("chat.questions.customAnswer"), "输入自定义回答…");
}

// ---- AutoContextChip: template is generic, not hardcoded -------------------

#[test]
fn audit_chip_remove_label_uses_the_react_template_for_other_fields() {
    let mut rt = Runtime::new(probe(), chip_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "ai-auto-context-error"));
    assert!(has_key(&rt, "ai-auto-context-error-remove"));
    assert!(has_key(&rt, "ai-auto-context-result"));
    assert_eq!(
        a11y_label(&rt, "ai-auto-context-error-remove").as_deref(),
        Some("remove 最近错误"),
        "aria-label is React's `remove {{label}}` template"
    );
    assert_eq!(
        a11y_label(&rt, "ai-auto-context-result-remove").as_deref(),
        Some("remove 结果预览")
    );
    click(&mut rt, "ai-auto-context-result-remove");
    assert_eq!(
        rt.state.removed,
        vec!["ai-auto-context-result".to_string()],
        "only the pressed chip reports, with its own testId"
    );
}

// ---- selection observability: layout + paint carry it, a11y does not ------

#[test]
fn audit_selected_option_is_visible_through_layout_state_and_paint() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(!element(&rt, "question-option-d1-pg").item_selected);
    click(&mut rt, "question-option-d1-pg");
    assert_eq!(
        rt.state.block.answer("d1"),
        Some(&Answer::Single("pg".to_string()))
    );
    let selected = element(&rt, "question-option-d1-pg");
    assert!(selected.item_selected, "layout tree reports the pick");
    assert_ne!(
        selected.fill,
        Color::TRANSPARENT,
        "the picked option paints its accent wash"
    );
    // Known Rune gap, pinned: no `aria-pressed` hook and Button is not a
    // `selectable_item`, so the a11y tree carries no selected state.
    assert!(
        !a11y_states(&rt, "question-option-d1-pg").contains(&"selected".to_string()),
        "a11y tree reports no selected state for the option button"
    );
    let sibling = element(&rt, "question-option-d1-my");
    assert!(!sibling.item_selected);
    assert_eq!(sibling.fill, Color::TRANSPARENT);
}

// ---- state machine: toggle semantics ---------------------------------------

#[test]
fn audit_single_toggle_switches_and_clears_multi_toggle_keeps_order() {
    let mut state = QuestionBlockState::new(&audit_questions());
    state.toggle("d1", "pg", false);
    state.toggle("d1", "my", false);
    assert_eq!(state.answer("d1"), Some(&Answer::Single("my".to_string())));
    state.toggle("d1", "my", false);
    assert_eq!(state.answer("d1"), Some(&Answer::Single(String::new())));

    state.toggle("d2", "m", true);
    state.toggle("d2", "i", true);
    assert_eq!(
        state.answer("d2"),
        Some(&Answer::Multiple(vec!["m".to_string(), "i".to_string()]))
    );
    state.toggle("d2", "m", true);
    assert_eq!(
        state.answer("d2"),
        Some(&Answer::Multiple(vec!["i".to_string()]))
    );
}

// ---- submission format: React handleSubmit, line by line -------------------

#[test]
fn audit_submission_format_matches_react_line_by_line() {
    use std::collections::HashMap;
    let questions = audit_questions();
    let answers: HashMap<String, Answer> = [
        ("d1".to_string(), Answer::Single("pg".to_string())),
        (
            "d2".to_string(),
            Answer::Multiple(vec!["m".to_string(), "zzz".to_string()]),
        ),
        ("d3".to_string(), Answer::Single("free text".to_string())),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        QuestionBlockState::format_submission(&questions, &answers).as_deref(),
        Some(
            "[d1] Pick a dialect?\nPostgres\n\n[d2] Pick tasks?\nMigrate, zzz\n\n[d3] Notes?\nfree text"
        ),
        "one bracketed-id line per answer, joined with a blank line; \
         unknown ids fall back to themselves"
    );
    assert_eq!(
        QuestionBlockState::format_submission(&questions, &HashMap::new()),
        None,
        "all empty yields None, so submit stays unsubmitted"
    );
}

// ---- submit: draft sync + empty shell --------------------------------------

#[test]
fn audit_submit_syncs_the_draft_and_empties_to_a_zero_height_shell() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    click(&mut rt, "question-custom-input-d3");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "question-submit");
    assert_eq!(
        rt.state.submitted_payloads,
        vec!["[d3] Notes?\nhello".to_string()]
    );
    assert!(rt.state.block.submitted());
    // React returns null; Rune keeps the keyed shell with no children.
    assert!(has_key(&rt, "question-block"), "shell keeps its key");
    for key in [
        "question-block-item-d1",
        "question-block-item-d2",
        "question-block-item-d3",
        "question-option-d1-pg",
        "question-custom-input-d3",
        "question-submit",
    ] {
        assert!(!has_key(&rt, key), "{key} is gone after submit");
    }
    // The shell does NOT vanish instantly: Rune animates the keyed element's
    // collapse (288.5pt before submit -> 274.5pt on the submit frame, then
    // ~x0.74 per frame). It settles toward zero height but its width stays
    // full the whole time, so the port's "zero-size" wording is imprecise.
    let shell_now = element(&rt, "question-block");
    assert!(
        shell_now.rect[3] < 288.5,
        "shell is already collapsing on the submit frame: {:?}",
        shell_now.rect
    );
    assert_eq!(
        shell_now.rect[2], 928.0,
        "shell width stays full (w_full) while it collapses: {:?}",
        shell_now.rect
    );
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let shell_settled = element(&rt, "question-block");
    assert!(
        shell_settled.rect[3] <= 0.5,
        "shell settles to ~zero height after the collapse: {:?}",
        shell_settled.rect
    );
}

// ---- deliberate divergence: option press after free text in multi ----------

#[test]
fn audit_option_press_after_free_text_in_multi_replaces_the_draft() {
    let mut state = QuestionBlockState::new(&audit_questions());
    state.custom_edit("d2").text = "something else".to_string();
    state.toggle("d2", "m", true);
    assert_eq!(
        state.answer("d2"),
        Some(&Answer::Multiple(vec!["m".to_string()])),
        "the pick replaces the typed draft instead of spreading it"
    );
    assert_eq!(state.custom_text("d2"), "", "the draft is cleared");
    assert_eq!(
        state.submit(&audit_questions()).as_deref(),
        Some("[d2] Pick tasks?\nMigrate")
    );
}

// ---- keyboard: Enter/Space toggle exactly once each ------------------------

#[test]
fn audit_tab_enter_and_space_toggle_exactly_once_each() {
    let mut rt = Runtime::new(probe(), block_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("question-option-d1-pg"),
        "Tab reaches the first option button"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.block.answer("d1"),
        Some(&Answer::Single("pg".to_string()))
    );
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(
        rt.state.block.answer("d1"),
        Some(&Answer::Single(String::new())),
        "Space toggles it back exactly once"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.block.answer("d1"),
        Some(&Answer::Single("pg".to_string()))
    );
}

// ---- ICON_SEND: registry entry is the lucide send glyph --------------------

#[test]
fn audit_icon_send_is_the_lucide_send_glyph() {
    assert_eq!(ICON_SEND.name, "send");
    assert_eq!(ICON_SEND.view, 24.0);
    assert!(
        ICON_SEND.body.contains("m22 2-7 20-4-9-9-4Z"),
        "paper-plane path: {}",
        ICON_SEND.body
    );
    assert!(
        ICON_SEND.body.contains("M22 2 11 13"),
        "diagonal path: {}",
        ICON_SEND.body
    );
}

// ---- discipline: no bare unwrap/expect, no oklch in the new product code ---

#[test]
fn audit_product_source_has_no_bare_unwrap_expect_or_oklch() {
    let src = include_str!("../components/ai/ai_chat_panel.rs");
    // Doc comments explain the port; only code matters here.
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for literal in ["unwrap()", "expect(", "oklch(", "oklcha("] {
        assert!(
            !code.contains(literal),
            "`{literal}` must not appear in the new product code"
        );
    }
}

// ---- geometry: both sizes x both themes, plus a long-label chip -----------

#[test]
fn audit_block_and_chips_stay_inside_both_windows_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = Runtime::new(probe(), block_view, (w, h), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("question block {w}x{h} dark={dark}"));
            let submit = element(&rt, "question-submit");
            assert!(
                submit.rect[0] + submit.rect[2] <= w + 0.5,
                "submit button right edge inside {w}: {:?}",
                submit.rect
            );
            let mut chips = Runtime::new(probe(), chip_view, (w, h), env(dark));
            chips.frame(0.0);
            assert_in_window(&chips, w, h, &format!("chips {w}x{h} dark={dark}"));
        }
    }
}

#[test]
fn audit_a_very_long_chip_label_is_capped_and_stays_in_window() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(probe(), long_chip_view, (w, h), env(true));
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("long chip {w}x{h}"));
        let chip = element(&rt, "audit-long-chip");
        assert!(
            chip.rect[2] <= 560.5,
            "chip honours max_w 560: rect {:?}",
            chip.rect
        );
        assert!(
            chip.rect[0] + chip.rect[2] <= w + 0.5,
            "chip right edge inside {w}: {:?}",
            chip.rect
        );
    }
}

