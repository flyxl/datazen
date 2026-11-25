//! Independent audit of T087-S4.
//!
//! The spinner deadline is `started + 100 + 200`. A workflow success does
//! not clear a previous error. MCP save order is removals first, then
//! fingerprint changes, and a disabled change does not reconnect. A skill
//! event whose id is empty matches the name. Injection for a different
//! request does not write.

use datazen_ui::ai_store::AiStore;
use datazen_ui::ai_store_chat::{ChatFollowUp, LoadedSkill};
use datazen_ui::ai_store_rest::{plan_mcp_save, McpClientConfig, McpSaveStep, WorkflowItem};

fn item(id: &str) -> WorkflowItem {
    WorkflowItem {
        id: id.into(),
        name: id.into(),
    }
}

#[test]
fn audit_spinner_deadline_is_three_hundred_ms_and_success_keeps_the_error() {
    let mut store = AiStore::new();
    store.workflow_error = Some("keep".into());
    store.begin_load_workflows(0);
    assert_eq!(
        store.complete_load_workflows(50, vec![item("fast")]),
        None,
        "50ms is under the 100ms show delay"
    );
    assert!(!store.workflows_loading);
    assert_eq!(store.workflows, vec![item("fast")]);

    store.begin_load_workflows(10_000);
    let until = store
        .complete_load_workflows(10_100, vec![item("slow")])
        .expect("shown at 100ms, held 200ms");
    assert_eq!(until, 10_300);
    assert!(store.workflows_loading);
    assert_eq!(store.workflows, vec![item("fast")]);
    assert!(store.release_workflow_load(10_300));
    assert_eq!(store.workflows, vec![item("slow")]);
    assert_eq!(store.workflow_error.as_deref(), Some("keep"));
}

#[test]
fn audit_mcp_plan_and_skill_name_identity() {
    let steps = plan_mcp_save(
        &["keep".into(), "drop".into()],
        &[McpClientConfig {
            id: "keep".into(),
            enabled: true,
            fingerprint: "same".into(),
        }],
        &[
            McpClientConfig {
                id: "keep".into(),
                enabled: false,
                fingerprint: "same".into(),
            },
            McpClientConfig {
                id: "fresh".into(),
                enabled: true,
                fingerprint: "x".into(),
            },
        ],
    );
    assert_eq!(steps, vec![McpSaveStep::Disconnect("drop".into())]);

    let mut store = AiStore::new();
    store.init_workflow_chat("id");
    store.chat_session = store.workflow_chat.clone();
    store.chat_session.as_mut().unwrap().request_id = Some("req".into());
    store.chat_session.as_mut().unwrap().loaded_skills = vec![LoadedSkill {
        id: String::new(),
        name: "beta".into(),
        source: None,
        loaded_at: "t".into(),
    }];
    assert!(!store.note_injection("other", "nope"));
    assert!(store.chat_session.as_ref().unwrap().injection.is_none());
    assert_eq!(
        store.note_skill_used("req", Some(""), "beta", None, "t2"),
        ChatFollowUp::Persist
    );
    let skills = &store.chat_session.as_ref().unwrap().loaded_skills;
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].loaded_at, "t2");
}

#[test]
fn audit_missing_mcp_config_does_not_start_connecting() {
    let mut store = AiStore::new();
    let err = store.begin_connect_mcp("z", false).unwrap_err();
    assert_eq!(err, "MCP server config not found: z");
    assert!(!store.mcp_connecting);
    store.mcp_connecting = true;
    let _ = store.begin_connect_mcp("z", false).unwrap_err();
    assert!(
        store.mcp_connecting,
        "a missing config only writes mcpError"
    );
    store.begin_connect_mcp("z", true).unwrap();
    store.fail_connect_mcp("z", "timeout");
    assert!(store.mcp_error.is_none());
    assert_eq!(
        store.mcp_server_errors.get("z").map(String::as_str),
        Some("timeout")
    );
}
