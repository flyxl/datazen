//! T009-S5: AiChatPanel input area (`AiChatPanel.tsx:777–949`).
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the area in a probe `Runtime` and read back the laid-out element tree
//! — never the component's own report of itself. Geometry is asserted on
//! `rt.elements()` rects, which come from the layout engine, not from the
//! code under test; the amber wash is asserted on the laid-out `fill` /
//! `text_color`, the same dual channel S1's audit used for selection.
//!
//! The area is a view-model placeholder for the `aiStore` slice (T088):
//! the tests seed the state through its `set_*` methods and assert the
//! callbacks report presses, because no store exists to read back yet.
//! The send itself (`composeKvAiMessage`, the snapshot attach, clearing
//! `contextItems`, `cancelChat`, the `onDraftConsumed` ack) is the panel
//! slice's job, so the tests pin the draft handover and the dismiss /
//! resolve reports, not a store round-trip.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::ai::{
    AiChatContextSnapshot, AiChatInputArea, AiChatInputAreaState, AiChatResultPreview, AiInjection,
    AiLoadedSkill, AiPendingDraft, ConflictResolution,
};
use crate::i18n::{has, t};
use crate::ui::palette::{ai_amber_fill, ai_amber_text};

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
        Some("summary text".to_string()),
        Some("mydb".to_string()),
        true,
        true,
    );
    p.area.set_loaded_skills(vec![
        AiLoadedSkill::new("sql-review").id("s1"),
        AiLoadedSkill::new("explain"),
    ]);
    p.area.set_snapshot(Some(full_snapshot()));
    p.area.set_injection(Some(full_injection()));
    p.area.set_kv_key_name(Some("user:1001".to_string()));
    p.area
        .set_pending_conflict(Some(AiPendingDraft::new("r1", "draft text")));
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

/// The same area with no button / chip callbacks wired at all: the
/// buttons and the chips render unwired, covering the `None` branches.
/// The field's `on_input_edit` stays wired — a text field without it
/// cannot take focus (`ui/field.rs` debug-asserts), so an unwired field
/// is not a renderable state.
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

// ---- runtime rendering ------------------------------------------------------

#[test]
fn every_i18n_key_the_input_area_asks_for_exists() {
    for key in [
        "chat.skillsLoaded",
        "chat.autoContext.sql",
        "chat.autoContext.selection",
        "chat.autoContext.error",
        "chat.autoContext.result",
        "chat.injection.template",
        "chat.injection.notes",
        "chat.injection.priority",
        "chat.injection.readOnlyHint",
        "chat.injection.priorityHint",
        "redis.ai.context.attached",
        "chat.draft.conflict",
        "chat.draft.replace",
        "chat.draft.append",
        "chat.draft.appendSeparator",
        "common.cancel",
        "chat.placeholder",
        "chat.egressNoticeRelaxed",
        "chat.egressNoticeContext",
        "chat.egressDatabase",
        "chat.stop",
    ] {
        assert!(has(key), "missing i18n key: {key}");
    }
}

#[test]
fn the_empty_area_renders_only_the_input_row() {
    let rt = runtime(probe(), 960.0, 640.0);
    for key in [
        "ai-chat-input-area",
        "ai-input",
        "ai-input-field",
        "ai-input-text",
        "ai-input-submit",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    for key in [
        "ai-egress-notice",
        "ai-loaded-skills",
        "ai-auto-context-chips",
        "ai-injection-chips",
        "ai-kv-context-chip",
        "draft-conflict-bar",
        "ai-input-stop",
    ] {
        assert!(!has_key(&rt, key), "{key} is absent on a fresh VM");
    }
}

#[test]
fn typing_and_clicking_send_submits_trimmed_text_and_clears() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("  hello  ");
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(rt.state.area.input_text(), "  hello  ");
    click(&mut rt, "ai-input-submit");
    assert_eq!(rt.state.submitted, vec!["hello".to_string()]);
    assert_eq!(
        rt.state.area.input_text(),
        "",
        "the field clears after send"
    );
}

#[test]
fn enter_in_the_field_submits() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.submitted, vec!["hello".to_string()]);
}

#[test]
fn tab_reaches_the_send_button_and_enter_submits_from_it() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-text");
    rt.paste("hello");
    rt.settle_all();
    rt.frame(0.0);
    focus_by_tab(&mut rt, "ai-input-submit");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.submitted, vec!["hello".to_string()]);
}

#[test]
fn blank_input_and_the_disabled_send_submit_nothing() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    click(&mut rt, "ai-input-submit");
    assert!(
        rt.state.submitted.is_empty(),
        "the send button is disabled while the draft is blank"
    );
    click(&mut rt, "ai-input-text");
    rt.paste("   ");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "ai-input-submit");
    assert!(
        rt.state.submitted.is_empty(),
        "a whitespace-only draft still submits nothing"
    );
}

#[test]
fn streaming_shows_stop_and_hides_send() {
    let mut p = probe();
    p.area.set_input("hello");
    p.area.set_streaming(true);
    let mut rt = runtime(p, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-input-stop"),
        "stop replaces send while streaming"
    );
    assert!(!has_key(&rt, "ai-input-submit"));
    assert_eq!(
        a11y_label(&rt, "ai-input-stop").as_deref(),
        Some(t("chat.stop")),
        "the stop button keeps React's title as its accessible name"
    );
    click(&mut rt, "ai-input-stop");
    assert_eq!(rt.state.stops, 1, "the stop press is reported");
}

#[test]
fn the_egress_notice_renders_summary_database_and_messages() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-egress-summary"),
        "summary text",
        "the summary line renders verbatim"
    );
    assert_eq!(element_text(&rt, "ai-egress-database"), "数据库：mydb");
    assert_eq!(
        element_text(&rt, "ai-egress-message-0"),
        t("chat.egressNoticeRelaxed"),
        "the relaxed note comes first"
    );
    assert_eq!(
        element_text(&rt, "ai-egress-message-1"),
        t("chat.egressNoticeContext")
    );
    // Dual channel: the amber wash is the laid-out fill, not a self-report.
    assert_eq!(
        element(&rt, "ai-egress-notice").fill,
        ai_amber_fill(true),
        "the notice fill is amber-500 at 10%"
    );
    assert_eq!(
        element(&rt, "ai-egress-summary").text_color,
        Some(ai_amber_text(true)),
        "the summary text is amber-200"
    );
}

#[test]
fn the_egress_notice_stays_hidden_for_a_database_label_alone() {
    let mut p = probe();
    p.area
        .set_egress(None, Some("mydb".to_string()), false, false);
    let rt = runtime(p, 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-egress-notice"),
        "a database label alone never shows the notice"
    );
}

#[test]
fn the_loaded_skills_row_shows_the_count_and_the_chips() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-loaded-skills-label"), "已加载技能：2");
    assert_eq!(element_text(&rt, "ai-loaded-skill-s1-label"), "sql-review");
    assert_eq!(
        element_text(&rt, "ai-loaded-skill-explain-label"),
        "explain"
    );
}

#[test]
fn auto_context_chips_remove_fields_one_by_one() {
    let mut rt = runtime(full_probe(), 960.0, 640.0);
    for key in [
        "ai-auto-context-sql",
        "ai-auto-context-selection",
        "ai-auto-context-error",
        "ai-auto-context-result",
    ] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    assert!(
        any_text(&rt, t("chat.autoContext.sql")),
        "the sql chip shows its label text"
    );
    click(&mut rt, "ai-auto-context-sql-remove");
    assert_eq!(rt.state.removed_fields, vec!["editorSql".to_string()]);
    assert!(!has_key(&rt, "ai-auto-context-sql"), "the chip goes away");
    assert!(
        has_key(&rt, "ai-auto-context-chips"),
        "other fields keep the row"
    );
    click(&mut rt, "ai-auto-context-selection-remove");
    click(&mut rt, "ai-auto-context-error-remove");
    click(&mut rt, "ai-auto-context-result-remove");
    assert_eq!(
        rt.state.removed_fields,
        vec![
            "editorSql".to_string(),
            "selection".to_string(),
            "lastError".to_string(),
            "resultPreview".to_string(),
        ]
    );
    assert!(
        !has_key(&rt, "ai-auto-context-chips"),
        "the row goes away once the snapshot is empty"
    );
}

#[test]
fn injection_chips_render_and_priority_needs_a_template() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-injection-template"), "模板：Tmpl");
    assert_eq!(
        element_text(&rt, "ai-injection-notes"),
        t("chat.injection.notes")
    );
    assert_eq!(
        element_text(&rt, "ai-injection-priority"),
        t("chat.injection.priority")
    );
    let mut p = probe();
    p.area.set_injection(Some(AiInjection {
        template_name: None,
        has_connection_notes: true,
        has_global_instructions: true,
    }));
    let rt = runtime(p, 960.0, 640.0);
    assert!(has_key(&rt, "ai-injection-chips"));
    assert!(has_key(&rt, "ai-injection-notes"));
    assert!(
        !has_key(&rt, "ai-injection-priority"),
        "no template, no priority chip"
    );
    assert!(!has_key(&rt, "ai-injection-template"));
}

#[test]
fn the_kv_chip_shows_the_key_name() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-kv-context-chip-label"),
        t("redis.ai.context.attached")
    );
    assert_eq!(element_text(&rt, "ai-kv-context-chip-key"), "user:1001");
}

#[test]
fn conflict_replace_append_and_cancel() {
    let rt = runtime(full_probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "draft-conflict-bar-text"),
        t("chat.draft.conflict")
    );
    // The kit buttons carry their labels as the accessible name; the
    // label text itself is a child, so it is read off the tree scan.
    assert_eq!(
        a11y_label(&rt, "draft-replace").as_deref(),
        Some(t("chat.draft.replace"))
    );
    assert_eq!(
        a11y_label(&rt, "draft-append").as_deref(),
        Some(t("chat.draft.append"))
    );
    assert_eq!(
        a11y_label(&rt, "draft-cancel").as_deref(),
        Some(t("common.cancel"))
    );
    assert!(any_text(&rt, t("chat.draft.replace")));
    assert!(any_text(&rt, t("chat.draft.append")));
    assert!(any_text(&rt, t("common.cancel")));
    assert_eq!(
        element(&rt, "draft-conflict-bar").fill,
        ai_amber_fill(true),
        "the conflict bar shares the egress amber wash"
    );

    // Replace: the draft takes over the input.
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "draft-replace");
    assert_eq!(
        rt.state.resolved,
        vec![(ConflictResolution::Replace, "r1".to_string())]
    );
    assert_eq!(rt.state.area.input_text(), "draft text");
    assert!(!has_key(&rt, "draft-conflict-bar"));

    // Append: prev + separator + draft.
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "draft-append");
    assert_eq!(
        rt.state.resolved,
        vec![(ConflictResolution::Append, "r1".to_string())]
    );
    assert_eq!(
        rt.state.area.input_text(),
        format!("keep me{}draft text", t("chat.draft.appendSeparator"))
    );
    assert!(!has_key(&rt, "draft-conflict-bar"));

    // Cancel: the input survives, the bar goes away.
    let mut p = full_probe();
    p.area.set_input("keep me");
    let mut rt = runtime(p, 960.0, 640.0);
    click(&mut rt, "draft-cancel");
    assert_eq!(rt.state.dismissed_conflicts, 1);
    assert!(rt.state.resolved.is_empty());
    assert_eq!(rt.state.area.input_text(), "keep me");
    assert!(!has_key(&rt, "draft-conflict-bar"));
}

#[test]
fn the_bare_area_renders_without_callbacks() {
    let mut p = full_probe();
    p.dark = false;
    let rt = bare_runtime(p, 960.0, 640.0);
    for key in [
        "ai-egress-notice",
        "ai-loaded-skills",
        "ai-auto-context-chips",
        "ai-injection-chips",
        "ai-kv-context-chip",
        "draft-conflict-bar",
        "ai-input-field",
        "ai-input-submit",
    ] {
        assert!(has_key(&rt, key), "{key} renders unwired in light mode");
    }
}

#[test]
fn the_full_area_stays_inside_both_window_sizes() {
    for (w, h, dark) in [(960.0, 640.0, true), (1440.0, 816.0, false)] {
        let mut p = full_probe();
        p.dark = dark;
        p.area
            .set_input("a long draft that keeps going so the field has content to lay out");
        let rt = runtime(p, w, h);
        assert_no_horizontal_overflow(&rt, w, "full input area");
        let root = element(&rt, "ai-chat-input-area").rect;
        assert!(
            root[0] >= -0.5 && root[1] >= -0.5 && root[0] + root[2] <= w + 0.5,
            "the input area stays inside {w}x{h}: {root:?}"
        );
        for key in [
            "ai-egress-notice",
            "ai-loaded-skills",
            "ai-auto-context-chips",
            "ai-injection-chips",
            "ai-kv-context-chip",
            "draft-conflict-bar",
            "ai-input-field",
        ] {
            let r = element(&rt, key).rect;
            assert!(
                r[0] >= -0.5 && r[0] + r[2] <= w + 0.5,
                "{key} escapes {w}x{h}: {r:?}"
            );
        }
    }
}
