//! T009-S3 independent audit: AiChatPanel header (`AiChatPanel.tsx:405–644`).
//!
//! Fresh vectors, written against the React source and the React zh-CN
//! locale packs — not against the coder's test file. Where a value can be
//! pinned without going through the component's own `t()` helper it is
//! pinned against the literal zh-CN string from
//! `packages/frontend/src/locales/zh-CN/{ai,core}.ts`, so a wrong table
//! entry cannot hide behind a self-referential comparison.
//!
//! Geometry is asserted on `rt.elements()` rects, which come from the
//! layout engine and the paint track, not from the code under test.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::ai::{
    detect_safety_preset, AiAgentMode, AiChatHeader, AiChatHeaderState, AiChatTab, AiProfile,
    AiPromptTemplate, AiSafetyGate, AiSafetyPreset,
};
use crate::i18n::t;
use crate::icons::{ICON_HISTORY, ICON_MESSAGE_SQUARE, ICON_SHIELD, ICON_WAND_SPARKLES};
use crate::ui::palette::{
    ai_shield_cloud_strict, ai_shield_enterprise_balanced, ai_shield_local_trust,
};

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

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
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

fn focus_by_tab<S>(rt: &mut Runtime<S>, key: &str) {
    for _ in 0..60 {
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
        press(rt, Key::Tab);
    }
    panic!("{key} never took focus after 60 Tabs");
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
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
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

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    let (ox, oy, ow, oh) = (outer[0], outer[1], outer[2], outer[3]);
    let (ix, iy, iw, ih) = (inner[0], inner[1], inner[2], inner[3]);
    ix >= ox - 0.5 && iy >= oy - 0.5 && ix + iw <= ox + ow + 0.5 && iy + ih <= oy + oh + 0.5
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn zero_area(rect: [f32; 4]) -> bool {
    rect[2] <= 0.5 || rect[3] <= 0.5
}

fn key_of(element: &rune::ElementInfo) -> String {
    element.key.clone().unwrap_or_else(|| element.path.clone())
}

fn assert_no_sibling_overlap<S>(rt: &Runtime<S>, label: &str) {
    let elements = rt.elements();
    let nodes: Vec<&rune::ElementInfo> = elements
        .iter()
        .filter(|element| element.path == "probe-page" || element.path.starts_with("probe-page/"))
        .collect();
    let mut collisions = Vec::new();
    for (index, first) in nodes.iter().enumerate() {
        for second in &nodes[index + 1..] {
            if parent_path(&first.path) != parent_path(&second.path) {
                continue;
            }
            if zero_area(first.rect) || zero_area(second.rect) {
                continue;
            }
            if contains(first.rect, second.rect) || contains(second.rect, first.rect) {
                continue;
            }
            let overlap_w = (first.rect[0] + first.rect[2]).min(second.rect[0] + second.rect[2])
                - first.rect[0].max(second.rect[0]);
            let overlap_h = (first.rect[1] + first.rect[3]).min(second.rect[1] + second.rect[3])
                - first.rect[1].max(second.rect[1]);
            if overlap_w > 0.5 && overlap_h > 0.5 {
                collisions.push(format!(
                    "{} {:?} x {} {:?}",
                    key_of(first),
                    first.rect,
                    key_of(second),
                    second.rect
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "{label}: {} sibling collision(s): {collisions:#?}",
        collisions.len()
    );
}

// ---- fixtures ---------------------------------------------------------------

fn templates() -> Vec<AiPromptTemplate> {
    vec![
        AiPromptTemplate::new("tpl-sql", "SQL Review"),
        AiPromptTemplate::new("tpl-blank", " "),
        AiPromptTemplate::new("tpl-empty", ""),
    ]
}

fn profiles() -> Vec<AiProfile> {
    vec![
        AiProfile::new("p-cloud", "Cloud", "gpt-5"),
        AiProfile::new("p-local", "", "ollama")
            .is_default(true)
            .safety_gate(AiSafetyGate::local_trust()),
    ]
}

#[derive(Default)]
struct AuditProbe {
    header: AiChatHeaderState,
    dark: bool,
    tabs: Vec<AiChatTab>,
    history_toggles: usize,
    ai_help: usize,
    workflow_help: usize,
    clears: usize,
    modes: Vec<AiAgentMode>,
    template_choices: Vec<String>,
    profile_choices: Vec<String>,
    sharing_toggles: usize,
}

fn audit_probe() -> AuditProbe {
    AuditProbe {
        header: AiChatHeaderState::default(),
        dark: true,
        ..AuditProbe::default()
    }
}

fn rich_audit_probe() -> AuditProbe {
    let mut probe = audit_probe();
    probe.header.set_templates(templates());
    probe.header.set_profiles(profiles());
    probe.header.set_active_profile_id("p-cloud");
    probe.header.set_can_share(true);
    probe.header.set_has_messages(true);
    probe
}

fn header_view(state: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let mut layer = Vec::new();
    let header = AiChatHeader::<AuditProbe>::new(&state.header)
        .on_select_tab(|s: &mut AuditProbe, tab| {
            s.tabs.push(tab);
            s.header.set_tab(tab);
        })
        .on_toggle_session_history(|s: &mut AuditProbe| {
            s.history_toggles += 1;
            s.header.toggle_session_history();
        })
        .on_open_ai_help(|s: &mut AuditProbe| s.ai_help += 1)
        .on_open_workflow_help(|s: &mut AuditProbe| s.workflow_help += 1)
        .on_clear_chat(|s: &mut AuditProbe| s.clears += 1)
        .on_select_agent_mode(|s: &mut AuditProbe, mode| {
            s.modes.push(mode);
            s.header.set_agent_mode(mode);
        })
        .on_toggle_template(|s: &mut AuditProbe| s.header.toggle_template_open())
        .on_close_template(|s: &mut AuditProbe| s.header.close_template_select())
        .on_choose_template(|s: &mut AuditProbe, id: &str, _tx: &mut Tx| {
            s.template_choices.push(id.to_string());
            s.header.choose_template(id);
        })
        .on_toggle_profile(|s: &mut AuditProbe| s.header.toggle_profile_open())
        .on_close_profile(|s: &mut AuditProbe| s.header.close_profile_select())
        .on_choose_profile(|s: &mut AuditProbe, id: &str, _tx: &mut Tx| {
            s.profile_choices.push(id.to_string());
            s.header.choose_profile(id);
        })
        .on_toggle_sharing(|s: &mut AuditProbe| {
            s.sharing_toggles += 1;
            let next = !s.header.sharing();
            s.header.set_sharing(next);
        })
        .build(state.dark, &mut layer);
    rune_kit::root(
        col()
            .key("probe-page")
            .w_full()
            .h_full()
            .padding(16.0)
            .child(header),
        layer,
    )
}

fn runtime(probe: AuditProbe, w: f32, h: f32) -> Runtime<AuditProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, header_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- preset detection: one-field-off vectors different from the coder's ----

#[test]
fn audit_preset_detection_other_single_field_deviations_are_custom() {
    // Cloud-strict with the MCP policy loosened.
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "strict",
            "read_only",
            "require_confirm",
            true
        )),
        AiSafetyPreset::Custom,
        "cloud_strict with an MCP policy other than disabled is custom"
    );
    // Enterprise-balanced with the egress level swapped for strict.
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "strict",
            "require_confirm",
            "require_confirm",
            true
        )),
        AiSafetyPreset::Custom,
        "enterprise_balanced with strict egress is custom"
    );
    // Local-trust with the DB policy tightened.
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "unrestricted",
            "read_only",
            "unrestricted",
            false
        )),
        AiSafetyPreset::Custom,
        "local_trust with a read-only DB policy is custom"
    );
    // All-empty gate: nothing matches.
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new("", "", "", false)),
        AiSafetyPreset::Custom
    );
    // The three exact presets still resolve (sanity: the audit agrees on
    // the positive cases too).
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::cloud_strict()),
        AiSafetyPreset::CloudStrict
    );
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::enterprise_balanced()),
        AiSafetyPreset::EnterpriseBalanced
    );
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::local_trust()),
        AiSafetyPreset::LocalTrust
    );
}

#[test]
fn audit_shield_preset_returns_custom_enum_for_a_custom_gate() {
    let mut state = AiChatHeaderState::default();
    state.set_profiles(vec![AiProfile::new("p-x", "X", "m").safety_gate(
        AiSafetyGate::new("strict", "require_confirm", "disabled", true),
    )]);
    state.set_active_profile_id("p-x");
    assert_eq!(
        state.shield_preset(),
        AiSafetyPreset::Custom,
        "the state reports Custom; only the rendered colour falls back to blue"
    );
    // And the rendered shield for that same gate is the blue one.
    let mut probe = audit_probe();
    probe
        .header
        .set_profiles(vec![AiProfile::new("p-x", "X", "m").safety_gate(
            AiSafetyGate::new("strict", "require_confirm", "disabled", true),
        )]);
    probe.header.set_active_profile_id("p-x");
    let rt = runtime(probe, 960.0, 640.0);
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_cloud_strict(true))
    );
}

// ---- JS `||` edges: whitespace is truthy in JS -------------------------------

#[test]
fn audit_whitespace_names_are_truthy_like_js_or() {
    // JS `template.name || template.id`: a blank-but-non-empty name wins.
    assert_eq!(AiPromptTemplate::new("tpl-blank", " ").label(), " ");
    assert_eq!(AiPromptTemplate::new("tpl-empty", "").label(), "tpl-empty");
    // JS `p.name || p.model`: same rule for profiles.
    assert_eq!(AiProfile::new("a", " ", "model").display_name(), " ");
    assert_eq!(AiProfile::new("b", "", " ").display_name(), " ");
    assert_eq!(AiProfile::new("c", "", "").display_name(), "");
}

#[test]
fn audit_profile_label_keeps_a_whitespace_name_instead_of_ai_model() {
    let mut state = AiChatHeaderState::default();
    state.set_profiles(vec![AiProfile::new("w", " ", "model")]);
    state.set_active_profile_id("w");
    assert_eq!(
        state.profile_display_label(),
        " :",
        "JS `name || model || 'AI Model'` keeps the whitespace name"
    );
}

#[test]
fn audit_first_default_wins_when_two_profiles_claim_default() {
    let mut state = AiChatHeaderState::default();
    state.set_profiles(vec![
        AiProfile::new("d1", "First", "m1").is_default(true),
        AiProfile::new("d2", "Second", "m2").is_default(true),
    ]);
    assert_eq!(
        state.active_profile().map(|p| p.id.as_str()),
        Some("d1"),
        "JS `Array.find` returns the first default, as does Rust's `find`"
    );
}

// ---- share gating: symmetric -----------------------------------------------

#[test]
fn audit_set_sharing_off_is_also_blocked_while_sharing_is_impossible() {
    let mut state = AiChatHeaderState::default();
    state.set_can_share(true);
    state.set_sharing(true);
    assert!(state.sharing());
    state.set_can_share(false);
    state.set_sharing(false);
    assert!(
        state.sharing(),
        "React's disabled checkbox cannot change the value either way"
    );
}

// ---- unknown ids fall back ---------------------------------------------------

#[test]
fn audit_unknown_template_id_falls_back_to_the_auto_label() {
    let mut probe = rich_audit_probe();
    probe.header.choose_template("no-such-template");
    assert_eq!(probe.header.template_id(), "no-such-template");
    let rt = runtime(probe, 960.0, 640.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("自动选择模板")),
        "the trigger shows the auto label, not the unknown id"
    );
}

#[test]
fn audit_unknown_profile_id_falls_back_to_default_label_and_shield() {
    let mut probe = rich_audit_probe();
    probe.header.choose_profile("no-such-profile");
    let rt = runtime(probe, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-profile-label"),
        "ollama:",
        "the label follows the default profile after an unknown choice"
    );
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_local_trust(true)),
        "and so does the shield"
    );
}

// ---- rendered strings pinned to the React zh-CN packs ------------------------

#[test]
fn audit_rendered_strings_match_the_react_zh_locale_verbatim() {
    // Literals copied from packages/frontend/src/locales/zh-CN/{ai,core}.ts,
    // not from the component's own `t()` table.
    let mut success = rich_audit_probe();
    success.header.set_session_migration_success(3);
    let rt = runtime(success, 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "ai-tab-chat").as_deref(),
        Some("AI 助手"),
        "zh-CN core.ts: 'common.aiAssistant'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-tab-workflows").as_deref(),
        Some("工作流"),
        "zh-CN ai.ts: 'ai.workflows.tab'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-session-history-toggle").as_deref(),
        Some("会话历史"),
        "zh-CN ai.ts: 'chat.sessions.open'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-open-ai-help").as_deref(),
        Some("AI 与上下文说明"),
        "zh-CN core.ts: 'docs.openAiHelp'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-clear-chat").as_deref(),
        Some("清除对话"),
        "zh-CN ai.ts: 'chat.clear'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-agent-mode-ask").as_deref(),
        Some("询问"),
        "zh-CN ai.ts: 'chat.mode.ask'"
    );
    assert_eq!(
        a11y_label(&rt, "ai-agent-mode-agent").as_deref(),
        Some("Agent"),
        "zh-CN ai.ts: 'chat.mode.agent'"
    );
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        "仅基于已有上下文回答，不调用数据库工具。",
        "zh-CN ai.ts: 'chat.mode.askHint'"
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("自动选择模板")),
        "zh-CN ai.ts: 'chat.template.auto' on the template trigger"
    );
    assert_eq!(
        element_text(&rt, "ai-session-migration-success-banner-text"),
        "已将 3 个旧版本地会话迁移到应用数据库。",
        "zh-CN ai.ts: 'chat.sessions.migrationSuccess' with {{count}} = 3"
    );
    assert_eq!(
        element_text(&rt, "ai-profile-label"),
        "Cloud:",
        "the profile label is the name plus a colon, as React renders it"
    );
}

#[test]
fn audit_agent_hint_switches_to_the_agent_text_verbatim() {
    let mut rt = runtime(rich_audit_probe(), 960.0, 640.0);
    click(&mut rt, "ai-agent-mode-agent");
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        "可调用数据库和 MCP 工具查证后回答。",
        "zh-CN ai.ts: 'chat.mode.agentHint'"
    );
}

#[test]
fn audit_share_strings_match_the_react_zh_locale_verbatim() {
    let mut probe = audit_probe();
    probe
        .header
        .set_share_error(Some("grant denied".to_string()));
    let rt = runtime(probe, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-share-editor-unavailable"),
        "请打开查询编辑器以共享其会话。",
        "zh-CN ai.ts: 'chat.shareEditor.unavailable'"
    );
    assert_eq!(
        element_text(&rt, "ai-share-editor-error"),
        "无法共享编辑器会话：grant denied",
        "zh-CN ai.ts: 'chat.shareEditor.error' with {{error}} filled in"
    );
}

// ---- migration banners: React's `&&` truthiness -----------------------------

#[test]
fn audit_empty_migration_error_string_renders_an_empty_error_banner() {
    // AUDIT D1 (non-blocking, pinned behaviour): React's
    // `{tab === 'chat' && sessionMigrationError && (...)}` treats an
    // empty string as falsy, so no error banner renders and the success
    // banner's `!sessionMigrationError` gate passes. The Rune header
    // treats `Some("")` as present and renders an empty error banner
    // instead. No React producer emits an empty error string today
    // (aiStore sets `result.error ?? 'AI session backend write failed'`,
    // i.e. a non-empty message or null), so this is unreachable from the
    // real store — the panel slice / T088 wiring should normalise "" to
    // None at the seam, or the header should filter empty strings.
    // This test pins the current behaviour so the later fix has a
    // regression tripwire to flip.
    let mut probe = rich_audit_probe();
    probe
        .header
        .set_session_migration_error(Some(String::new()));
    probe.header.set_session_migration_success(3);
    let rt = runtime(probe, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-session-migration-banner"),
        "D1: Some(\"\") currently renders the error banner"
    );
    assert_eq!(
        element_text(&rt, "ai-session-migration-banner-text"),
        "",
        "D1: the banner text is empty"
    );
    assert!(
        !has_key(&rt, "ai-session-migration-success-banner"),
        "D1: the empty error currently wins over success"
    );
}

#[test]
fn audit_empty_share_error_string_renders_an_empty_error_span() {
    // AUDIT D1 (same root cause as the migration banner): React's
    // `{shareError && (...)}` renders nothing for an empty string, while
    // the Rune header renders the error span with an empty interpolation.
    // Unreachable from the real grant flow (it sets a message or null);
    // pinned here so the later fix has a tripwire to flip.
    let mut probe = rich_audit_probe();
    probe.header.set_share_error(Some(String::new()));
    let rt = runtime(probe, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-share-editor-error"),
        "D1: Some(\"\") currently renders the error span"
    );
    assert_eq!(
        element_text(&rt, "ai-share-editor-error"),
        t("chat.shareEditor.error").replacen("{error}", "", 1),
        "D1: the span shows the template with an empty error"
    );
}

#[test]
fn audit_migration_banner_hides_on_the_workflows_tab() {
    let mut probe = rich_audit_probe();
    probe
        .header
        .set_session_migration_error(Some("migration failed".to_string()));
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "ai-session-migration-banner"));
    click(&mut rt, "ai-tab-workflows");
    assert!(
        !has_key(&rt, "ai-session-migration-banner"),
        "React gates both banners on `tab === 'chat'`"
    );
    assert!(!has_key(&rt, "ai-session-migration-success-banner"));
    click(&mut rt, "ai-tab-chat");
    assert!(
        has_key(&rt, "ai-session-migration-banner"),
        "switching back restores the banner"
    );
}

#[test]
fn audit_success_count_zero_shows_no_banner_and_one_interpolates() {
    let mut zero = rich_audit_probe();
    zero.header.set_session_migration_success(0);
    let rt = runtime(zero, 960.0, 640.0);
    assert!(!has_key(&rt, "ai-session-migration-banner"));
    assert!(!has_key(&rt, "ai-session-migration-success-banner"));

    let mut one = rich_audit_probe();
    one.header.set_session_migration_success(1);
    let rt = runtime(one, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-session-migration-success-banner-text"),
        "已将 1 个旧版本地会话迁移到应用数据库。"
    );
}

// ---- selects: the other channel ----------------------------------------------

#[test]
fn audit_profile_select_keyboard_choose_matches_pointer() {
    let mut keys = runtime(rich_audit_probe(), 960.0, 640.0);
    focus_by_tab(&mut keys, "ai-profile-select-field-trigger");
    press(&mut keys, Key::Text(" ".into()));
    assert!(keys.state.header.profile_open(), "Space opens the listbox");
    press(&mut keys, Key::Down);
    assert_eq!(
        focused_key(&keys).as_deref(),
        Some("option1"),
        "one Down lands on the second row"
    );
    press(&mut keys, Key::Enter);
    assert_eq!(keys.state.profile_choices, vec!["p-local".to_string()]);
    assert_eq!(keys.state.header.active_profile_id(), "p-local");
    assert!(!keys.state.header.profile_open());

    let mut pointer = runtime(rich_audit_probe(), 960.0, 640.0);
    click(&mut pointer, "ai-profile-select-field-trigger");
    click(&mut pointer, "option1");
    assert_eq!(pointer.state.profile_choices, keys.state.profile_choices);
}

#[test]
fn audit_template_trigger_shows_the_id_when_the_chosen_name_is_empty() {
    let mut rt = runtime(rich_audit_probe(), 960.0, 640.0);
    click(&mut rt, "ai-template-select-field-trigger");
    // options: 0 = auto, 1 = tpl-sql, 2 = tpl-blank (" "), 3 = tpl-empty ("").
    click(&mut rt, "option3");
    assert_eq!(rt.state.template_choices, vec!["tpl-empty".to_string()]);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("tpl-empty")),
        "the trigger shows the id when the template name is empty"
    );
}

#[test]
fn audit_escape_closes_the_template_select_without_choosing() {
    let mut rt = runtime(rich_audit_probe(), 960.0, 640.0);
    click(&mut rt, "ai-template-select-field-trigger");
    assert!(rt.state.header.template_open());
    press(&mut rt, Key::Escape);
    assert!(
        !rt.state.header.template_open(),
        "Escape closes the listbox (kit select contract)"
    );
    assert!(rt.state.template_choices.is_empty());
    assert_eq!(rt.state.header.template_id(), "");
}

// ---- share row: both spans together ------------------------------------------

#[test]
fn audit_share_error_and_unavailable_note_render_together() {
    let mut probe = audit_probe();
    probe
        .header
        .set_share_error(Some("grant denied".to_string()));
    let rt = runtime(probe, 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-share-editor-unavailable"),
        "React renders the note whenever sharing is impossible"
    );
    assert!(
        has_key(&rt, "ai-share-editor-error"),
        "and the error span alongside it"
    );
    assert!(
        !a11y_states(&rt, "ai-share-editor-toggle").contains(&"checked".to_string()),
        "the switch is unchecked"
    );
}

// ---- clear button: disabled is observable in the a11y tree -------------------

#[test]
fn audit_clear_button_reports_disabled_without_messages() {
    let rt = runtime(audit_probe(), 960.0, 640.0);
    assert!(
        a11y_states(&rt, "ai-clear-chat").contains(&"disabled".to_string()),
        "the a11y tree marks the disabled clear button"
    );
    let rt = runtime(rich_audit_probe(), 960.0, 640.0);
    assert!(
        !a11y_states(&rt, "ai-clear-chat").contains(&"disabled".to_string()),
        "with messages the clear button is enabled"
    );
}

// ---- dividers -----------------------------------------------------------------

#[test]
fn audit_dividers_follow_every_rendered_row() {
    let mut error = rich_audit_probe();
    error
        .header
        .set_session_migration_error(Some("migration failed".to_string()));
    let rt = runtime(error, 960.0, 640.0);
    for key in [
        "ai-chat-header-divider",
        "ai-session-migration-divider",
        "ai-agent-mode-divider",
        "ai-profile-divider",
        "ai-share-editor-divider",
    ] {
        assert!(has_key(&rt, key), "{key} follows its row");
    }

    let rt = runtime(rich_audit_probe(), 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-session-migration-divider"),
        "no banner, no migration divider"
    );
    assert!(has_key(&rt, "ai-agent-mode-divider"));
}

// ---- icons: registered statics match lucide-react 0.487.0 --------------------

#[test]
fn audit_new_icons_match_lucide_paths_and_are_distinct() {
    assert_eq!(ICON_MESSAGE_SQUARE.name, "message-square");
    assert_eq!(ICON_WAND_SPARKLES.name, "wand-sparkles");
    assert_eq!(ICON_HISTORY.name, "history");
    assert_eq!(ICON_SHIELD.name, "shield");
    for icon in [
        &ICON_MESSAGE_SQUARE,
        &ICON_WAND_SPARKLES,
        &ICON_HISTORY,
        &ICON_SHIELD,
    ] {
        assert_eq!(icon.view, 24.0, "{} is a 24-view icon", icon.name);
        assert!(!icon.body.is_empty(), "{} has a body", icon.name);
    }
    // Path data copied from the installed lucide-react@0.487.0 files.
    assert!(ICON_MESSAGE_SQUARE
        .body
        .contains("M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"));
    assert!(ICON_WAND_SPARKLES
        .body
        .contains("m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64"));
    assert!(ICON_HISTORY
        .body
        .contains("M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"));
    assert!(ICON_SHIELD
        .body
        .contains("M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6"));
    let bodies = [
        ICON_MESSAGE_SQUARE.body,
        ICON_WAND_SPARKLES.body,
        ICON_HISTORY.body,
        ICON_SHIELD.body,
    ];
    for (i, first) in bodies.iter().enumerate() {
        for second in &bodies[i + 1..] {
            assert_ne!(first, second, "no two new icons share a body");
        }
    }
}

// ---- shield hues in the light theme -------------------------------------------

#[test]
fn audit_shield_hues_are_theme_independent_tailwind_values() {
    // The palette fns ignore the theme flag: the same Tailwind sRGB value
    // in both themes, exactly as React's `text-*-400` classes do.
    for dark in [true, false] {
        assert_eq!(
            ai_shield_cloud_strict(dark),
            rune::color::hex("#60a5fa").unwrap_or(rune::Color::WHITE)
        );
        assert_eq!(
            ai_shield_enterprise_balanced(dark),
            rune::color::hex("#fbbf24").unwrap_or(rune::Color::WHITE)
        );
        assert_eq!(
            ai_shield_local_trust(dark),
            rune::color::hex("#34d399").unwrap_or(rune::Color::WHITE)
        );
    }
    let mut light = rich_audit_probe();
    light.dark = false;
    let rt = runtime(light, 960.0, 640.0);
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_cloud_strict(false)),
        "the light-theme shield is the same blue"
    );
}

// ---- geometry: own vectors -----------------------------------------------------

#[test]
fn audit_header_geometry_both_windows_both_themes_with_a_wrapping_banner() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = rich_audit_probe();
            probe.dark = dark;
            probe.header.set_session_migration_error(Some(
                "迁移失败：旧会话数据库文件已损坏，无法读取其中的历史记录，请检查后重试。"
                    .to_string(),
            ));
            let rt = runtime(probe, w, h);
            assert_in_window(&rt, w, h, &format!("{w}x{h} dark={dark}"));
            assert_no_sibling_overlap(&rt, &format!("{w}x{h} dark={dark}"));
            // Rows stack in React order: header, banner, mode, profile, share.
            let header = element(&rt, "ai-chat-header-row").rect;
            let banner = element(&rt, "ai-session-migration-banner").rect;
            let mode = element(&rt, "ai-agent-mode-row").rect;
            let profile = element(&rt, "ai-profile-row").rect;
            let share = element(&rt, "ai-share-editor-row").rect;
            assert!(
                header[1] + header[3] <= banner[1] + 0.5
                    && banner[1] + banner[3] <= mode[1] + 0.5
                    && mode[1] + mode[3] <= profile[1] + 0.5
                    && profile[1] + profile[3] <= share[1] + 0.5,
                "{w}x{h} dark={dark}: rows stack top to bottom without overlap"
            );
            // The template select stays at the row's right edge.
            let row = element(&rt, "ai-agent-mode-row").rect;
            let select = element(&rt, "ai-template-select").rect;
            assert!(
                (select[0] + select[2] - (row[0] + row[2] - 12.0)).abs() <= 1.0,
                "{w}x{h} dark={dark}: the select ends at the row's right padding"
            );
            // The profile select (w-44 = 176) also ends at its row's right edge.
            let prow = element(&rt, "ai-profile-row").rect;
            let pselect = element(&rt, "ai-profile-select").rect;
            assert!(
                (pselect[0] + pselect[2] - (prow[0] + prow[2] - 12.0)).abs() <= 1.0,
                "{w}x{h} dark={dark}: the profile select ends at the row's right padding"
            );
            assert!(
                (pselect[2] - 176.0).abs() <= 1.0,
                "{w}x{h} dark={dark}: the profile select is w-44"
            );
            assert!(
                (select[2] - 144.0).abs() <= 1.0,
                "{w}x{h} dark={dark}: the template select is w-36"
            );
        }
    }
}

#[test]
fn audit_i18n_keys_used_by_the_header_exist_in_the_rune_table() {
    // Independent of the coder's identical list: re-derive it from the
    // React source (every `t('…')` in AiChatPanel.tsx:405–644 plus the two
    // header keys the clear button and tabs use).
    for key in [
        "common.aiAssistant",
        "ai.workflows.tab",
        "chat.sessions.open",
        "docs.openAiHelp",
        "docs.openWorkflowHelp",
        "chat.clear",
        "chat.sessions.migrationSuccess",
        "chat.mode.ask",
        "chat.mode.agent",
        "chat.mode.askHint",
        "chat.mode.agentHint",
        "chat.template.auto",
        "chat.template.label",
        "chat.shareEditor.label",
        "chat.shareEditor.unavailable",
        "chat.shareEditor.error",
    ] {
        assert!(crate::i18n::has(key), "missing i18n key: {key}");
        assert!(
            !t(key).is_empty(),
            "i18n key {key} translates to an empty string"
        );
    }
}
