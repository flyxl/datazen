//! T087-S4: workflow chat, workflow spinner, insight actions, MCP, events.

use datazen_ui::ai_store::AiStore;
use datazen_ui::ai_store_chat::{ChatFollowUp, ChatMessage, ToolCall};
use datazen_ui::ai_store_rest::{
    plan_mcp_save, McpClientConfig, McpSaveStep, McpServerStatus, WorkflowItem,
    WORKFLOW_MIN_VISIBLE_MS, WORKFLOW_SHOW_AFTER_MS,
};

fn item(id: &str) -> WorkflowItem {
    WorkflowItem {
        id: id.into(),
        name: id.into(),
    }
}

fn cfg(id: &str, enabled: bool, fingerprint: &str) -> McpClientConfig {
    McpClientConfig {
        id: id.into(),
        enabled,
        fingerprint: fingerprint.into(),
    }
}

#[test]
fn a_fast_workflow_load_never_flashes_and_keeps_the_old_error() {
    let mut store = AiStore::new();
    store.workflow_error = Some("stale".into());
    store.workflows = vec![item("old")];
    store.begin_load_workflows(1_000);
    assert!(!store.workflows_loading);
    let hold = store.complete_load_workflows(1_000 + WORKFLOW_SHOW_AFTER_MS - 1, vec![item("new")]);
    assert_eq!(hold, None);
    assert!(!store.workflows_loading);
    assert_eq!(store.workflows, vec![item("new")]);
    assert_eq!(store.workflow_error.as_deref(), Some("stale"));
}

#[test]
fn a_slow_load_holds_the_spinner_and_a_failure_keeps_the_list() {
    let mut store = AiStore::new();
    store.workflows = vec![item("old")];
    let start = 5_000;
    store.begin_load_workflows(start);
    store.tick_workflow_load(start + WORKFLOW_SHOW_AFTER_MS);
    assert!(store.workflows_loading);
    let until = store
        .fail_load_workflows(start + WORKFLOW_SHOW_AFTER_MS, "Error: boom")
        .expect("hold");
    assert_eq!(until, start + WORKFLOW_SHOW_AFTER_MS + WORKFLOW_MIN_VISIBLE_MS);
    assert!(store.workflows_loading);
    assert_eq!(store.workflows, vec![item("old")]);
    assert!(store.workflow_error.is_none());
    assert!(!store.release_workflow_load(until - 1));
    assert!(store.release_workflow_load(until));
    assert!(!store.workflows_loading);
    assert_eq!(store.workflows, vec![item("old")]);
    assert_eq!(store.workflow_error.as_deref(), Some("Error: boom"));
}

#[test]
fn execute_and_insight_actions_clear_on_the_way_in() {
    let mut store = AiStore::new();
    store.workflow_error = Some("x".into());
    store.workflow_execution_result = Some("old".into());
    store.begin_execute_workflow();
    assert!(store.workflow_error.is_none());
    assert!(store.workflow_execution_result.is_none());
    assert!(store.is_executing_workflow);
    store.fail_execute_workflow("Error: no");
    assert!(!store.is_executing_workflow);
    assert_eq!(store.workflow_error.as_deref(), Some("Error: no"));
    store.clear_workflow_result();
    assert!(store.workflow_error.is_none());

    store.schema_doc = Some("doc".into());
    store.begin_schema_doc();
    assert!(store.schema_doc.is_none());
    store.fail_schema_doc("denied");
    assert!(!store.is_generating_schema_doc);
    assert_eq!(store.schema_doc_error.as_deref(), Some("denied"));
    store.clear_schema_doc();
    assert!(store.schema_doc_error.is_none());

    store.begin_diagnose_connection();
    store.complete_diagnose_connection("fix");
    assert_eq!(store.connection_diagnosis.as_deref(), Some("fix"));
    assert!(!store.is_diagnosing_connection);
    store.clear_connection_diagnosis();
    assert!(store.connection_diagnosis.is_none());

    store.begin_analyze_queries();
    store.complete_analyze_queries("slow");
    assert_eq!(store.query_analysis.as_deref(), Some("slow"));
    store.fail_analyze_queries("later");
    assert_eq!(store.query_analysis.as_deref(), Some("slow"));
    assert_eq!(store.query_analysis_error.as_deref(), Some("later"));
}

#[test]
fn workflow_chat_asks_become_tool_results_and_clear_drops_attribution() {
    let mut store = AiStore::new();
    assert!(store.begin_send_workflow("", "m", "r", "t").is_none());
    store.init_workflow_chat("wf");
    store.memory_candidate_notice = Some(datazen_ui::ai_store_chat::MemoryCandidateNotice {
        request_id: "keep".into(),
        count: 2,
    });
    store
        .workflow_chat
        .as_mut()
        .expect("chat")
        .messages
        .push(ChatMessage {
            id: "q".into(),
            role: "assistant".into(),
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![ToolCall {
                id: "ask-1".into(),
                name: "ask_questions".into(),
            }],
            status: None,
            created_at: "t".into(),
        });
    let request = store
        .begin_send_workflow("blue", "m2", "req", "t2")
        .expect("started");
    assert_eq!(request, "req");
    assert_eq!(store.workflow_attribution.as_deref(), Some("independent"));
    assert_eq!(store.memory_candidate_notice.as_ref().unwrap().count, 2);
    let last = store.workflow_chat.as_ref().unwrap().messages.last().unwrap();
    assert_eq!(last.role, "tool");
    assert_eq!(last.tool_call_id.as_deref(), Some("ask-1"));
    store.fail_send_workflow("down", "e1", "t3");
    let failed = store.workflow_chat.as_ref().unwrap().messages.last().unwrap();
    assert_eq!(failed.role, "assistant");
    assert_eq!(failed.content, "Error: down");
    assert!(failed.status.is_none());
    assert!(store.workflow_attribution.is_some());
    store.clear_workflow_chat();
    assert!(store.workflow_chat.as_ref().unwrap().messages.is_empty());
    assert!(store.workflow_attribution.is_none());
    assert!(store.memory_candidate_notice.is_none());
}

#[test]
fn mcp_connect_miss_is_global_and_a_transport_miss_is_per_server() {
    let mut store = AiStore::new();
    let missing = store.begin_connect_mcp("gone", false).expect_err("missing");
    assert_eq!(missing, "MCP server config not found: gone");
    assert_eq!(store.mcp_error.as_deref(), Some(missing.as_str()));
    assert!(!store.mcp_connecting);

    store.begin_connect_mcp("s1", true).expect("found");
    assert!(store.mcp_connecting);
    assert_eq!(store.mcp_connecting_server_id.as_deref(), Some("s1"));
    assert!(store.mcp_error.is_none());
    store.mcp_server_errors.insert("s1".into(), "old".into());
    store.begin_connect_mcp("s1", true).expect("retry");
    assert!(!store.mcp_server_errors.contains_key("s1"));
    let thrown = store.fail_connect_mcp("s1", "refused");
    assert_eq!(thrown, "refused");
    assert!(!store.mcp_connecting);
    assert!(store.mcp_error.is_none());
    assert_eq!(store.mcp_server_errors.get("s1").map(String::as_str), Some("refused"));

    store.apply_mcp_servers(vec![McpServerStatus {
        server_id: "s1".into(),
    }]);
    store.fail_load_mcp_servers();
    assert_eq!(store.mcp_servers.len(), 1);
    store.apply_mcp_tools(vec!["echo".into()]);
    store.fail_load_mcp_tools();
    assert_eq!(store.mcp_tools, vec!["echo".to_owned()]);
    store.fail_disconnect_mcp("busy");
    assert_eq!(store.mcp_error.as_deref(), Some("busy"));
    store.clear_mcp_error();
    assert!(store.mcp_error.is_none());
    assert!(store.mcp_server_errors.contains_key("s1"));
    store.clear_mcp_server_error("s1");
    assert!(store.mcp_server_errors.is_empty());
    assert_eq!(store.fail_call_mcp("nope"), "nope");
    assert_eq!(store.mcp_error.as_deref(), Some("nope"));
}

#[test]
fn mcp_save_disconnects_removals_and_reconnects_changed_enabled_rows() {
    let connected = vec!["a".into(), "b".into(), "c".into()];
    let prev = vec![cfg("a", true, "1"), cfg("b", true, "1"), cfg("c", true, "1")];
    let next = vec![
        cfg("b", true, "1"),
        cfg("c", true, "2"),
        cfg("d", true, "9"),
    ];
    assert_eq!(
        plan_mcp_save(&connected, &prev, &next),
        vec![
            McpSaveStep::Disconnect("a".into()),
            McpSaveStep::Disconnect("c".into()),
            McpSaveStep::Connect("c".into()),
        ]
    );
    let disabled = vec![cfg("b", false, "9")];
    assert_eq!(
        plan_mcp_save(&["b".into()], &prev, &disabled),
        vec![McpSaveStep::Disconnect("b".into())]
    );
}

#[test]
fn injection_and_skills_follow_the_live_request() {
    let mut store = AiStore::new();
    store.init_workflow_chat("wf");
    assert!(!store.note_injection("r", "snap"));
    store.chat_session = store.workflow_chat.clone();
    store.chat_session.as_mut().unwrap().request_id = Some("r".into());
    assert!(!store.note_injection("other", "snap"));
    assert!(store.note_injection("r", "snap"));
    assert_eq!(
        store.chat_session.as_ref().unwrap().injection.as_deref(),
        Some("snap")
    );
    assert!(!store.note_memory_candidates("r", 0));
    assert!(store.memory_candidate_notice.is_none());
    assert!(store.note_memory_candidates("r", 3));
    assert_eq!(store.memory_candidate_notice.as_ref().unwrap().count, 3);
    assert!(!store.note_memory_candidates("r", 0));
    assert_eq!(store.memory_candidate_notice.as_ref().unwrap().count, 3);

    store.chat_session.as_mut().unwrap().loaded_skills = vec![
        datazen_ui::ai_store_chat::LoadedSkill {
            id: "skill-a".into(),
            name: "A".into(),
            source: None,
            loaded_at: "t0".into(),
        },
        datazen_ui::ai_store_chat::LoadedSkill {
            id: String::new(),
            name: "plain".into(),
            source: None,
            loaded_at: "t0".into(),
        },
    ];
    assert_eq!(
        store.note_skill_used("nope", Some("skill-a"), "A2", None, "t1"),
        ChatFollowUp::None
    );
    assert_eq!(
        store.note_skill_used("r", Some(""), "", None, "t1"),
        ChatFollowUp::None
    );
    assert_eq!(
        store.note_skill_used("r", Some("skill-a"), "A2", Some(""), "t1"),
        ChatFollowUp::Persist
    );
    let skills = &store.chat_session.as_ref().unwrap().loaded_skills;
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].name, "plain");
    assert_eq!(skills[1].name, "A2");
    assert!(skills[1].source.is_none());
    assert_eq!(
        store.note_skill_used("r", None, "plain", Some("user"), "t2"),
        ChatFollowUp::Persist
    );
    let skills = &store.chat_session.as_ref().unwrap().loaded_skills;
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[1].name, "plain");
    assert_eq!(skills[1].source.as_deref(), Some("user"));
}
