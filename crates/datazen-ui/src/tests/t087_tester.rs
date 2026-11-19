//! Independent T087 retest of the branches that live in `stores/aiStore.ts`:
//! config `isConfigured`, chat init token / clobber, `ask_questions`, the
//! 100ms/200ms workflow spinner, the MCP reconnect plan, and `String(e)`
//! versus `e.message` as the host is told to pass them.
//!
//! `streaming.ts` / `sessions.ts` forwards are not re-implemented.

use crate::ai_store::{
    next_settings_for_delete, next_settings_for_save, AiModelProfile, AiProviderConfig,
    AiSettingsConfig, AiStore, ConfigStart, ModelInfo, ProfileEdit, ProviderListItem,
};
use crate::ai_store_chat::{
    outgoing_message, ChatFollowUp, ChatMessage, ChatSession, ChatSessionSummary, LoadedSkill,
    MemoryCandidateNotice, RestoredChat, ToolCall, DELETE_WHILE_STREAMING, PERSIST_FAILED,
};
use crate::ai_store_rest::{
    plan_mcp_save, McpClientConfig, McpSaveStep, McpServerStatus, WorkflowItem,
    WORKFLOW_MIN_VISIBLE_MS, WORKFLOW_SHOW_AFTER_MS,
};

fn profile(id: &str, is_default: bool) -> AiModelProfile {
    AiModelProfile {
        id: id.to_owned(),
        name: id.to_owned(),
        is_default,
    }
}

fn settings(active: &str, profiles: Vec<AiModelProfile>) -> AiSettingsConfig {
    AiSettingsConfig {
        active_profile_id: active.to_owned(),
        profiles,
        global_instructions: "keep".into(),
        prompt_templates: vec![],
        memory: Default::default(),
        skills: Default::default(),
    }
}

fn message(id: &str, calls: Vec<ToolCall>) -> ChatMessage {
    ChatMessage {
        id: id.to_owned(),
        role: "assistant".into(),
        content: String::new(),
        tool_call_id: None,
        tool_calls: calls,
        status: None,
        created_at: "t0".into(),
    }
}

fn session(id: &str, messages: Vec<ChatMessage>) -> ChatSession {
    ChatSession {
        id: id.to_owned(),
        session_key: "conn::main".into(),
        connection_id: Some("conn".into()),
        database: Some("main".into()),
        messages,
        is_streaming: false,
        stream_content: "stale".into(),
        request_id: Some("old-req".into()),
        loaded_skills: vec![
            LoadedSkill {
                id: String::new(),
                name: "from-name".into(),
                source: None,
                loaded_at: "t".into(),
            },
            LoadedSkill {
                id: "skill-a".into(),
                name: "ignored-name".into(),
                source: None,
                loaded_at: "t".into(),
            },
            LoadedSkill {
                id: String::new(),
                name: String::new(),
                source: None,
                loaded_at: "t".into(),
            },
        ],
        template_id: Some("kept".into()),
        agent_mode: None,
        memory_mode: None,
        memory_scope: None,
        injection: Some("prior-injection".into()),
    }
}

fn tool(id: &str, name: &str) -> ToolCall {
    ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

#[test]
fn is_configured_follows_config_or_nonempty_profiles_and_delete_forces_false() {
    let mut store = AiStore::new();
    let (start, effect) = store.begin_load_config(false);
    assert_eq!(start, ConfigStart::Skipped);
    assert!(effect.is_none());
    assert!(!store.config_loading);

    let (start, effect) = store.begin_load_config(true);
    assert_eq!(start, ConfigStart::Started);
    assert!(effect.is_some());
    assert!(store.config_loading);
    store.note_listener_installed(false);
    let (_start, again) = store.begin_load_config(true);
    assert!(again.is_some());

    store.complete_load_config(None, Some(settings("p", vec![])));
    assert!(!store.is_configured);
    store.complete_load_config(None, Some(settings("p", vec![profile("p", true)])));
    assert!(store.is_configured);
    store.complete_load_config(
        Some(AiProviderConfig {
            provider_type: "openai".into(),
            model: "m".into(),
            api_key: None,
            endpoint: None,
        }),
        Some(settings("", vec![])),
    );
    assert!(store.is_configured);

    store.complete_delete_config();
    assert!(store.config.is_none());
    assert!(!store.is_configured);
    assert!(store.settings_config.as_ref().unwrap().profiles.is_empty());

    store.settings_config = Some(settings("p", vec![profile("p", true)]));
    store.is_configured = true;
    store.complete_delete_config();
    assert!(!store.is_configured, "deleteConfig sets isConfigured false and does not look at profiles");
    assert_eq!(store.settings_config.as_ref().unwrap().profiles.len(), 1);
}

#[test]
fn profile_save_and_delete_match_the_local_merge() {
    let saved = next_settings_for_save(
        Some(&settings("old", vec![profile("old", true), profile("keep", false)])),
        profile("new", true),
    );
    assert_eq!(saved.active_profile_id, "new");
    assert!(saved.profiles.iter().filter(|item| item.is_default).count() == 1);
    assert!(saved.profiles.iter().any(|item| item.id == "new" && item.is_default));
    assert!(saved.profiles.iter().any(|item| item.id == "old" && !item.is_default));
    assert_eq!(saved.global_instructions, "keep");

    let blank = next_settings_for_save(None, profile("only", false));
    assert_eq!(blank.active_profile_id, "only");
    assert_eq!(blank.profiles.len(), 1);

    let deleted = next_settings_for_delete(
        Some(&settings("gone", vec![profile("gone", true), profile("next", false)])),
        "gone",
    )
    .unwrap();
    assert_eq!(deleted.active_profile_id, "next");
    assert_eq!(deleted.profiles.len(), 1);
    let emptied = next_settings_for_delete(Some(&settings("gone", vec![profile("gone", false)])), "gone")
        .unwrap();
    assert_eq!(emptied.active_profile_id, "");
    assert!(next_settings_for_delete(None, "x").is_none());

    let mut store = AiStore::new();
    assert!(matches!(store.delete_profile("x"), ProfileEdit::Rejected));
    assert!(!store.saving);
}

#[test]
fn provider_failure_keeps_the_list_and_remote_models_survive_an_error() {
    let mut store = AiStore::new();
    store.providers = vec![ProviderListItem {
        provider_type: "openai".into(),
        display_name: "OpenAI".into(),
    }];
    store.config_error = Some("earlier".into());
    store.fail_load_providers();
    assert_eq!(store.providers.len(), 1);
    assert_eq!(store.config_error.as_deref(), Some("earlier"));

    store.remote_models = vec![ModelInfo {
        id: "kept".into(),
        display_name: "Kept".into(),
    }];
    let returned = store.fail_fetch_remote_models("missing key");
    assert!(returned.is_empty());
    assert_eq!(store.remote_models.len(), 1);
    assert!(!store.fetching_remote_models);
    assert_eq!(store.config_error.as_deref(), Some("missing key"));
}

#[test]
fn nl2sql_and_filter_blank_inputs_do_not_clobber_and_failures_keep_request_id() {
    let mut store = AiStore::new();
    store.nl2sql.input = "   \n".into();
    store.nl2sql.generated_sql = "select 1".into();
    assert!(!store.begin_generate_sql("req-1"));
    assert_eq!(store.nl2sql.generated_sql, "select 1");
    store.nl2sql.input = "count users".into();
    assert!(store.begin_generate_sql("req-2"));
    assert!(store.nl2sql.is_generating);
    assert_eq!(store.nl2sql.generated_sql, "");
    store.fail_generate_sql("timeout");
    assert!(!store.nl2sql.is_generating);
    assert_eq!(store.nl2sql.request_id.as_deref(), Some("req-2"));
    assert_eq!(store.nl2sql_error.as_deref(), Some("timeout"));

    store.parsed_filters = Some(vec![]);
    store.nl_filter_input = " \t".into();
    assert!(!store.begin_parse_filter());
    assert!(store.parsed_filters.is_some());
}

#[test]
fn init_token_and_message_count_block_a_late_restore() {
    let mut store = AiStore::new();
    let first = store.begin_init_chat(session("local", vec![message("m", vec![])]));
    assert!(store.session_history_loading);
    assert!(store.memory_candidate_notice.is_none());
    let second = store.begin_init_chat(session("local-2", vec![message("m", vec![])]));
    assert_ne!(first, second);
    let restored = RestoredChat {
        session: session("backend", vec![message("b", vec![])]),
        summaries: vec![ChatSessionSummary {
            id: "backend".into(),
            title: "Backend".into(),
        }],
        migration_error: Some("moved".into()),
        migration_success: 3,
    };
    assert!(!store.complete_init_chat(first, restored.clone()));
    assert_eq!(store.chat_session.as_ref().unwrap().id, "local-2");
    assert!(store.complete_init_chat(second, restored));
    assert_eq!(store.chat_session.as_ref().unwrap().id, "backend");
    assert!(!store.session_history_loading);
    assert_eq!(store.session_migration_success, 3);

    let token = store.begin_init_chat(session("again", vec![message("m", vec![])]));
    let sent = store
        .begin_send_chat("hello", "user-1", "req", "t1", None, Some("ask"), None, None)
        .unwrap();
    assert_eq!(sent.loaded_skill_ids, vec!["from-name".to_owned(), "skill-a".to_owned()]);
    assert_eq!(sent.follow_up, ChatFollowUp::Persist);
    let late = RestoredChat {
        session: session("should-not-land", vec![]),
        summaries: vec![],
        migration_error: None,
        migration_success: 0,
    };
    assert!(!store.complete_init_chat(token, late));
    assert!(store.session_history_loading, "a clobbered restore leaves loading set");
    assert_eq!(store.chat_session.as_ref().unwrap().messages.len(), 2);
    assert!(store.chat_session.as_ref().unwrap().is_streaming);
}

#[test]
fn ask_questions_becomes_a_tool_result_and_any_other_call_stays_a_user_message() {
    let ask = outgoing_message(
        Some(&message(
            "a",
            vec![tool("other", "run_sql"), tool("ask-9", "ask_questions")],
        )),
        "blue",
        "id-1",
        "t1",
    );
    assert_eq!(ask.role, "tool");
    assert_eq!(ask.tool_call_id.as_deref(), Some("ask-9"));
    assert_eq!(ask.content, "blue");

    let user = outgoing_message(
        Some(&message("a", vec![tool("other", "run_sql")])),
        "blue",
        "id-2",
        "t1",
    );
    assert_eq!(user.role, "user");
    assert!(user.tool_call_id.is_none());

    let mut store = AiStore::new();
    store.memory_candidate_notice = Some(MemoryCandidateNotice {
        request_id: "r".into(),
        count: 2,
    });
    store.workflow_chat = Some(session(
        "wf",
        vec![message("a", vec![tool("ask-1", "ask_questions")])],
    ));
    let request = store
        .begin_send_workflow("answer", "mid", "wf-req", "t2")
        .unwrap();
    assert_eq!(request, "wf-req");
    let workflow = store.workflow_chat.as_ref().unwrap();
    assert_eq!(workflow.messages.last().unwrap().role, "tool");
    assert_eq!(workflow.messages.last().unwrap().tool_call_id.as_deref(), Some("ask-1"));
    assert_eq!(store.workflow_attribution.as_deref(), Some("independent"));
    assert!(store.memory_candidate_notice.is_some(), "workflow send does not clear the notice");

    store.fail_send_workflow("socket closed", "err", "t3");
    let failed = store.workflow_chat.as_ref().unwrap().messages.last().unwrap();
    assert_eq!(failed.role, "assistant");
    assert_eq!(failed.content, "Error: socket closed");
    assert!(failed.status.is_none());
    assert!(!store.workflow_chat.as_ref().unwrap().is_streaming);
}

#[test]
fn chat_failure_prefixes_error_and_clear_keeps_injection() {
    let mut store = AiStore::new();
    store.chat_session = Some(session("s", vec![]));
    store.chat_session.as_mut().unwrap().injection = Some("snap".into());
    assert_eq!(
        store.fail_send_chat("socket closed", "e1", "t"),
        ChatFollowUp::Persist
    );
    let failed = store.chat_session.as_ref().unwrap().messages.last().unwrap();
    assert_eq!(failed.content, "Error: socket closed");
    assert_eq!(failed.status.as_deref(), Some("failed"));
    assert_eq!(store.clear_chat(), ChatFollowUp::Persist);
    assert!(store.chat_session.as_ref().unwrap().messages.is_empty());
    assert_eq!(store.chat_session.as_ref().unwrap().injection.as_deref(), Some("snap"));
    assert_eq!(store.chat_session.as_ref().unwrap().template_id.as_deref(), Some("kept"));

    store.note_persist_result(false, None);
    assert_eq!(store.session_migration_error.as_deref(), Some(PERSIST_FAILED));
    store.note_persist_result(true, Some("ignored"));
    assert_eq!(store.session_migration_error.as_deref(), Some(PERSIST_FAILED));
}

#[test]
fn select_and_delete_honor_streaming_and_only_init_checks_the_token() {
    let mut store = AiStore::new();
    assert!(!store.begin_select_session());
    let token = store.begin_init_chat(session("s", vec![message("m", vec![])]));
    store.chat_session.as_mut().unwrap().is_streaming = true;
    assert!(!store.begin_select_session());
    assert_eq!(store.chat_init_token, token);
    assert!(!store.new_chat_session("fresh"));
    assert_eq!(store.chat_init_token, token);
    store.session_history_loading = false;
    store.session_history_error = None;
    assert!(!store.begin_delete_session());
    assert_eq!(store.session_history_error.as_deref(), Some(DELETE_WHILE_STREAMING));
    assert!(!store.session_history_loading, "a streaming delete does not enter the loading state");

    store.chat_session.as_mut().unwrap().is_streaming = false;
    assert!(store.begin_select_session());
    assert_ne!(store.chat_init_token, token);
    assert!(!store.complete_init_chat(
        token,
        RestoredChat {
            session: session("late", vec![]),
            summaries: vec![],
            migration_error: None,
            migration_success: 0,
        }
    ));
    store.chat_sessions = vec![
        ChatSessionSummary {
            id: "s".into(),
            title: "open".into(),
        },
        ChatSessionSummary {
            id: "other".into(),
            title: "other".into(),
        },
    ];
    store.complete_delete_session("s", false, None, Some(session("replacement", vec![])));
    assert_eq!(store.chat_session.as_ref().unwrap().id, "replacement");
    assert_eq!(store.chat_sessions.len(), 1);
    assert!(store.session_history_error.is_none(), "not-ok with a null error stores null");
}

#[test]
fn workflow_spinner_waits_100ms_to_show_and_200ms_once_shown() {
    assert_eq!(WORKFLOW_SHOW_AFTER_MS, 100);
    assert_eq!(WORKFLOW_MIN_VISIBLE_MS, 200);
    let mut store = AiStore::new();
    store.workflow_error = Some("previous".into());
    store.workflows = vec![WorkflowItem {
        id: "old".into(),
        name: "Old".into(),
    }];
    store.begin_load_workflows(1_000);
    store.tick_workflow_load(1_099);
    assert!(!store.workflows_loading);
    assert!(store.complete_load_workflows(
        1_050,
        vec![WorkflowItem {
            id: "fast".into(),
            name: "Fast".into(),
        }]
    ).is_none());
    assert!(!store.workflows_loading);
    assert_eq!(store.workflows[0].id, "fast");
    assert_eq!(store.workflow_error.as_deref(), Some("previous"), "success does not clear workflowError");

    store.begin_load_workflows(2_000);
    let hold = store.fail_load_workflows(2_150, "Error: list failed");
    assert_eq!(hold, Some(2_300));
    assert!(store.workflows_loading);
    assert_eq!(store.workflows[0].id, "fast", "the list stays until the hold elapses");
    assert!(!store.release_workflow_load(2_299));
    assert!(store.release_workflow_load(2_300));
    assert!(!store.workflows_loading);
    assert_eq!(store.workflow_error.as_deref(), Some("Error: list failed"));
    assert_eq!(store.workflows[0].id, "fast");

    store.begin_execute_workflow();
    store.fail_execute_workflow("Error: boom");
    assert!(!store.is_executing_workflow);
    assert_eq!(store.workflow_error.as_deref(), Some("Error: boom"));
    store.fail_schema_doc("boom");
    assert_eq!(store.schema_doc_error.as_deref(), Some("boom"));
}

#[test]
fn mcp_plan_disconnects_removed_and_changed_servers_and_connect_errors_stay_per_server() {
    let steps = plan_mcp_save(
        &["a".into(), "b".into(), "c".into()],
        &[
            McpClientConfig {
                id: "a".into(),
                enabled: true,
                fingerprint: "same".into(),
            },
            McpClientConfig {
                id: "b".into(),
                enabled: true,
                fingerprint: "v1".into(),
            },
            McpClientConfig {
                id: "c".into(),
                enabled: true,
                fingerprint: "v1".into(),
            },
        ],
        &[
            McpClientConfig {
                id: "b".into(),
                enabled: true,
                fingerprint: "v1".into(),
            },
            McpClientConfig {
                id: "c".into(),
                enabled: false,
                fingerprint: "v2".into(),
            },
            McpClientConfig {
                id: "d".into(),
                enabled: true,
                fingerprint: "new".into(),
            },
        ],
    );
    assert_eq!(
        steps,
        vec![
            McpSaveStep::Disconnect("a".into()),
            McpSaveStep::Disconnect("c".into()),
        ]
    );

    let mut store = AiStore::new();
    let missing = store.begin_connect_mcp("ghost", false).unwrap_err();
    assert_eq!(missing, "MCP server config not found: ghost");
    assert_eq!(store.mcp_error.as_deref(), Some("MCP server config not found: ghost"));
    assert!(!store.mcp_connecting);

    store.mcp_server_errors.insert("live".into(), "old".into());
    store.mcp_server_errors.insert("other".into(), "keep".into());
    store.begin_connect_mcp("live", true).unwrap();
    assert!(store.mcp_connecting);
    assert!(store.mcp_error.is_none());
    assert!(!store.mcp_server_errors.contains_key("live"));
    assert_eq!(store.mcp_server_errors["other"], "keep");
    let thrown = store.fail_connect_mcp("live", "refused");
    assert_eq!(thrown, "refused");
    assert!(!store.mcp_connecting);
    assert!(store.mcp_error.is_none(), "connect failure records the per-server map, not mcpError");
    assert_eq!(store.mcp_server_errors["live"], "refused");

    store.mcp_servers = vec![McpServerStatus {
        server_id: "live".into(),
    }];
    store.fail_load_mcp_servers();
    assert_eq!(store.mcp_servers.len(), 1);
    store.fail_disconnect_mcp("still up");
    assert_eq!(store.mcp_error.as_deref(), Some("still up"));
}

#[test]
fn injection_memory_and_skill_events_ignore_a_mismatched_request() {
    let mut store = AiStore::new();
    store.chat_session = Some(session("s", vec![]));
    store.chat_session.as_mut().unwrap().request_id = Some("req".into());
    assert!(!store.note_injection("other", "snap"));
    assert!(store.note_injection("req", "snap"));
    assert_eq!(store.chat_session.as_ref().unwrap().injection.as_deref(), Some("snap"));

    store.memory_candidate_notice = Some(MemoryCandidateNotice {
        request_id: "old".into(),
        count: 4,
    });
    assert!(!store.note_memory_candidates("req", 0));
    assert_eq!(store.memory_candidate_notice.as_ref().unwrap().count, 4);
    assert!(store.note_memory_candidates("req", 2));
    assert_eq!(store.memory_candidate_notice.as_ref().unwrap().count, 2);

    assert_eq!(
        store.note_skill_used("nope", Some("id"), "name", None, "t"),
        ChatFollowUp::None
    );
    assert_eq!(
        store.note_skill_used("req", Some(""), "", None, "t"),
        ChatFollowUp::None
    );
    store.chat_session.as_mut().unwrap().loaded_skills = vec![LoadedSkill {
        id: "skill-a".into(),
        name: "old".into(),
        source: Some("disk".into()),
        loaded_at: "t0".into(),
    }];
    assert_eq!(
        store.note_skill_used("req", Some("skill-a"), "renamed", Some(""), "t1"),
        ChatFollowUp::Persist
    );
    let skills = &store.chat_session.as_ref().unwrap().loaded_skills;
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "renamed");
    assert!(skills[0].source.is_none());
}
