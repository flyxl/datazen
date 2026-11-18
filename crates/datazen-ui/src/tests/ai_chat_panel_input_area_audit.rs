//! T009-S5 independent audit: AiChatPanel input area
//! (`AiChatPanel.tsx:777–949`, `AiEgressNotice.tsx`, `AiInput.tsx:70,162–164,257–289`).
//!
//! Fresh vectors, written against the React source and the React zh-CN
//! locale packs — not against the coder's test file. Rendered strings are
//! pinned against the literal zh-CN text from
//! `packages/frontend/src/locales/zh-CN/ai.ts` / `core.ts` /
//! `connection.ts`, and the amber wash against literal Tailwind oklch
//! values, so a wrong table entry or a wrong palette entry cannot hide
//! behind a self-referential comparison.
//!
//! Geometry is asserted on `rt.elements()` rects, which come from the
//! layout engine, not from the code under test; help text is read off
//! `ElementInfo.help`, and the accessible names off
//! `rt.accessibility_json()`.
//!
//! Divergences pinned here on purpose (non-blocking; see the audit
//! report): D1 — a whitespace-only template name hides the template /
//! priority chips, where React's truthy check would show them; D2 — a
//! whitespace-only snapshot field gets no chip, where React's untrimmed
//! chip gate would show one once the row is up; D3 — the field box stays
//! at full opacity while streaming, where React dims it (`opacity-50`);
//! D5 — the egress summary / message lines render at full amber-200,
//! where React uses amber-200/90.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiChatContextSnapshot, AiChatInputArea, AiChatInputAreaState, AiChatResultPreview, AiInjection,
    AiLoadedSkill, AiPendingDraft, ConflictResolution,
};

// Literal zh-CN text from packages/frontend/src/locales/zh-CN/ai.ts,
// core.ts and connection.ts — pinned here, not through `t()`.
const SKILLS_LOADED_3: &str = "已加载技能：3";
const PLACEHOLDER: &str = "输入消息，询问关于数据库的问题…";
const STOP: &str = "停止生成";
const AUTO_SQL: &str = "当前 SQL";
const AUTO_SELECTION: &str = "选区";
const AUTO_ERROR: &str = "最近错误";
const AUTO_RESULT: &str = "结果预览";
const INJECTION_TEMPLATE_TMPL: &str = "模板：Tmpl";
const INJECTION_NOTES: &str = "已包含连接备注";
const INJECTION_PRIORITY: &str = "全局指令优先";
const INJECTION_READONLY_HINT: &str = "该注入内容随消息冻结，只读展示";
const INJECTION_PRIORITY_HINT: &str = "全局指令与模板冲突时，以全局指令为准。";
const EGRESS_RELAXED: &str = "严格出域已关闭——查询结果、附加文件与工具输出可能会发送给 AI 提供商。";
const EGRESS_CONTEXT: &str = "附加的 @ 上下文将包含在发往 AI 提供商的请求中。";
const DRAFT_CONFLICT: &str = "输入框中已有未发送的文本，如何处理草稿？";
const DRAFT_REPLACE: &str = "替换";
const DRAFT_APPEND: &str = "追加";
const DRAFT_CANCEL: &str = "取消";
const APPEND_SEPARATOR: &str = "\n\n---\n\n";
const KV_ATTACHED: &str = "正在询问的键";

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

fn focused_key<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

/// Tab through the focus order until `key` is focused (at most 60 stops),
/// so keyboard tests do not depend on the exact tab order.
fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 60 Tabs");
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Every laid-out element stays inside the window horizontally.
fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) horizontally out of window {w}: {outside:?}",
        outside.len()
    );
}

// ---- fixtures ---------------------------------------------------------------

fn full_snapshot() -> AiChatContextSnapshot {
    AiChatContextSnapshot {
        editor_sql: Some("select 1".to_string()),
        selection: Some("x".to_string()),
        last_error: Some("boom".to_string()),
        result_preview: Some(AiChatResultPreview {
            columns: vec!["a".to_string()],
            rows: Vec::new(),
        }),
    }
}

fn full_injection() -> AiInjection {
    AiInjection {
        template_name: Some("Tmpl".to_string()),
        has_connection_notes: true,
        has_global_instructions: true,
    }
}

#[derive(Default)]
struct Probe {
    area: AiChatInputAreaState,
    dark: bool,
    submitted: Vec<String>,
    stops: usize,
    removed_fields: Vec<String>,
    resolved: Vec<(ConflictResolution, String)>,
    dismissed_conflicts: usize,
}

fn probe() -> Probe {
    Probe {
        area: AiChatInputAreaState::default(),
        dark: true,
        ..Probe::default()
    }
}

fn full_probe() -> Probe {
    let mut p = probe();
    p.area.set_egress(
        Some("host summary".to_string()),
        Some("orders".to_string()),
        true,
        true,
    );
    p.area.set_loaded_skills(vec![
        AiLoadedSkill::new("sql-review").id("k9"),
        AiLoadedSkill::new("explain"),
        AiLoadedSkill::new("migrate"),
    ]);
    p.area.set_snapshot(Some(full_snapshot()));
    p.area.set_injection(Some(full_injection()));
    p.area.set_kv_key_name(Some("user:1001".to_string()));
    p.area
        .set_pending_conflict(Some(AiPendingDraft::new("req-7", "draft text")));
    p
}

fn area_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(
                AiChatInputArea::<Probe>::new(&state.area)
                    .on_input_edit(|s: &mut Probe| s.area.input_edit())
                    .on_submit(|s: &mut Probe| {
                        if let Some(text) = s.area.submit() {
                            s.submitted.push(text);
                        }
                    })
                    .on_stop(|s: &mut Probe| {
                        s.stops += 1;
                    })
                    .on_remove_snapshot_field(|s: &mut Probe, field: &str| {
                        s.removed_fields.push(field.to_string());
                        s.area.remove_snapshot_field(field);
                    })
                    .on_resolve_conflict(|s: &mut Probe, strategy| {
                        if let Some(request_id) = s.area.resolve_conflict(strategy) {
                            s.resolved.push((strategy, request_id));
                        }
                    })
                    .on_dismiss_conflict(|s: &mut Probe| {
                        s.dismissed_conflicts += 1;
                        s.area.dismiss_conflict();
                    })
                    .build(state.dark),
            ),
        Vec::new(),
    )
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, area_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

/// The same area with no button / chip callbacks wired at all.
fn bare_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(
                AiChatInputArea::<Probe>::new(&state.area)
                    .on_input_edit(|s: &mut Probe| s.area.input_edit())
                    .build(state.dark),
            ),
        Vec::new(),
    )
}

fn bare_runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, bare_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- egress notice: literal text, gates, wrap -------------------------------

#[test]
fn audit_egress_summary_only_renders_no_database_line() {
    let mut p = probe();
    p.area
        .set_egress(Some("host summary".to_string()), None, false, false);
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-egress-notice"));
    assert_eq!(element_text(&rt, "ai-egress-summary"), "host summary");
    assert!(!has_key(&rt, "ai-egress-database"));
    assert!(!has_key(&rt, "ai-egress-message-0"));
}

#[test]
fn audit_egress_relaxed_only_shows_relaxed_first_with_database() {
    let mut p = probe();
    p.area
        .set_egress(None, Some("orders".to_string()), true, false);
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-egress-database"), "数据库：orders");
    assert_eq!(element_text(&rt, "ai-egress-message-0"), EGRESS_RELAXED);
    assert!(
        !has_key(&rt, "ai-egress-message-1"),
        "no context note without context files"
    );
}

#[test]
fn audit_egress_context_only_message_is_the_context_note() {
    let mut p = probe();
    p.area.set_egress(None, None, false, true);
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-egress-message-0"), EGRESS_CONTEXT);
    assert!(!has_key(&rt, "ai-egress-summary"));
    assert!(!has_key(&rt, "ai-egress-database"));
}

#[test]
fn audit_egress_blank_database_hides_the_line_but_keeps_the_notice() {
    let mut p = probe();
    p.area.set_egress(
        Some("host summary".to_string()),
        Some("   ".to_string()),
        false,
        false,
    );
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-egress-notice"));
    assert!(!has_key(&rt, "ai-egress-database"));
}

#[test]
fn audit_egress_wrap_present_for_skills_alone_and_notice_alone() {
    let mut p = probe();
    p.area
        .set_loaded_skills(vec![AiLoadedSkill::new("explain")]);
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-input-egress-wrap"));
    assert!(has_key(&rt, "ai-loaded-skills"));
    assert!(!has_key(&rt, "ai-egress-notice"));

    let mut p = probe();
    p.area.set_egress(None, None, true, false);
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-input-egress-wrap"));
    assert!(has_key(&rt, "ai-egress-notice"));
    assert!(!has_key(&rt, "ai-loaded-skills"));
}

#[test]
fn audit_egress_amber_is_tailwind_amber_not_the_palette_self_report() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    // Tailwind amber-500 at 10% fill, amber-200 text — pinned as oklch
    // literals, not through the `ai_amber_*` helpers under test.
    assert_eq!(
        element(&rt, "ai-egress-notice").fill,
        rune::oklcha(0.769, 0.164, 70.08, 0.10)
    );
    assert_eq!(
        element(&rt, "ai-egress-summary").text_color,
        Some(rune::oklch(0.924, 0.12, 95.08))
    );
    assert_eq!(
        element(&rt, "draft-conflict-bar").fill,
        rune::oklcha(0.769, 0.164, 70.08, 0.10),
        "the conflict bar shares the egress wash"
    );
}

// ---- loaded skills ----------------------------------------------------------

#[test]
fn audit_skills_count_three_and_id_keyed_chips() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-loaded-skills-label"), SKILLS_LOADED_3);
    assert_eq!(element_text(&rt, "ai-loaded-skill-k9-label"), "sql-review");
    assert_eq!(
        element_text(&rt, "ai-loaded-skill-explain-label"),
        "explain"
    );
    assert_eq!(
        element_text(&rt, "ai-loaded-skill-migrate-label"),
        "migrate"
    );
}

// ---- auto-context chips -----------------------------------------------------

#[test]
fn audit_auto_context_labels_are_the_literal_zh_cn_strings() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert!(any_text(&rt, AUTO_SQL));
    assert!(any_text(&rt, AUTO_SELECTION));
    assert!(any_text(&rt, AUTO_ERROR));
    assert!(any_text(&rt, AUTO_RESULT));
}

#[test]
fn audit_auto_context_whitespace_field_gets_no_chip_once_row_shows() {
    // D2 (pinned, non-blocking): React's chip gate is an untrimmed truthy
    // check, so a whitespace-only `editorSql` would still get a chip once
    // another field puts the row up. Rune trims at chip level too.
    let mut p = probe();
    p.area.set_snapshot(Some(AiChatContextSnapshot {
        editor_sql: Some("   ".to_string()),
        selection: Some("x".to_string()),
        last_error: None,
        result_preview: None,
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-auto-context-chips"));
    assert!(has_key(&rt, "ai-auto-context-selection"));
    assert!(
        !has_key(&rt, "ai-auto-context-sql"),
        "D2: whitespace-only editorSql renders no chip"
    );
}

#[test]
fn audit_auto_context_result_preview_rows_only_still_shows() {
    let mut p = probe();
    p.area.set_snapshot(Some(AiChatContextSnapshot {
        editor_sql: None,
        selection: None,
        last_error: None,
        result_preview: Some(AiChatResultPreview {
            columns: Vec::new(),
            rows: vec![vec![serde_json::json!(1)]],
        }),
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-auto-context-result"));
}

#[test]
fn audit_auto_context_empty_result_preview_hides_row() {
    let mut p = probe();
    p.area.set_snapshot(Some(AiChatContextSnapshot {
        editor_sql: None,
        selection: None,
        last_error: None,
        result_preview: Some(AiChatResultPreview {
            columns: Vec::new(),
            rows: Vec::new(),
        }),
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(!has_key(&rt, "ai-auto-context-chips"));
}

#[test]
fn audit_auto_context_each_remove_reports_its_own_field() {
    let mut rt = runtime(full_probe(), 960.0, 640.0);
    click(&mut rt, "ai-auto-context-selection-remove");
    assert_eq!(rt.state.removed_fields, vec!["selection".to_string()]);
    click(&mut rt, "ai-auto-context-error-remove");
    click(&mut rt, "ai-auto-context-result-remove");
    assert_eq!(
        rt.state.removed_fields,
        vec![
            "selection".to_string(),
            "lastError".to_string(),
            "resultPreview".to_string(),
        ]
    );
    assert!(has_key(&rt, "ai-auto-context-sql"));
}

// ---- injection chips --------------------------------------------------------

#[test]
fn audit_injection_notes_only_and_global_only_rows() {
    let mut p = probe();
    p.area.set_injection(Some(AiInjection {
        template_name: None,
        has_connection_notes: true,
        has_global_instructions: false,
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-injection-chips"));
    assert_eq!(element_text(&rt, "ai-injection-notes"), INJECTION_NOTES);
    assert!(!has_key(&rt, "ai-injection-template"));
    assert!(!has_key(&rt, "ai-injection-priority"));

    // Global instructions alone still show the row — with no chips at all,
    // exactly as React renders the empty div.
    let mut p = probe();
    p.area.set_injection(Some(AiInjection {
        template_name: None,
        has_connection_notes: false,
        has_global_instructions: true,
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-injection-chips"));
    assert!(!has_key(&rt, "ai-injection-template"));
    assert!(!has_key(&rt, "ai-injection-notes"));
    assert!(!has_key(&rt, "ai-injection-priority"));
}

#[test]
fn audit_injection_blank_template_hides_template_and_priority() {
    // D1 (pinned, non-blocking): React's truthy check would show both
    // chips for a whitespace-only template name; Rune treats it as absent.
    let mut p = probe();
    p.area.set_injection(Some(AiInjection {
        template_name: Some("   ".to_string()),
        has_connection_notes: false,
        has_global_instructions: true,
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-injection-chips"));
    assert!(
        !has_key(&rt, "ai-injection-template"),
        "D1: blank template name renders no template chip"
    );
    assert!(
        !has_key(&rt, "ai-injection-priority"),
        "D1: blank template name renders no priority chip"
    );
}

#[test]
fn audit_injection_chip_help_carries_the_react_title_text() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-injection-template"),
        INJECTION_TEMPLATE_TMPL
    );
    assert_eq!(
        element_text(&rt, "ai-injection-priority"),
        INJECTION_PRIORITY
    );
    // The `title` tooltips ride `El::help` — read off the laid-out element,
    // not the component's own report.
    assert_eq!(
        element(&rt, "ai-injection-template").help.as_deref(),
        Some(INJECTION_READONLY_HINT)
    );
    assert_eq!(
        element(&rt, "ai-injection-notes").help.as_deref(),
        Some(INJECTION_READONLY_HINT)
    );
    assert_eq!(
        element(&rt, "ai-injection-priority").help.as_deref(),
        Some(INJECTION_PRIORITY_HINT)
    );
}

// ---- KV chip ----------------------------------------------------------------

#[test]
fn audit_kv_chip_literal_label_and_long_key_stays_inside() {
    let mut p = full_probe();
    p.area.set_kv_key_name(Some(format!(
        "user:{}:session:{}",
        "x".repeat(60),
        "y".repeat(60)
    )));
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-kv-context-chip-label"), KV_ATTACHED);
    let key_rect = element(&rt, "ai-kv-context-chip-key").rect;
    assert!(
        key_rect[0] >= -0.5 && key_rect[0] + key_rect[2] <= 960.5,
        "a 130-char key stays inside 960: {key_rect:?}"
    );
    let chip_rect = element(&rt, "ai-kv-context-chip").rect;
    assert!(
        chip_rect[0] + chip_rect[2] <= 960.5,
        "the kv chip stays inside 960: {chip_rect:?}"
    );
}

// ---- conflict bar: keyboard paths, literal text -----------------------------

#[test]
fn audit_conflict_bar_literal_text_and_enter_keys() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(element_text(&rt, "draft-conflict-bar-text"), DRAFT_CONFLICT);
    assert_eq!(
        a11y_label(&rt, "draft-replace").as_deref(),
        Some(DRAFT_REPLACE)
    );
    assert_eq!(
        a11y_label(&rt, "draft-append").as_deref(),
        Some(DRAFT_APPEND)
    );
    assert_eq!(
        a11y_label(&rt, "draft-cancel").as_deref(),
        Some(DRAFT_CANCEL)
    );

    // Enter on the focused replace button resolves with the request id.
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "draft-replace");
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.resolved,
        vec![(ConflictResolution::Replace, "req-7".to_string())]
    );
    assert_eq!(rt.state.area.input_text(), "draft text");
    assert!(!has_key(&rt, "draft-conflict-bar"));

    // Enter on append joins with the literal separator.
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "draft-append");
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.resolved,
        vec![(ConflictResolution::Append, "req-7".to_string())]
    );
    assert_eq!(
        rt.state.area.input_text(),
        format!("keep me{APPEND_SEPARATOR}draft text")
    );
}

#[test]
fn audit_conflict_cancel_via_enter_keeps_the_input() {
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "draft-cancel");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.dismissed_conflicts, 1);
    assert!(rt.state.resolved.is_empty());
    assert_eq!(rt.state.area.input_text(), "keep me");
    assert!(!has_key(&rt, "draft-conflict-bar"));
}

// ---- input row: send / stop, placeholder, geometry --------------------------

#[test]
fn audit_send_button_has_no_accessible_name_like_react() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "ai-input-submit"),
        None,
        "React's submit button carries no aria-label or title, so neither does this one"
    );
}

#[test]
fn audit_stop_button_enter_key_stops_and_keeps_the_stop_label() {
    let mut p = probe();
    p.area.set_input("hello");
    p.area.set_streaming(true);
    let mut rt = runtime(p, 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "ai-input-stop").as_deref(),
        Some(STOP),
        "the stop button keeps React's title as its accessible name"
    );
    focus_by_tab(&mut rt, "ai-input-stop");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.stops, 1);
}

#[test]
fn audit_streaming_field_still_edits_but_enter_submits_nothing() {
    // D3's editable half (pinned, documented): React disables the textarea
    // while streaming; the placeholder keeps it editable and only the
    // submit path is gated.
    let mut p = probe();
    p.area.set_input("hello");
    p.area.set_streaming(true);
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste(" world");
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        rt.state.area.input_text(),
        "hello world",
        "the field stays editable while streaming"
    );
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.submitted.is_empty(),
        "Enter while streaming submits nothing"
    );
    // D3's visual half: no `opacity-50` dim on the field box.
    assert_eq!(
        element(&rt, "ai-input-field").opacity,
        1.0,
        "D3: the field box is not dimmed while streaming"
    );
}

#[test]
fn audit_double_send_submits_once() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "ai-input-submit");
    click(&mut rt, "ai-input-submit");
    assert_eq!(
        rt.state.submitted,
        vec!["hello".to_string()],
        "the second press finds an empty, disabled send"
    );
}

#[test]
fn audit_placeholder_accessible_name_follows_context_items() {
    // `AiInput` shows `chat.placeholder` only while no context tokens are
    // attached. The placeholder lives on the kit inner input (`input`): its
    // accessible name and its `placeholder` both carry the literal text.
    // The outer `ai-input-text` group carries no name — the name is the
    // inner field's, which is the observable channel.
    let rt = runtime(probe(), 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "input").as_deref(),
        Some(PLACEHOLDER),
        "the empty field names itself with the placeholder"
    );
    let mut p = probe();
    p.area.set_context_items_count(2);
    let rt = runtime(p, 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "input"),
        None,
        "with context tokens attached the placeholder is suppressed"
    );
}

#[test]
fn audit_non_enter_keys_on_focused_buttons_do_nothing() {
    // Closes the `false` arms of the send / stop `on_key` handlers.
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    focus_by_tab(&mut rt, "ai-input-submit");
    press(&mut rt, Key::Escape);
    assert!(
        rt.state.submitted.is_empty(),
        "Escape on the focused send button submits nothing"
    );
    assert!(has_key(&rt, "ai-input-submit"));

    let mut p = probe();
    p.area.set_input("hello");
    p.area.set_streaming(true);
    let mut rt = runtime(p, 960.0, 640.0);
    focus_by_tab(&mut rt, "ai-input-stop");
    press(&mut rt, Key::Escape);
    assert_eq!(
        rt.state.stops, 0,
        "Escape on the focused stop button stops nothing"
    );
}

#[test]
fn audit_input_row_height_and_send_button_centering() {
    for (w, h, dark) in [(960.0, 640.0, true), (1440.0, 816.0, false)] {
        let mut p = full_probe();
        p.dark = dark;
        let rt = runtime(p, w, h);
        // 36pt field + py-2 (8 + 8): the row is 52pt tall.
        let field = element(&rt, "ai-input-field").rect;
        assert!(
            (51.0..=53.0).contains(&field[3]),
            "{w}x{h}: input row height is 52, got {field:?}"
        );
        let send = element(&rt, "ai-input-submit").rect;
        let field_cy = field[1] + field[3] / 2.0;
        let send_cy = send[1] + send[3] / 2.0;
        assert!(
            (field_cy - send_cy).abs() <= 1.5,
            "{w}x{h}: send button is vertically centred in the row: field {field:?} send {send:?}"
        );
    }
}

#[test]
fn audit_sections_stack_in_order_without_overlap() {
    for (w, h, dark) in [(960.0, 640.0, true), (1440.0, 816.0, false)] {
        let mut p = full_probe();
        p.dark = dark;
        p.area
            .set_input("a long draft that keeps going so the field has content to lay out");
        let rt = runtime(p, w, h);
        assert_no_horizontal_overflow(&rt, w, "audit full input area");
        let keys = [
            "ai-chat-input-egress-wrap",
            "ai-auto-context-chips",
            "ai-injection-chips",
            "ai-kv-context-chip",
            "draft-conflict-bar",
            "ai-input",
        ];
        let mut prev_bottom = f32::MIN;
        for key in keys {
            let r = element(&rt, key).rect;
            assert!(
                r[1] >= prev_bottom - 0.5,
                "{w}x{h}: {key} overlaps the section above it: {r:?} after {prev_bottom}"
            );
            assert!(
                r[1] + r[3] <= h + 0.5,
                "{w}x{h}: {key} runs past the bottom: {r:?}"
            );
            prev_bottom = r[1] + r[3];
        }
    }
}

#[test]
fn audit_bare_area_remove_is_unwired_and_streaming_keeps_send() {
    let mut p = full_probe();
    p.area.set_streaming(true);
    let mut rt = bare_runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-auto-context-sql"));
    // S1's chip builds the remove button unconditionally; only the press
    // wiring is conditional — so the button renders unwired here and its
    // press reports nothing.
    assert!(has_key(&rt, "ai-auto-context-sql-remove"));
    click(&mut rt, "ai-auto-context-sql-remove");
    assert!(
        rt.state.removed_fields.is_empty(),
        "an unwired remove press reports nothing"
    );
    assert!(has_key(&rt, "ai-auto-context-sql"));
    // Streaming with no `on_stop` wired: the send button stays (React's
    // `showStop = isLoading && onStop` is false), disabled by `can_send`.
    assert!(has_key(&rt, "ai-input-submit"));
    assert!(!has_key(&rt, "ai-input-stop"));
}

// ---- pure logic: fresh vectors ----------------------------------------------

#[test]
fn audit_submit_trims_tabs_and_newlines() {
    let mut state = AiChatInputAreaState::default();
    state.set_input("\t hello \n");
    assert_eq!(state.submit().as_deref(), Some("hello"));
    assert_eq!(state.input_text(), "");
}

#[test]
fn audit_resolve_append_on_empty_input_starts_with_separator() {
    let mut state = AiChatInputAreaState::default();
    state.set_pending_conflict(Some(AiPendingDraft::new("r9", "draft text")));
    assert_eq!(
        state
            .resolve_conflict(ConflictResolution::Append)
            .as_deref(),
        Some("r9")
    );
    assert_eq!(
        state.input_text(),
        format!("{APPEND_SEPARATOR}draft text"),
        "React joins prev + separator + draft even when prev is empty"
    );
}

#[test]
fn audit_resolve_twice_second_is_none() {
    let mut state = AiChatInputAreaState::default();
    state.set_pending_conflict(Some(AiPendingDraft::new("r9", "draft text")));
    assert!(state
        .resolve_conflict(ConflictResolution::Replace)
        .is_some());
    assert_eq!(
        state.resolve_conflict(ConflictResolution::Replace),
        None,
        "the pending draft is gone after the first resolve"
    );
}

#[test]
fn audit_remove_empty_result_preview_nulls_the_snapshot() {
    let mut state = AiChatInputAreaState::default();
    state.set_snapshot(Some(AiChatContextSnapshot {
        editor_sql: None,
        selection: None,
        last_error: None,
        result_preview: Some(AiChatResultPreview {
            columns: Vec::new(),
            rows: Vec::new(),
        }),
    }));
    assert!(
        !state.show_auto_context(),
        "an empty result preview is no content"
    );
    state.remove_snapshot_field("resultPreview");
    assert!(
        state.snapshot().is_none(),
        "removing the last (empty) field nulls the snapshot"
    );
}

#[test]
fn audit_injection_show_row_matrix() {
    let cases = [
        (None, false, false, false),
        (Some("Tmpl"), false, false, true),
        (None, true, false, true),
        (None, false, true, true),
        (Some("  "), false, false, false),
        (Some("  "), true, false, true),
        (Some("  "), false, true, true),
    ];
    for (template, notes, global, expected) in cases {
        let injection = AiInjection {
            template_name: template.map(str::to_string),
            has_connection_notes: notes,
            has_global_instructions: global,
        };
        assert_eq!(
            injection.shows_row(),
            expected,
            "template={template:?} notes={notes} global={global}"
        );
        assert_eq!(
            injection.shows_priority(),
            global && template.is_some_and(|name| !name.trim().is_empty()),
            "priority needs a non-blank template beside global instructions"
        );
    }
}

#[test]
fn audit_egress_summary_getter_trims_but_keeps_inner_space() {
    let mut state = AiChatInputAreaState::default();
    state.set_egress(
        Some("  a  b  ".to_string()),
        Some("  mydb  ".to_string()),
        false,
        false,
    );
    assert_eq!(state.egress_summary_text(), Some("a  b"));
    assert_eq!(
        state.egress_database_label().as_deref(),
        Some("数据库：mydb")
    );
}
