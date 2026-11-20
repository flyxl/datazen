//! T198 audit: the AiToolBridgeSection port, verified against the React
//! source re-read line by line (there is no dedicated React test for
//! this file — its only React coverage is the mount-effect wiring in
//! AiSettingsSection.test.tsx, mirrored by the load-seam pins below).
//! Element evidence comes from `Runtime::elements()`, presses go
//! through real pointer events.
//!
//! Tester-written: every expectation below is re-derived from the JSX
//! and its handlers.

use rune::{col, Env, El, Runtime};

use crate::views::ai_tool_bridge_section::{
    toggle_tool_id, AiToolBridgeSection, AiToolBridgeSectionState, BridgeAiToolVm, BridgeWappVm,
    BridgeWorkflowVm,
};

fn wapp(id: &str, name: &str, enabled: bool, tools: &[(&str, bool)]) -> BridgeWappVm {
    BridgeWappVm {
        id: id.to_owned(),
        name: name.to_owned(),
        enabled,
        ai_tools: tools
            .iter()
            .map(|(name, read_only)| BridgeAiToolVm {
                name: name.to_string(),
                read_only: *read_only,
            })
            .collect(),
    }
}

fn workflow(id: &str, name: &str, description: &str) -> BridgeWorkflowVm {
    BridgeWorkflowVm {
        id: id.to_owned(),
        name: name.to_owned(),
        description: description.to_owned(),
    }
}

#[derive(Clone)]
struct AuditState {
    bridge: AiToolBridgeSectionState,
    wapps: Vec<BridgeWappVm>,
    plugin_ids: Vec<String>,
    workflow_ids: Vec<String>,
    dark: bool,
    plugin_toggles: Vec<(String, bool)>,
    workflow_toggles: Vec<(String, bool)>,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            bridge: AiToolBridgeSectionState::new(),
            wapps: Vec::new(),
            plugin_ids: Vec::new(),
            workflow_ids: Vec::new(),
            dark: true,
            plugin_toggles: Vec::new(),
            workflow_toggles: Vec::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = AiToolBridgeSection::<AuditState>::new(
        &s.bridge,
        &s.wapps,
        &s.plugin_ids,
        &s.workflow_ids,
    )
    .on_toggle_plugin(|s: &mut AuditState, id: String, enabled: bool| {
        s.plugin_ids = toggle_tool_id(&s.plugin_ids, &id, enabled);
        s.plugin_toggles.push((id, enabled));
    })
    .on_toggle_workflow(|s: &mut AuditState, id: String, enabled: bool| {
        s.workflow_ids = toggle_tool_id(&s.workflow_ids, &id, enabled);
        s.workflow_toggles.push((id, enabled));
    })
    .build(s.dark, &mut layer);
    col().max_w(640.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt =
        Runtime::new(scene, view, (960.0, 640.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the toggle's set logic (the handlers' whole body) -----------------------

#[test]
fn toggle_tool_id_enables_by_deduped_append() {
    let ids: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    // Enable a new id: appended at the end.
    assert_eq!(toggle_tool_id(&ids, "c", true), vec!["a", "b", "c"]);
    // Enable an existing id: deduped, moved to the end.
    assert_eq!(toggle_tool_id(&ids, "a", true), vec!["b", "a"]);
    // Disable: removed wherever it sat.
    assert_eq!(toggle_tool_id(&ids, "a", false), vec!["b"]);
    assert_eq!(toggle_tool_id(&ids, "c", false), vec!["a", "b"]);
    // A no-op disable stays a no-op.
    assert_eq!(toggle_tool_id(&[], "a", false), Vec::<String>::new());
}

// ---- the bridge filter and the rows ------------------------------------------

/// Only wapps declaring at least one AI tool are bridge plugins; the
/// rest never render a row.
#[test]
fn only_wapps_with_tools_render_plugin_rows() {
    let mut scene = AuditState::empty();
    scene.wapps = vec![
        wapp("with-tools", "Query helper", true, &[("run_query", true)]),
        wapp("no-tools", "Plain wapp", true, &[]),
        wapp("disabled-tools", "Sleepy", false, &[("dream", false)]),
    ];
    let rt = rt(scene);
    assert!(has(&rt, "ai-tool-bridge-section"));
    assert!(has(&rt, "ai-plugin-tool-with-tools"));
    assert!(has(&rt, "ai-plugin-tool-disabled-tools"));
    assert!(!has(&rt, "ai-plugin-tool-no-tools"), "no declared tools, no row");
    assert!(!has(&rt, "ai-tool-bridge-section-plugins-empty"));
}

/// The empty plugins line.
#[test]
fn no_plugins_renders_the_empty_line() {
    let scene = AuditState::empty();
    let rt = rt(scene);
    assert!(has(&rt, "ai-tool-bridge-section-plugins-empty"));
    assert_eq!(
        text_of(&rt, "ai-tool-bridge-section-plugins-empty"),
        "没有已安装插件声明 AI 工具。"
    );
    // No workflows either: its own empty line (the error is absent).
    assert!(has(&rt, "ai-tool-bridge-section-workflows-empty"));
    assert_eq!(
        text_of(&rt, "ai-tool-bridge-section-workflows-empty"),
        "没有已保存的 Workflow。"
    );
}

/// The rows press into the host with the right id and the flipped value,
/// and the state's set logic applies (the handler's contract).
#[test]
fn plugin_presses_toggle_the_setting() {
    let mut scene = AuditState::empty();
    scene.wapps = vec![wapp("w1", "Helper", true, &[("run_query", true)])];
    let mut rt = rt(scene);
    press(&mut rt, "ai-plugin-tool-w1");
    let state = rt.state.clone();
    assert_eq!(state.plugin_ids, vec!["w1".to_owned()]);
    assert_eq!(state.plugin_toggles, vec![("w1".to_owned(), true)]);
    // Press again: off.
    press(&mut rt, "ai-plugin-tool-w1");
    let state = rt.state.clone();
    assert!(state.plugin_ids.is_empty());
    assert_eq!(state.plugin_toggles.last(), Some(&("w1".to_owned(), false)));
}

/// Workflow rows: press toggles `aiWorkflowToolIds`; the empty line
/// yields to rows; the hint falls back to the id when the description is
/// empty.
#[test]
fn workflow_rows_toggle_and_fall_back() {
    let mut scene = AuditState::empty();
    scene.bridge.note_workflow_rows(Some(vec![
        workflow("wf-1", "Nightly ETL", "Rebuilds the warehouse"),
        workflow("wf-2", "Unnamed", ""),
    ]));
    let mut rt = rt(scene);
    assert!(has(&rt, "ai-workflow-tool-wf-1"));
    assert!(has(&rt, "ai-workflow-tool-wf-2"));
    assert!(!has(&rt, "ai-tool-bridge-section-workflows-empty"));
    press(&mut rt, "ai-workflow-tool-wf-1");
    let state = rt.state.clone();
    assert_eq!(state.workflow_ids, vec!["wf-1".to_owned()]);
    assert_eq!(state.workflow_toggles, vec![("wf-1".to_owned(), true)]);
    press(&mut rt, "ai-workflow-tool-wf-2");
    let state = rt.state.clone();
    // Both switches are on: the list holds both ids (independent rows).
    assert_eq!(state.workflow_ids, vec!["wf-1".to_owned(), "wf-2".to_owned()]);
}

/// The workflow load seam: a null answer is an empty list, a rejection
/// is the error line (and the empty line yields to it).
#[test]
fn the_workflow_load_seam() {
    let mut state = AiToolBridgeSectionState::new();
    // `rows ?? []`: None is empty, not an error.
    state.note_workflow_rows(None);
    assert!(state.workflows().is_empty());
    assert!(state.workflow_error().is_none());
    state.note_workflow_rows(Some(vec![workflow("wf-1", "A", "desc")]));
    assert_eq!(state.workflows().len(), 1);
    // A rejection surfaces the message and keeps the rows.
    state.note_workflow_error("backend unreachable".to_owned());
    assert_eq!(state.workflow_error(), Some("backend unreachable"));
    assert_eq!(state.workflows().len(), 1);
    // A later success clears the error.
    state.note_workflow_rows(None);
    assert!(state.workflow_error().is_none());

    // The rendered halves: the error line replaces the empty line.
    let mut scene = AuditState::empty();
    scene.bridge.note_workflow_error("backend unreachable".to_owned());
    let rt = rt(scene);
    assert!(has(&rt, "ai-tool-bridge-section-error"));
    assert_eq!(
        text_of(&rt, "ai-tool-bridge-section-error"),
        "backend unreachable"
    );
    assert!(!has(&rt, "ai-tool-bridge-section-workflows-empty"));
}

/// The plugin hint: enabled wapps spell `{id} · {tools}` with read-only
/// markers; disabled wapps say the disabled line.
#[test]
fn the_plugin_hint_spells_the_tool_list() {
    let mut scene = AuditState::empty();
    scene.wapps = vec![
        wapp("w1", "Helper", true, &[("run_query", true), ("list", false)]),
        wapp("w2", "Sleepy", false, &[("dream", false)]),
    ];
    let rt = rt(scene);
    // The hint rides the checkbox's description; the audit reads the
    // hint text from the row's description node.
    let hint = rt
        .elements()
        .into_iter()
        .find(|e| {
            !e.exiting
                && e.text
                    .as_deref()
                    .is_some_and(|t| t.starts_with("w1 ·"))
        })
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(hint, "w1 · run_query (read-only), list");
    // The disabled wapp's hint is the disabled line.
    let disabled_hint = rt
        .elements()
        .into_iter()
        .find(|e| {
            !e.exiting && e.text.as_deref().is_some_and(|t| t == "请先启用该插件。")
        })
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(disabled_hint, "请先启用该插件。");
}

/// A stored id renders checked (absent from the list = unchecked).
#[test]
fn stored_ids_render_checked() {
    let mut scene = AuditState::empty();
    scene.wapps = vec![wapp("w1", "Helper", true, &[("t", true)])];
    scene.plugin_ids = vec!["w1".to_owned()];
    let mut rt = rt(scene);
    // Pressing the checked row flips it off.
    press(&mut rt, "ai-plugin-tool-w1");
    assert!(rt.state.plugin_ids.is_empty());
}
