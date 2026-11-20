//! T009-S3: AiChatPanel header (`AiChatPanel.tsx:405–644`).
//!
//! Pure-logic tests call the state machine directly; runtime tests render
//! the header in a probe `Runtime` and read back the laid-out element tree
//! and the accessibility dump — never the component's own report of itself.
//! Geometry is asserted on `rt.elements()` rects, fills and `text_color`,
//! which come from the layout engine and the paint track, not from the code
//! under test.
//!
//! The header is a view-model placeholder for the `aiStore` slice (T088):
//! the tests seed the state through its `set_*` methods and assert the
//! callbacks report presses, because no store exists to read back yet.
//! `AiSessionHistory` is not rendered here — only its toggle — and the
//! help / clear buttons only report, so the tests pin the report, not a
//! window opening or a session clearing.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::ai::{
    detect_safety_preset, AiAgentMode, AiChatHeader, AiChatHeaderState, AiChatTab, AiProfile,
    AiPromptTemplate, AiSafetyGate, AiSafetyPreset,
};
use crate::i18n::{has, t};
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

/// Siblings under the same parent path never overlap, except when one rect
/// contains the other (a child inside its parent's box is layering, not a
/// collision). Copied from `base_geometry.rs`'s `assert_siblings`, scoped
/// to the probe page; no allow-list — a collision here is a real defect.
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
struct Probe {
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

fn probe() -> Probe {
    Probe {
        header: AiChatHeaderState::default(),
        dark: true,
        ..Probe::default()
    }
}

fn rich_probe() -> Probe {
    let mut probe = probe();
    probe.header.set_templates(templates());
    probe.header.set_profiles(profiles());
    probe.header.set_active_profile_id("p-cloud");
    probe.header.set_can_share(true);
    probe.header.set_has_messages(true);
    probe
}

fn header_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let header = AiChatHeader::<Probe>::new(&state.header)
        .on_select_tab(|s: &mut Probe, tab| {
            s.tabs.push(tab);
            s.header.set_tab(tab);
        })
        .on_toggle_session_history(|s: &mut Probe| {
            s.history_toggles += 1;
            s.header.toggle_session_history();
        })
        .on_open_ai_help(|s: &mut Probe| s.ai_help += 1)
        .on_open_workflow_help(|s: &mut Probe| s.workflow_help += 1)
        .on_clear_chat(|s: &mut Probe| s.clears += 1)
        .on_select_agent_mode(|s: &mut Probe, mode| {
            s.modes.push(mode);
            s.header.set_agent_mode(mode);
        })
        .on_toggle_template(|s: &mut Probe| s.header.toggle_template_open())
        .on_close_template(|s: &mut Probe| s.header.close_template_select())
        .on_choose_template(|s: &mut Probe, id: &str, _tx: &mut Tx| {
            s.template_choices.push(id.to_string());
            s.header.choose_template(id);
        })
        .on_toggle_profile(|s: &mut Probe| s.header.toggle_profile_open())
        .on_close_profile(|s: &mut Probe| s.header.close_profile_select())
        .on_choose_profile(|s: &mut Probe, id: &str, _tx: &mut Tx| {
            s.profile_choices.push(id.to_string());
            s.header.choose_profile(id);
        })
        .on_toggle_sharing(|s: &mut Probe| {
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

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, header_view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// ---- AiChatHeaderState: pure logic ------------------------------------------

#[test]
fn detect_safety_preset_matches_the_typescript_conjunctions() {
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
    // Each preset is an exact conjunction: one field off is `custom`.
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "strict",
            "require_confirm",
            "disabled",
            true
        )),
        AiSafetyPreset::Custom,
        "cloud_strict with a balanced db policy is custom"
    );
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "sample_masked",
            "require_confirm",
            "require_confirm",
            false
        )),
        AiSafetyPreset::Custom,
        "enterprise_balanced without the SQL confirm is custom"
    );
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new(
            "unrestricted",
            "unrestricted",
            "unrestricted",
            true
        )),
        AiSafetyPreset::Custom,
        "local_trust with the SQL confirm on is custom"
    );
    assert_eq!(
        detect_safety_preset(&AiSafetyGate::new("off", "off", "off", false)),
        AiSafetyPreset::Custom,
        "an unknown gate is custom"
    );
}

#[test]
fn active_profile_falls_back_by_id_then_default_then_first() {
    let mut state = AiChatHeaderState::default();
    assert!(state.active_profile().is_none(), "no profiles, no profile");
    state.set_profiles(profiles());
    assert_eq!(
        state.active_profile().map(|p| p.id.as_str()),
        Some("p-local"),
        "no active id: the default profile wins over the first"
    );
    state.set_active_profile_id("p-cloud");
    assert_eq!(
        state.active_profile().map(|p| p.id.as_str()),
        Some("p-cloud"),
        "an explicit active id wins over the default"
    );
    state.set_active_profile_id("missing");
    assert_eq!(
        state.active_profile().map(|p| p.id.as_str()),
        Some("p-local"),
        "an unknown active id falls back to the default"
    );
    state.set_profiles(vec![AiProfile::new("only", "Only", "m")]);
    assert_eq!(
        state.active_profile().map(|p| p.id.as_str()),
        Some("only"),
        "with no default, the first profile wins"
    );
}

#[test]
fn profile_labels_fall_back_name_then_model_then_ai_model() {
    let named = AiProfile::new("a", "Cloud", "gpt-5");
    assert_eq!(named.display_name(), "Cloud");
    let model_only = AiProfile::new("b", "", "ollama");
    assert_eq!(model_only.display_name(), "ollama");
    let unnamed = AiProfile::new("c", "", "");
    assert_eq!(
        unnamed.display_name(),
        "",
        "React's `p.name || p.model` is empty when both are"
    );

    let mut state = AiChatHeaderState::default();
    assert_eq!(state.profile_display_label(), "AI Model:");
    state.set_profiles(vec![named]);
    state.set_active_profile_id("a");
    assert_eq!(state.profile_display_label(), "Cloud:");
    state.set_profiles(vec![model_only]);
    assert_eq!(state.profile_display_label(), "ollama:");
    state.set_profiles(vec![unnamed]);
    assert_eq!(
        state.profile_display_label(),
        "AI Model:",
        "the header label adds the 'AI Model' fallback React has"
    );
}

#[test]
fn template_label_falls_back_to_id_on_empty_name() {
    assert_eq!(
        AiPromptTemplate::new("tpl-sql", "SQL Review").label(),
        "SQL Review"
    );
    assert_eq!(
        AiPromptTemplate::new("tpl-empty", "").label(),
        "tpl-empty",
        "React's `template.name || template.id`"
    );
}

#[test]
fn set_sharing_is_gated_on_can_share() {
    let mut state = AiChatHeaderState::default();
    state.set_sharing(true);
    assert!(
        !state.sharing(),
        "the checkbox is disabled while sharing is impossible"
    );
    state.set_can_share(true);
    state.set_sharing(true);
    assert!(state.sharing(), "sharing applies once it is possible");
    state.set_sharing(false);
    assert!(!state.sharing());
}

#[test]
fn choose_template_and_choose_profile_close_their_selects() {
    let mut state = AiChatHeaderState::default();
    state.set_templates(templates());
    state.set_profiles(profiles());
    state.toggle_template_open();
    state.toggle_profile_open();
    assert!(state.template_open() && state.profile_open());
    state.choose_template("tpl-sql");
    assert_eq!(state.template_id(), "tpl-sql");
    assert!(!state.template_open(), "choosing closes the listbox");
    state.choose_profile("p-local");
    assert_eq!(state.active_profile_id(), "p-local");
    assert!(!state.profile_open(), "choosing closes the listbox");
    state.toggle_template_open();
    state.close_template_select();
    assert!(
        !state.template_open(),
        "Escape / outside closes without choosing"
    );
    state.toggle_profile_open();
    state.close_profile_select();
    assert!(!state.profile_open());
}

#[test]
fn session_history_open_can_be_set_directly_for_the_later_history_slice() {
    let mut state = AiChatHeaderState::default();
    state.toggle_session_history();
    assert!(state.session_history_open());
    state.set_session_history_open(false);
    assert!(
        !state.session_history_open(),
        "the history panel's onClose writes it directly"
    );
}

#[test]
fn defaults_mirror_reacts_use_state_initials() {
    let state = AiChatHeaderState::default();
    assert_eq!(state.tab(), AiChatTab::Chat);
    assert!(!state.session_history_open());
    assert_eq!(state.agent_mode(), AiAgentMode::Ask);
    assert_eq!(state.template_id(), "");
    assert!(state.templates().is_empty());
    assert!(!state.template_open());
    assert!(state.profiles().is_empty());
    assert_eq!(state.active_profile_id(), "");
    assert!(!state.profile_open());
    assert!(!state.sharing());
    assert!(!state.can_share());
    assert!(state.share_error().is_none());
    assert!(state.session_migration_error().is_none());
    assert_eq!(state.session_migration_success(), 0);
    assert!(!state.has_messages());
    assert!(!state.show_profile_row(), "no profiles, no profile row");
    assert_eq!(
        state.shield_preset(),
        AiSafetyPreset::CloudStrict,
        "no profile falls back to cloud_strict, as React does"
    );
}

#[test]
fn every_i18n_key_the_header_asks_for_exists() {
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
        assert!(has(key), "missing i18n key: {key}");
    }
}

// ---- header row --------------------------------------------------------------

#[test]
fn header_renders_both_tabs_with_the_active_chat_tab_selected() {
    let rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-chat-header"));
    // Kit buttons carry their label as the accessible name (the button
    // root itself holds no text node).
    assert_eq!(
        a11y_label(&rt, "ai-tab-chat").as_deref(),
        Some(t("common.aiAssistant"))
    );
    assert_eq!(
        a11y_label(&rt, "ai-tab-workflows").as_deref(),
        Some(t("ai.workflows.tab"))
    );
    assert!(
        element(&rt, "ai-tab-chat").item_selected,
        "the active tab reports selected through focus_selected"
    );
    assert!(
        !element(&rt, "ai-tab-workflows").item_selected,
        "the inactive tab does not"
    );
    assert_eq!(
        a11y_label(&rt, "ai-session-history-toggle").as_deref(),
        Some(t("chat.sessions.open")),
        "the history toggle is named by its title, as React's aria-label is"
    );
    assert_eq!(
        a11y_label(&rt, "ai-open-ai-help").as_deref(),
        Some(t("docs.openAiHelp"))
    );
    assert_eq!(
        a11y_label(&rt, "ai-clear-chat").as_deref(),
        Some(t("chat.clear")),
        "React gives the clear button no name; the kit button takes chat.clear"
    );
    // Tabs on the left, actions on the right (React's justify-between).
    assert!(
        element(&rt, "ai-chat-tabs").rect[0] < element(&rt, "ai-chat-header-actions").rect[0],
        "tabs sit left of the actions"
    );
}

#[test]
fn clicking_the_workflows_tab_switches_tabs_and_hides_chat_rows() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    click(&mut rt, "ai-tab-workflows");
    assert_eq!(rt.state.tabs, vec![AiChatTab::Workflows]);
    assert_eq!(rt.state.header.tab(), AiChatTab::Workflows);
    assert!(element(&rt, "ai-tab-workflows").item_selected);
    assert!(!element(&rt, "ai-tab-chat").item_selected);
    assert!(has_key(&rt, "ai-open-workflow-help"));
    assert_eq!(
        a11y_label(&rt, "ai-open-workflow-help").as_deref(),
        Some(t("docs.openWorkflowHelp"))
    );
    for gone in [
        "ai-session-history-toggle",
        "ai-open-ai-help",
        "ai-clear-chat",
        "ai-agent-mode-row",
        "ai-profile-row",
        "ai-share-editor-row",
    ] {
        assert!(!has_key(&rt, gone), "{gone} is chat-only and hides");
    }
    click(&mut rt, "ai-tab-chat");
    assert_eq!(
        rt.state.tabs,
        vec![AiChatTab::Workflows, AiChatTab::Chat],
        "switching back reports the tab again"
    );
    assert!(has_key(&rt, "ai-agent-mode-row"));
}

#[test]
fn keyboard_enter_on_a_focused_tab_switches_tabs() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    focus_by_tab(&mut rt, "ai-tab-workflows");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.tabs, vec![AiChatTab::Workflows]);
    assert_eq!(rt.state.header.tab(), AiChatTab::Workflows);
}

#[test]
fn history_toggle_reports_and_expands() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert_eq!(
        element(&rt, "ai-session-history-toggle").expanded,
        Some(false)
    );
    click(&mut rt, "ai-session-history-toggle");
    assert_eq!(rt.state.history_toggles, 1);
    assert!(rt.state.header.session_history_open());
    assert_eq!(
        element(&rt, "ai-session-history-toggle").expanded,
        Some(true)
    );
    click(&mut rt, "ai-session-history-toggle");
    assert_eq!(rt.state.history_toggles, 2);
    assert!(!rt.state.header.session_history_open());
    assert_eq!(
        element(&rt, "ai-session-history-toggle").expanded,
        Some(false)
    );
}

#[test]
fn keyboard_enter_on_the_focused_history_toggle_toggles_it() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    focus_by_tab(&mut rt, "ai-session-history-toggle");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.history_toggles, 1);
    assert!(rt.state.header.session_history_open());
    assert_eq!(
        element(&rt, "ai-session-history-toggle").expanded,
        Some(true)
    );
}

#[test]
fn help_buttons_report_their_doc_targets() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    click(&mut rt, "ai-open-ai-help");
    assert_eq!(
        rt.state.ai_help, 1,
        "the chat help reports openDocsWindow('context')"
    );
    assert_eq!(rt.state.workflow_help, 0);
    click(&mut rt, "ai-tab-workflows");
    click(&mut rt, "ai-open-workflow-help");
    assert_eq!(
        rt.state.workflow_help, 1,
        "the workflows help reports openDocsWindow('workflows')"
    );
}

#[test]
fn clear_is_disabled_without_messages_and_reports_with_them() {
    let mut rt = runtime(probe(), 960.0, 640.0);
    assert!(
        has_key(&rt, "ai-clear-chat"),
        "React always renders the clear button"
    );
    click(&mut rt, "ai-clear-chat");
    assert_eq!(
        rt.state.clears, 0,
        "a disabled clear button never fires, as in React"
    );

    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    click(&mut rt, "ai-clear-chat");
    assert_eq!(rt.state.clears, 1, "with messages, clear reports clearChat");
}

// ---- agent-mode row ------------------------------------------------------------

#[test]
fn agent_mode_buttons_switch_the_hint() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert_eq!(
        a11y_label(&rt, "ai-agent-mode-ask").as_deref(),
        Some(t("chat.mode.ask")),
        "kit buttons carry their label as the accessible name"
    );
    assert_eq!(
        a11y_label(&rt, "ai-agent-mode-agent").as_deref(),
        Some(t("chat.mode.agent"))
    );
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        t("chat.mode.askHint")
    );
    assert!(element(&rt, "ai-agent-mode-ask").item_selected);
    assert!(!element(&rt, "ai-agent-mode-agent").item_selected);
    assert!(
        element(&rt, "ai-agent-mode-ask").fill.a > 0.0,
        "the active mode carries the accent wash"
    );
    assert_eq!(element(&rt, "ai-agent-mode-agent").fill.a, 0.0);

    click(&mut rt, "ai-agent-mode-agent");
    assert_eq!(rt.state.modes, vec![AiAgentMode::Agent]);
    assert_eq!(rt.state.header.agent_mode(), AiAgentMode::Agent);
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        t("chat.mode.agentHint")
    );
    assert!(element(&rt, "ai-agent-mode-agent").item_selected);
    assert!(!element(&rt, "ai-agent-mode-ask").item_selected);

    click(&mut rt, "ai-agent-mode-ask");
    assert_eq!(
        rt.state.modes,
        vec![AiAgentMode::Agent, AiAgentMode::Ask],
        "switching back reports the mode again"
    );
    assert_eq!(
        element_text(&rt, "ai-agent-mode-hint"),
        t("chat.mode.askHint")
    );
}

#[test]
fn template_select_is_hidden_without_templates() {
    let rt = runtime(probe(), 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-template-select"),
        "React only renders the select while templates exist"
    );
    assert!(has_key(&rt, "ai-agent-mode-row"));
    // Without templates there is no spacer either: the hint is the last child.
    assert!(!has_key(&rt, "ai-agent-mode-spacer"));
}

#[test]
fn template_select_opens_and_chooses_by_pointer() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-template-select"));
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some(t("chat.template.auto"))),
        "no template chosen yet: the trigger shows the auto label"
    );

    click(&mut rt, "ai-template-select-field-trigger");
    assert!(rt.state.header.template_open(), "a press opens the listbox");
    assert!(has_key(&rt, "ai-template-select-field-content"));
    assert_eq!(
        element(&rt, "option0").label.as_deref(),
        Some(t("chat.template.auto")),
        "the first row is the automatic choice"
    );
    assert_eq!(
        element(&rt, "option1").label.as_deref(),
        Some("SQL Review"),
        "then the templates in order"
    );
    assert_eq!(
        element(&rt, "option2").label.as_deref(),
        Some("tpl-empty"),
        "an empty template name falls back to its id"
    );

    click(&mut rt, "option1");
    assert_eq!(rt.state.template_choices, vec!["tpl-sql".to_string()]);
    assert_eq!(rt.state.header.template_id(), "tpl-sql");
    assert!(
        !rt.state.header.template_open(),
        "the app's handler closes the listbox"
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("SQL Review")),
        "the trigger now shows the chosen template"
    );
}

#[test]
fn template_select_reports_values_identically_from_keys_and_pointer() {
    let mut keys = runtime(rich_probe(), 960.0, 640.0);
    focus_by_tab(&mut keys, "ai-template-select-field-trigger");
    press(&mut keys, Key::Text(" ".into()));
    assert!(keys.state.header.template_open(), "Space opens the listbox");
    press(&mut keys, Key::Down);
    press(&mut keys, Key::Down);
    assert_eq!(
        focused_key(&keys).as_deref(),
        Some("option2"),
        "two Downs land on the third row"
    );
    press(&mut keys, Key::Enter);
    assert_eq!(keys.state.template_choices, vec!["tpl-empty".to_string()]);
    assert_eq!(keys.state.header.template_id(), "tpl-empty");
    assert!(!keys.state.header.template_open());

    let mut pointer = runtime(rich_probe(), 960.0, 640.0);
    click(&mut pointer, "ai-template-select-field-trigger");
    click(&mut pointer, "option2");
    assert_eq!(pointer.state.template_choices, keys.state.template_choices);
}

// ---- profile row -----------------------------------------------------------------

#[test]
fn profile_row_is_hidden_without_profiles() {
    let rt = runtime(probe(), 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-profile-row"),
        "React only renders the row while profiles exist"
    );
}

#[test]
fn profile_row_shows_the_active_profile_with_the_preset_shield() {
    // Cloud profile: active by id, cloud_strict gate, blue shield.
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(has_key(&rt, "ai-profile-row"));
    assert_eq!(element_text(&rt, "ai-profile-label"), "Cloud:");
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_cloud_strict(true)),
        "cloud_strict paints the blue shield"
    );
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(rune::color::hex("#60a5fa").unwrap_or(rune::Color::WHITE)),
        "the blue is React's text-blue-400, not an invented hue"
    );

    // Local-trust profile: the default, emerald shield, model as label.
    let mut local = probe();
    local.header.set_profiles(profiles());
    let rt = runtime(local, 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-profile-label"), "ollama:");
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_local_trust(true)),
        "local_trust paints the emerald shield"
    );
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(rune::color::hex("#34d399").unwrap_or(rune::Color::WHITE)),
        "the emerald is React's text-emerald-400"
    );

    // Enterprise profile: amber shield.
    let mut enterprise = probe();
    enterprise.header.set_profiles(vec![
        AiProfile::new("p-ent", "Corp", "m").safety_gate(AiSafetyGate::enterprise_balanced())
    ]);
    enterprise.header.set_active_profile_id("p-ent");
    let rt = runtime(enterprise, 960.0, 640.0);
    assert_eq!(element_text(&rt, "ai-profile-label"), "Corp:");
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_enterprise_balanced(true)),
        "enterprise_balanced paints the amber shield"
    );
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(rune::color::hex("#fbbf24").unwrap_or(rune::Color::WHITE)),
        "the amber is React's text-amber-400"
    );

    // A custom gate falls back to blue, as React's default branch does.
    let mut custom = probe();
    custom
        .header
        .set_profiles(vec![AiProfile::new("p-x", "X", "m").safety_gate(
            AiSafetyGate::new("strict", "require_confirm", "disabled", true),
        )]);
    custom.header.set_active_profile_id("p-x");
    let rt = runtime(custom, 960.0, 640.0);
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_cloud_strict(true)),
        "a custom gate paints blue, like cloud_strict"
    );
}

#[test]
fn profile_select_chooses_by_pointer() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Cloud")),
        "the trigger shows the active profile"
    );
    click(&mut rt, "ai-profile-select-field-trigger");
    assert!(rt.state.header.profile_open());
    assert_eq!(
        element(&rt, "option0").label.as_deref(),
        Some("Cloud"),
        "profiles list in order"
    );
    assert_eq!(
        element(&rt, "option1").label.as_deref(),
        Some("ollama"),
        "an empty profile name falls back to its model"
    );
    click(&mut rt, "option1");
    assert_eq!(rt.state.profile_choices, vec!["p-local".to_string()]);
    assert_eq!(rt.state.header.active_profile_id(), "p-local");
    assert!(!rt.state.header.profile_open());
    assert_eq!(
        element_text(&rt, "ai-profile-label"),
        "ollama:",
        "the label follows the new active profile"
    );
    assert_eq!(
        element(&rt, "ai-profile-shield").text_color,
        Some(ai_shield_local_trust(true)),
        "and so does the shield"
    );
}

// ---- share-editor row ---------------------------------------------------------------

#[test]
fn share_switch_toggles_only_when_sharing_is_possible() {
    let mut rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-share-editor-unavailable"),
        "no note while an editor session can be shared"
    );
    assert!(
        !a11y_states(&rt, "ai-share-editor-toggle").contains(&"checked".to_string()),
        "unchecked at first"
    );
    click(&mut rt, "ai-share-editor-toggle");
    assert_eq!(rt.state.sharing_toggles, 1);
    assert!(rt.state.header.sharing());
    assert!(
        a11y_states(&rt, "ai-share-editor-toggle").contains(&"checked".to_string()),
        "the switch reports checked in the accessibility tree"
    );

    let mut rt = runtime(probe(), 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-share-editor-unavailable"),
        t("chat.shareEditor.unavailable")
    );
    click(&mut rt, "ai-share-editor-toggle");
    assert_eq!(
        rt.state.sharing_toggles, 0,
        "a disabled switch never fires, as in React"
    );
    assert!(!rt.state.header.sharing());
}

#[test]
fn share_error_renders_as_an_alert_with_the_error_filled_in() {
    let mut probe = rich_probe();
    probe
        .header
        .set_share_error(Some("grant denied".to_string()));
    let rt = runtime(probe, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-share-editor-error"),
        t("chat.shareEditor.error").replacen("{error}", "grant denied", 1)
    );
    assert_eq!(
        element(&rt, "ai-share-editor-error").role,
        "alert",
        "React marks the error role=\"alert\""
    );
    assert_eq!(
        element(&rt, "ai-share-editor-error").text_color,
        Some(crate::ui::kit::kit(true).c.danger.fg),
        "the error is danger ink"
    );
}

// ---- migration banners ---------------------------------------------------------------

#[test]
fn migration_banners_render_error_then_success_and_hide_otherwise() {
    let rt = runtime(rich_probe(), 960.0, 640.0);
    assert!(
        !has_key(&rt, "ai-session-migration-banner"),
        "no notice, no banner"
    );
    assert!(!has_key(&rt, "ai-session-migration-success-banner"));

    let mut error = rich_probe();
    error
        .header
        .set_session_migration_error(Some("migration failed".to_string()));
    error.header.set_session_migration_success(3);
    let rt = runtime(error, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-session-migration-banner-text"),
        "migration failed"
    );
    assert!(
        !has_key(&rt, "ai-session-migration-success-banner"),
        "the error banner wins over success, as React's gate does"
    );
    assert_eq!(
        element(&rt, "ai-session-migration-banner-text").text_color,
        Some(crate::ui::kit::kit(true).c.danger.fg)
    );

    let mut success = rich_probe();
    success.header.set_session_migration_success(3);
    let rt = runtime(success, 960.0, 640.0);
    assert_eq!(
        element_text(&rt, "ai-session-migration-success-banner-text"),
        t("chat.sessions.migrationSuccess").replacen("{count}", "3", 1)
    );
    assert_eq!(
        element(&rt, "ai-session-migration-success-banner-text").text_color,
        Some(crate::ui::kit::kit(true).c.success.fg)
    );
    assert!(!has_key(&rt, "ai-session-migration-banner"));
}

// ---- geometry + unwired render ----------------------------------------------------------

#[test]
fn header_stays_inside_both_windows_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = rich_probe();
            probe.dark = dark;
            probe.header.set_session_migration_error(Some(
                "a long migration failure that wraps onto a second line".to_string(),
            ));
            probe.header.set_templates(templates());
            let rt = runtime(probe, w, h);
            assert_in_window(&rt, w, h, &format!("{w}x{h} dark={dark}"));
            assert_no_sibling_overlap(&rt, &format!("{w}x{h} dark={dark}"));
            // The template select sits at the right edge of its row.
            let row = element(&rt, "ai-agent-mode-row").rect;
            let select = element(&rt, "ai-template-select").rect;
            assert!(
                (select[0] + select[2] - (row[0] + row[2] - 12.0)).abs() <= 1.0,
                "{w}x{h} dark={dark}: the select ends at the row's right padding"
            );
            // Rows stack in React order: header, mode, profile, share.
            let header = element(&rt, "ai-chat-header-row").rect;
            let mode = element(&rt, "ai-agent-mode-row").rect;
            let profile = element(&rt, "ai-profile-row").rect;
            let share = element(&rt, "ai-share-editor-row").rect;
            assert!(
                header[1] + header[3] <= mode[1] + 0.5
                    && mode[1] + mode[3] <= profile[1] + 0.5
                    && profile[1] + profile[3] <= share[1] + 0.5,
                "{w}x{h} dark={dark}: rows stack top to bottom without overlap"
            );
        }
    }
}

#[test]
fn unwired_header_still_renders() {
    // No callbacks at all (and no templates / profiles, so no select is
    // built): every `None` arm of the wiring renders inertly instead of
    // panicking, the way S1's unwired chip does.
    fn unwired_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
        let mut layer = Vec::new();
        let header = AiChatHeader::<Probe>::new(&state.header).build(state.dark, &mut layer);
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
    let probe = probe();
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, unwired_view, (960.0, 640.0), env(dark));
    rt.frame(0.0);
    for key in [
        "ai-chat-header",
        "ai-tab-chat",
        "ai-tab-workflows",
        "ai-session-history-toggle",
        "ai-agent-mode-row",
        "ai-agent-mode-ask",
        "ai-agent-mode-agent",
        "ai-agent-mode-hint",
        "ai-share-editor-row",
        "ai-share-editor-toggle",
    ] {
        assert!(has_key(&rt, key), "unwired header still lays out {key}");
    }
}
