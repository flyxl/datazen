//! Independent audit of T087-S3 chat session actions.
//!
//! Fresh cases: a streaming session rejects the restore even when the
//! token matches and the message count is unchanged; select of a quiet
//! session bumps the token so an older init cannot land; deleting a
//! non-open id keeps the open session; a failed persist with an explicit
//! error uses that error rather than the fallback sentence.

use crate::ai_store::AiStore;
use crate::ai_store_chat::{
    ChatMessage, ChatSession, ChatSessionSummary, RestoredChat, ToolCall, PERSIST_FAILED,
};

fn quiet(id: &str) -> ChatSession {
    ChatSession {
        id: id.into(),
        session_key: "scope".into(),
        connection_id: None,
        database: Some("db".into()),
        messages: Vec::new(),
        is_streaming: false,
        stream_content: String::new(),
        request_id: None,
        loaded_skills: Vec::new(),
        template_id: None,
        agent_mode: None,
        memory_mode: None,
        memory_scope: None,
        injection: None,
    }
}

#[test]
fn audit_streaming_blocks_the_restore_and_select_invalidates_the_old_token() {
    let mut store = AiStore::new();
    let token = store.begin_init_chat(quiet("local"));
    store.chat_session.as_mut().expect("open").is_streaming = true;
    assert!(!store.complete_init_chat(
        token,
        RestoredChat {
            session: quiet("server"),
            summaries: vec![ChatSessionSummary {
                id: "server".into(),
                title: "S".into(),
            }],
            migration_error: None,
            migration_success: 1,
        },
    ));
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("local"));
    assert!(store.session_history_loading);
    assert_eq!(store.session_migration_success, 0);

    store.chat_session.as_mut().expect("open").is_streaming = false;
    assert!(store.begin_select_session());
    assert_ne!(store.chat_init_token, token);
    assert!(!store.complete_init_chat(
        token,
        RestoredChat {
            session: quiet("server"),
            summaries: Vec::new(),
            migration_error: None,
            migration_success: 4,
        },
    ));
    store.complete_select_session(quiet("picked"));
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("picked"));
    assert!(!store.session_history_loading);
}

#[test]
fn audit_delete_keeps_a_different_open_session_and_persist_uses_the_given_error() {
    let mut store = AiStore::new();
    let _ = store.begin_init_chat(quiet("keep"));
    store.chat_sessions = vec![
        ChatSessionSummary {
            id: "keep".into(),
            title: "K".into(),
        },
        ChatSessionSummary {
            id: "gone".into(),
            title: "G".into(),
        },
    ];
    assert!(store.begin_delete_session());
    store.complete_delete_session("gone", false, None, None);
    assert_eq!(store.chat_session.as_ref().map(|s| s.id.as_str()), Some("keep"));
    assert_eq!(store.chat_sessions.len(), 1);
    assert!(store.session_history_error.is_none());

    store.note_persist_result(false, Some("disk full"));
    assert_eq!(store.session_migration_error.as_deref(), Some("disk full"));
    assert_ne!(store.session_migration_error.as_deref(), Some(PERSIST_FAILED));

    let mut asking = ChatMessage {
        id: "a".into(),
        role: "assistant".into(),
        content: String::new(),
        tool_call_id: None,
        tool_calls: vec![ToolCall {
            id: "q".into(),
            name: "ask_questions".into(),
        }],
        status: None,
        created_at: "t".into(),
    };
    asking.tool_calls.push(ToolCall {
        id: "other".into(),
        name: "list_tables".into(),
    });
    store.chat_session.as_mut().expect("open").messages = vec![asking];
    let sent = store
        .begin_send_chat("42", "mid", "rid", "now", None, Some(""), None, None)
        .expect("send");
    assert_eq!(sent.loaded_skill_ids, Vec::<String>::new());
    let last = store
        .chat_session
        .as_ref()
        .and_then(|s| s.messages.last())
        .expect("message");
    assert_eq!(last.role, "tool");
    assert_eq!(last.tool_call_id.as_deref(), Some("q"));
    assert!(store.chat_session.as_ref().expect("open").agent_mode.is_none());
}
