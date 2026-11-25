//! T087-S3: chat session actions from `stores/aiStore.ts`.
//!
//! Session persistence (`stores/ai/sessions.ts`) and the stream registry
//! (`stores/ai/streaming.ts`) stay outside this slice. The host builds the
//! local/restored/created session objects and feeds them back. This module
//! owns the token, the clobber guard, the streaming delete refusal, and
//! the outgoing-message role (`ask_questions` → tool, otherwise user).
//!
//! `console.debug` is dropped. `persistLiveChatSession` is reported as
//! [`ChatFollowUp::Persist`]; the host writes, then calls
//! [`AiStore::note_persist_result`]. A missing error string becomes
//! `AI session backend write failed`, matching the React fallback.

#![allow(dead_code)]

use crate::ai_store::AiStore;

pub const DELETE_WHILE_STREAMING: &str = "Cannot delete a conversation while it is streaming.";
pub const PERSIST_FAILED: &str = "AI session backend write failed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub status: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedSkill {
    pub id: String,
    pub name: String,
    pub source: Option<String>,
    pub loaded_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSession {
    pub id: String,
    pub session_key: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub is_streaming: bool,
    pub stream_content: String,
    pub request_id: Option<String>,
    pub loaded_skills: Vec<LoadedSkill>,
    pub template_id: Option<String>,
    pub agent_mode: Option<String>,
    pub memory_mode: Option<String>,
    pub memory_scope: Option<String>,
    /// Set by `ai:chat-injection` for the live request. `sendChatMessage`
    /// clears it; `clearChat` keeps it (the spread does not touch it).
    pub injection: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSessionSummary {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCandidateNotice {
    pub request_id: String,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoredChat {
    pub session: ChatSession,
    pub summaries: Vec<ChatSessionSummary>,
    pub migration_error: Option<String>,
    pub migration_success: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatFollowUp {
    /// Host runs `persistChatSession` on the current session.
    Persist,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendChat {
    pub request_id: String,
    pub loaded_skill_ids: Vec<String>,
    pub follow_up: ChatFollowUp,
}

impl AiStore {
    /// `initChatSession` up to the restore. Returns the token the host must
    /// pass back to [`AiStore::complete_init_chat`].
    pub fn begin_init_chat(&mut self, initial: ChatSession) -> u64 {
        self.chat_init_token += 1;
        let baseline = initial.messages.len();
        self.init_message_baseline = Some(baseline);
        self.chat_session = Some(initial);
        self.chat_sessions.clear();
        self.session_migration_error = None;
        self.session_migration_success = 0;
        self.session_history_error = None;
        self.session_history_loading = true;
        self.memory_candidate_notice = None;
        self.chat_init_token
    }

    /// Apply a restore only when this init is still the latest one and the
    /// user has not started a stream or changed the message count.
    pub fn complete_init_chat(&mut self, token: u64, restored: RestoredChat) -> bool {
        if token != self.chat_init_token {
            return false;
        }
        let Some(current) = self.chat_session.as_ref() else {
            return false;
        };
        let Some(baseline) = self.init_message_baseline else {
            return false;
        };
        if current.is_streaming || current.messages.len() != baseline {
            return false;
        }
        self.chat_session = Some(restored.session);
        self.chat_sessions = restored.summaries;
        self.session_migration_error = restored.migration_error;
        self.session_migration_success = restored.migration_success;
        self.session_history_loading = false;
        true
    }

    /// `refreshChatSessions`. `false` when there is no session.
    pub fn begin_refresh_sessions(&mut self) -> bool {
        if self.chat_session.is_none() {
            return false;
        }
        self.session_history_loading = true;
        self.session_history_error = None;
        true
    }

    pub fn complete_refresh_sessions(&mut self, summaries: Vec<ChatSessionSummary>) {
        self.chat_sessions = summaries;
        self.session_history_loading = false;
    }

    pub fn fail_refresh_sessions(&mut self, message: &str) {
        self.session_history_loading = false;
        self.session_history_error = Some(message.to_owned());
    }

    /// `selectChatSession`. `false` with no state change when missing or streaming.
    pub fn begin_select_session(&mut self) -> bool {
        match self.chat_session.as_ref() {
            Some(session) if !session.is_streaming => {}
            _ => return false,
        }
        self.chat_init_token += 1;
        self.session_history_loading = true;
        self.session_history_error = None;
        true
    }

    pub fn complete_select_session(&mut self, session: ChatSession) -> ChatFollowUp {
        self.chat_session = Some(session);
        self.session_history_loading = false;
        ChatFollowUp::Persist
    }

    pub fn fail_select_session(&mut self, message: &str) {
        self.session_history_loading = false;
        self.session_history_error = Some(message.to_owned());
    }

    /// `newChatSession`. The host supplies the new id; scope and key are copied.
    pub fn new_chat_session(&mut self, new_id: impl Into<String>) -> bool {
        let Some(current) = self.chat_session.as_ref() else {
            return false;
        };
        if current.is_streaming {
            return false;
        }
        let session = ChatSession {
            id: new_id.into(),
            session_key: current.session_key.clone(),
            connection_id: current.connection_id.clone(),
            database: current.database.clone(),
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
        };
        self.chat_init_token += 1;
        self.chat_session = Some(session);
        self.memory_candidate_notice = None;
        true
    }

    /// `deleteChatSession` before the persist call. Streaming refuses with
    /// the React error string and does not set loading.
    pub fn begin_delete_session(&mut self) -> bool {
        let Some(current) = self.chat_session.as_ref() else {
            return false;
        };
        if current.is_streaming {
            self.session_history_error = Some(DELETE_WHILE_STREAMING.to_owned());
            return false;
        }
        self.session_history_loading = true;
        self.session_history_error = None;
        true
    }

    /// Apply the delete. `error` is the persist result's error (None when ok).
    /// When the deleted id is the open session, the host's `replacement` is
    /// installed (React `createChatSession` of the same scope and key).
    pub fn complete_delete_session(
        &mut self,
        deleted_id: &str,
        ok: bool,
        error: Option<&str>,
        replacement: Option<ChatSession>,
    ) {
        self.chat_sessions.retain(|session| session.id != deleted_id);
        let replacing = self
            .chat_session
            .as_ref()
            .is_some_and(|session| session.id == deleted_id);
        if replacing {
            if let Some(session) = replacement {
                self.chat_session = Some(session);
            }
        }
        self.session_history_loading = false;
        self.session_history_error = if ok { None } else { error.map(str::to_owned) };
    }

    /// `exportChatSession` failure. Success is a host download and does not
    /// change the store. `false` when there is no session.
    pub fn fail_export_session(&mut self, message: &str) -> bool {
        if self.chat_session.is_none() {
            return false;
        }
        self.session_history_error = Some(message.to_owned());
        true
    }

    /// `sendChatMessage`. `None` when no session is open.
    pub fn begin_send_chat(
        &mut self,
        content: &str,
        message_id: impl Into<String>,
        request_id: impl Into<String>,
        created_at: impl Into<String>,
        template_id: Option<&str>,
        agent_mode: Option<&str>,
        memory_mode: Option<&str>,
        memory_scope: Option<&str>,
    ) -> Option<SendChat> {
        if self.chat_session.is_none() {
            return None;
        }
        let request_id = request_id.into();
        let created_at = created_at.into();
        let message_id = message_id.into();
        let loaded_skill_ids = self
            .chat_session
            .as_ref()
            .map(|session| skill_ids(&session.loaded_skills))
            .unwrap_or_default();
        let message = outgoing_message(
            self.chat_session.as_ref().and_then(|session| session.messages.last()),
            content,
            &message_id,
            &created_at,
        );
        if let Some(session) = self.chat_session.as_mut() {
            session.messages.push(message);
            session.is_streaming = true;
            session.stream_content.clear();
            session.request_id = Some(request_id.clone());
            set_meta(&mut session.template_id, template_id);
            set_meta(&mut session.agent_mode, agent_mode);
            set_meta(&mut session.memory_mode, memory_mode);
            set_meta(&mut session.memory_scope, memory_scope);
            session.injection = None;
        }
        self.memory_candidate_notice = None;
        Some(SendChat {
            request_id,
            loaded_skill_ids,
            follow_up: ChatFollowUp::Persist,
        })
    }

    /// The `catch` of `sendChatMessage`: stop the stream and append
    /// `Error: {message}` as a failed assistant message.
    pub fn fail_send_chat(
        &mut self,
        message: &str,
        error_message_id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> ChatFollowUp {
        let Some(session) = self.chat_session.as_mut() else {
            return ChatFollowUp::None;
        };
        session.is_streaming = false;
        session.stream_content.clear();
        session.request_id = None;
        session.messages.push(ChatMessage {
            id: error_message_id.into(),
            role: "assistant".to_owned(),
            content: format!("Error: {message}"),
            tool_call_id: None,
            tool_calls: Vec::new(),
            status: Some("failed".to_owned()),
            created_at: created_at.into(),
        });
        ChatFollowUp::Persist
    }

    /// `clearChat`. `None` when there is no session.
    pub fn clear_chat(&mut self) -> ChatFollowUp {
        let Some(session) = self.chat_session.as_mut() else {
            return ChatFollowUp::None;
        };
        session.messages.clear();
        session.is_streaming = false;
        session.stream_content.clear();
        session.request_id = None;
        session.loaded_skills.clear();
        self.memory_candidate_notice = None;
        ChatFollowUp::Persist
    }

    pub fn dismiss_memory_candidate_notice(&mut self) {
        self.memory_candidate_notice = None;
    }

    /// Persist callback. `None` error on failure uses [`PERSIST_FAILED`].
    pub fn note_persist_result(&mut self, ok: bool, error: Option<&str>) {
        if ok {
            return;
        }
        self.session_migration_error = Some(error.unwrap_or(PERSIST_FAILED).to_owned());
    }
}

fn set_meta(slot: &mut Option<String>, value: Option<&str>) {
    if let Some(value) = value {
        if !value.is_empty() {
            *slot = Some(value.to_owned());
        }
    }
}

fn skill_ids(skills: &[LoadedSkill]) -> Vec<String> {
    skills
        .iter()
        .filter_map(|skill| {
            if !skill.id.is_empty() {
                Some(skill.id.clone())
            } else if !skill.name.is_empty() {
                Some(skill.name.clone())
            } else {
                None
            }
        })
        .collect()
}

/// The message `sendChatMessage` appends. A trailing `ask_questions` tool
/// call makes it a tool result; every other shape is a user message.
pub fn outgoing_message(
    last: Option<&ChatMessage>,
    content: &str,
    id: &str,
    created_at: &str,
) -> ChatMessage {
    let ask_id = last.and_then(|message| {
        message
            .tool_calls
            .iter()
            .find(|call| call.name == "ask_questions")
            .map(|call| call.id.clone())
    });
    if let Some(tool_call_id) = ask_id {
        ChatMessage {
            id: id.to_owned(),
            role: "tool".to_owned(),
            content: content.to_owned(),
            tool_call_id: Some(tool_call_id),
            tool_calls: Vec::new(),
            status: None,
            created_at: created_at.to_owned(),
        }
    } else {
        ChatMessage {
            id: id.to_owned(),
            role: "user".to_owned(),
            content: content.to_owned(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            status: None,
            created_at: created_at.to_owned(),
        }
    }
}
