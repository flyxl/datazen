//! T087-S3: chat session actions of `stores/aiStore.ts`.

use crate::ai_store::AiStore;
use crate::ai_store_chat::{
    outgoing_message, ChatFollowUp, ChatMessage, ChatSession, ChatSessionSummary, LoadedSkill,
    RestoredChat, ToolCall, DELETE_WHILE_STREAMING, PERSIST_FAILED,
};

fn session(id: &str, messages: Vec<ChatMessage>) -> ChatSession {
    ChatSession {
        id: id.into(),
        session_key: "k".into(),
        connection_id: Some("c1".into()),
        database: Some("app".into()),
        messages,
        is_streaming: false,
        stream_content: String::new(),
        request_id: None,
        loaded_skills: vec![LoadedSkill {
            id: String::new(),
            name: "sql".into(),
            source: None,
            loaded_at: String::new(),
        }],
        template_id: Some("tmpl".into()),
        agent_mode: None,
        memory_mode: None,
        memory_scope: None,
        injection: None,
    }
}

fn user(id: &str) -> ChatMessage {
    ChatMessage {
        id: id.into(),
        role: "user".into(),
        content: "hi".into(),
        tool_call_id: None,
        tool_calls: Vec::new(),
        status: None,
        created_at: "t".into(),
    }
}

#[test]
fn init_applies_the_restore_only_while_the_token_and_message_count_hold() {
    let mut store = AiStore::new();
    let token = store.begin_init_chat(session("local", vec![]));
    assert_eq!(token, 1);
    assert!(store.session_history_loading);
    assert!(store.chat_sessions.is_empty());

    let stale = store.complete_init_chat(
        0,
        RestoredChat {
            session: session("remote", vec![]),
            summaries: vec![ChatSessionSummary {
                id: "remote".into(),
                title: "Remote".into(),
            }],
            migration_error: None,
            migration_success: 2,
        },
    );
    assert!(!stale);
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("local"));
    assert!(store.session_history_loading);

    store
        .chat_session
        .as_mut()
        .expect("session")
        .messages
        .push(user("m"));
    assert!(!store.complete_init_chat(
        token,
        RestoredChat {
            session: session("remote", vec![]),
            summaries: Vec::new(),
            migration_error: None,
            migration_success: 0,
        },
    ));
    assert_eq!(store.chat_session.as_ref().map(|s| s.messages.len()), Some(1));

    let token = store.begin_init_chat(session("local-2", vec![]));
    assert!(store.complete_init_chat(
        token,
        RestoredChat {
            session: session("remote-2", vec![]),
            summaries: vec![ChatSessionSummary {
                id: "remote-2".into(),
                title: "R".into(),
            }],
            migration_error: Some("moved".into()),
            migration_success: 3,
        },
    ));
    assert!(!store.session_history_loading);
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("remote-2"));
    assert_eq!(store.session_migration_success, 3);
    assert_eq!(store.session_migration_error.as_deref(), Some("moved"));
}

#[test]
fn streaming_blocks_select_new_and_delete() {
    let mut store = AiStore::new();
    assert!(!store.begin_select_session());
    assert!(!store.new_chat_session("n"));
    assert!(!store.begin_delete_session());
    assert_eq!(store.chat_init_token, 0);

    let _ = store.begin_init_chat(session("s", vec![]));
    store.chat_session.as_mut().expect("session").is_streaming = true;
    let token = store.chat_init_token;
    assert!(!store.begin_select_session());
    assert!(!store.new_chat_session("n"));
    assert_eq!(store.chat_init_token, token);
    assert!(!store.begin_delete_session());
    assert_eq!(store.session_history_error.as_deref(), Some(DELETE_WHILE_STREAMING));
    // The refusal only sets the error. It does not clear a load already in flight.
    assert!(store.session_history_loading);
}

#[test]
fn delete_of_the_open_session_installs_the_replacement_and_drops_the_summary() {
    let mut store = AiStore::new();
    let _ = store.begin_init_chat(session("open", vec![user("a")]));
    store.chat_sessions = vec![
        ChatSessionSummary {
            id: "open".into(),
            title: "Open".into(),
        },
        ChatSessionSummary {
            id: "other".into(),
            title: "Other".into(),
        },
    ];
    assert!(store.begin_delete_session());
    store.complete_delete_session("other", true, None, None);
    assert!(store.session_history_error.is_none());
    assert_eq!(store.chat_sessions.len(), 1);
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("open"));

    assert!(store.begin_delete_session());
    store.complete_delete_session(
        "open",
        false,
        Some("backend"),
        Some(session("fresh", vec![])),
    );
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("fresh"));
    assert!(store.chat_sessions.is_empty());
    assert_eq!(store.session_history_error.as_deref(), Some("backend"));
    assert!(!store.session_history_loading);
}

#[test]
fn send_uses_a_tool_result_only_for_ask_questions_and_clear_keeps_the_template() {
    let mut last = user("q");
    last.tool_calls.push(ToolCall {
        id: "call-1".into(),
        name: "ask_questions".into(),
    });
    let tool = outgoing_message(Some(&last), "yes", "m1", "now");
    assert_eq!(tool.role, "tool");
    assert_eq!(tool.tool_call_id.as_deref(), Some("call-1"));

    let mut other = user("q");
    other.tool_calls.push(ToolCall {
        id: "call-2".into(),
        name: "read_table".into(),
    });
    assert_eq!(outgoing_message(Some(&other), "next", "m2", "now").role, "user");

    let mut store = AiStore::new();
    assert!(store.begin_send_chat("x", "m", "r", "t", None, None, None, None).is_none());
    let mut open = session("s", vec![last]);
    open.loaded_skills = vec![
        LoadedSkill {
            id: String::new(),
            name: "sql".into(),
            source: None,
            loaded_at: String::new(),
        },
        LoadedSkill {
            id: String::new(),
            name: String::new(),
            source: None,
            loaded_at: String::new(),
        },
    ];
    let _ = store.begin_init_chat(open);
    let sent = store
        .begin_send_chat("answer", "m9", "req", "ts", Some("tmpl"), Some("ask"), None, None)
        .expect("send");
    assert_eq!(sent.request_id, "req");
    assert_eq!(sent.loaded_skill_ids, vec!["sql".to_owned()]);
    assert_eq!(sent.follow_up, ChatFollowUp::Persist);
    let messages = &store.chat_session.as_ref().expect("session").messages;
    assert_eq!(messages.last().map(|m| m.role.as_str()), Some("tool"));
    assert!(store.chat_session.as_ref().expect("session").is_streaming);

    assert_eq!(
        store.fail_send_chat("nope", "err", "ts2"),
        ChatFollowUp::Persist
    );
    let session = store.chat_session.as_ref().expect("session");
    assert!(!session.is_streaming);
    assert!(session.request_id.is_none());
    assert_eq!(session.messages.last().map(|m| m.content.as_str()), Some("Error: nope"));
    assert_eq!(session.messages.last().map(|m| m.status.as_deref()), Some(Some("failed")));

    assert_eq!(store.clear_chat(), ChatFollowUp::Persist);
    let session = store.chat_session.as_ref().expect("session");
    assert!(session.messages.is_empty());
    assert!(session.loaded_skills.is_empty());
    assert_eq!(session.template_id.as_deref(), Some("tmpl"));

    store.note_persist_result(false, None);
    assert_eq!(store.session_migration_error.as_deref(), Some(PERSIST_FAILED));
    store.note_persist_result(true, None);
    assert_eq!(store.session_migration_error.as_deref(), Some(PERSIST_FAILED));
}
