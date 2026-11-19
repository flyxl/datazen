//! T087-S4: the rest of `stores/aiStore.ts`.
//!
//! Workflow chat, the workflow-list spinner (show after 100ms, stay up at
//! least 200ms), execute / schema doc / connection diagnosis / query
//! analysis, MCP connect bookkeeping, and the three `setupEventListeners`
//! guards that are not pure forwards (`ai:chat-injection`,
//! `ai:memory-candidates`, `ai:skill-used`).
//!
//! ## Left with the host
//!
//! * `handleStreamChunk`, `cancelChat`, `handleCancelTimeout` are one-line
//!   forwards to `stores/ai/streaming.ts`. That module is not this task.
//! * `ai:stream-chunk` and `ai:stream-error` only call those forwards.
//! * `normalizeLoadedSkills` (`lib/aiSkills.ts`) drops nameless entries.
//!   [`LoadedSkill`] values stored here are already that shape.
//! * `loadWorkflows` / `executeWorkflow` catch with `String(e)`, not
//!   `e.message`. The host passes that string through. The other catches
//!   in this slice use `e.message`.
//! * `console.debug` / `console.error` are dropped. A failed MCP list keeps
//!   the previous slice and does not set `mcpError`.
//!
//! `window.__datazenAiStore` is a DOM test hook and is not ported.

#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::ai_store::AiStore;
use crate::ai_store_chat::{
    outgoing_message, ChatFollowUp, ChatMessage, ChatSession, LoadedSkill, MemoryCandidateNotice,
};

/// Delay before `loadWorkflows` shows the spinner.
pub(crate) const WORKFLOW_SHOW_AFTER_MS: u64 = 100;
/// Minimum time the spinner stays up once shown.
pub(crate) const WORKFLOW_MIN_VISIBLE_MS: u64 = 200;

pub(crate) const MCP_CONFIG_MISSING: &str = "MCP server config not found: ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkflowItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct McpServerStatus {
    pub server_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct McpClientConfig {
    pub id: String,
    pub enabled: bool,
    /// Host's `JSON.stringify` of the config. Compared as text.
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum McpSaveStep {
    Disconnect(String),
    Connect(String),
}

impl AiStore {
    /// `initWorkflowChat`. The host supplies `crypto.randomUUID()`.
    pub(crate) fn init_workflow_chat(&mut self, id: impl Into<String>) {
        self.workflow_chat = Some(blank_workflow_session(id.into()));
        self.workflow_attribution = None;
    }

    /// `sendWorkflowChatMessage`. `None` when no workflow chat exists.
    /// An empty content still sends — this path does not trim.
    /// Does not clear [`AiStore::memory_candidate_notice`].
    pub(crate) fn begin_send_workflow(
        &mut self,
        content: &str,
        message_id: impl Into<String>,
        request_id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Option<String> {
        if self.workflow_chat.is_none() {
            return None;
        }
        let request_id = request_id.into();
        let message = outgoing_message(
            self.workflow_chat.as_ref().and_then(|session| session.messages.last()),
            content,
            &message_id.into(),
            &created_at.into(),
        );
        if let Some(session) = self.workflow_chat.as_mut() {
            session.messages.push(message);
            session.is_streaming = true;
            session.stream_content.clear();
            session.request_id = Some(request_id.clone());
        }
        self.workflow_attribution = Some("independent".to_owned());
        Some(request_id)
    }

    /// `catch` of `sendWorkflowChatMessage`. No `status: 'failed'` and no persist.
    pub(crate) fn fail_send_workflow(
        &mut self,
        message: &str,
        error_message_id: impl Into<String>,
        created_at: impl Into<String>,
    ) {
        let Some(session) = self.workflow_chat.as_mut() else {
            return;
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
            status: None,
            created_at: created_at.into(),
        });
    }

    /// `clearWorkflowChat`. Also clears the memory-candidate notice.
    pub(crate) fn clear_workflow_chat(&mut self) {
        let Some(session) = self.workflow_chat.as_mut() else {
            return;
        };
        session.messages.clear();
        session.is_streaming = false;
        session.stream_content.clear();
        session.request_id = None;
        self.workflow_attribution = None;
        self.memory_candidate_notice = None;
    }

    /// Arm `loadWorkflows`. The spinner stays off until
    /// [`WORKFLOW_SHOW_AFTER_MS`] have passed.
    pub(crate) fn begin_load_workflows(&mut self, now_ms: u64) {
        self.workflow_load_started = Some(now_ms);
        self.workflow_shown_at = None;
        self.pending_workflows = None;
        self.pending_workflow_error = None;
    }

    /// Host frame tick. A late tick still records the show time as
    /// `started + 100`, which is when React's timer was due.
    pub(crate) fn tick_workflow_load(&mut self, now_ms: u64) {
        let Some(started) = self.workflow_load_started else {
            return;
        };
        if self.workflow_shown_at.is_some() {
            return;
        }
        if now_ms.saturating_sub(started) >= WORKFLOW_SHOW_AFTER_MS {
            self.workflows_loading = true;
            self.workflow_shown_at = Some(started + WORKFLOW_SHOW_AFTER_MS);
        }
    }

    /// Success path. `None` means the list is applied and the spinner is down.
    /// `Some(until)` means the spinner must stay until that timestamp; the
    /// list is not visible yet. A success does **not** clear `workflow_error`.
    pub(crate) fn complete_load_workflows(
        &mut self,
        now_ms: u64,
        workflows: Vec<WorkflowItem>,
    ) -> Option<u64> {
        self.tick_workflow_load(now_ms);
        self.pending_workflows = Some(workflows);
        self.pending_workflow_error = None;
        self.settle_workflow_load(now_ms)
    }

    /// Failure path. `String(e)` is the host's string. The previous list stays
    /// until a later success. Same hold rule as success.
    pub(crate) fn fail_load_workflows(&mut self, now_ms: u64, error: &str) -> Option<u64> {
        self.tick_workflow_load(now_ms);
        self.pending_workflow_error = Some(error.to_owned());
        self.settle_workflow_load(now_ms)
    }

    /// Apply a held result once `now_ms` reaches the returned deadline.
    pub(crate) fn release_workflow_load(&mut self, now_ms: u64) -> bool {
        let Some(shown) = self.workflow_shown_at else {
            return false;
        };
        if now_ms < shown + WORKFLOW_MIN_VISIBLE_MS {
            return false;
        }
        self.apply_workflow_outcome();
        true
    }

    pub(crate) fn begin_execute_workflow(&mut self) {
        self.is_executing_workflow = true;
        self.workflow_execution_result = None;
        self.workflow_error = None;
    }

    pub(crate) fn complete_execute_workflow(&mut self, result: impl Into<String>) {
        self.is_executing_workflow = false;
        self.workflow_execution_result = Some(result.into());
    }

    /// `String(e)`, not `e.message`.
    pub(crate) fn fail_execute_workflow(&mut self, error: &str) {
        self.is_executing_workflow = false;
        self.workflow_error = Some(error.to_owned());
    }

    pub(crate) fn clear_workflow_result(&mut self) {
        self.workflow_execution_result = None;
        self.workflow_error = None;
    }

    pub(crate) fn begin_schema_doc(&mut self) {
        self.is_generating_schema_doc = true;
        self.schema_doc = None;
        self.schema_doc_error = None;
    }

    pub(crate) fn complete_schema_doc(&mut self, doc: impl Into<String>) {
        self.schema_doc = Some(doc.into());
        self.is_generating_schema_doc = false;
    }

    pub(crate) fn fail_schema_doc(&mut self, message: &str) {
        self.is_generating_schema_doc = false;
        self.schema_doc_error = Some(message.to_owned());
    }

    pub(crate) fn clear_schema_doc(&mut self) {
        self.schema_doc = None;
        self.schema_doc_error = None;
        self.is_generating_schema_doc = false;
    }

    pub(crate) fn begin_diagnose_connection(&mut self) {
        self.is_diagnosing_connection = true;
        self.connection_diagnosis = None;
        self.connection_diagnosis_error = None;
    }

    pub(crate) fn complete_diagnose_connection(&mut self, result: impl Into<String>) {
        self.connection_diagnosis = Some(result.into());
        self.is_diagnosing_connection = false;
    }

    pub(crate) fn fail_diagnose_connection(&mut self, message: &str) {
        self.is_diagnosing_connection = false;
        self.connection_diagnosis_error = Some(message.to_owned());
    }

    pub(crate) fn clear_connection_diagnosis(&mut self) {
        self.connection_diagnosis = None;
        self.connection_diagnosis_error = None;
        self.is_diagnosing_connection = false;
    }

    pub(crate) fn begin_analyze_queries(&mut self) {
        self.is_analyzing_queries = true;
        self.query_analysis = None;
        self.query_analysis_error = None;
    }

    pub(crate) fn complete_analyze_queries(&mut self, result: impl Into<String>) {
        self.query_analysis = Some(result.into());
        self.is_analyzing_queries = false;
    }

    pub(crate) fn fail_analyze_queries(&mut self, message: &str) {
        self.is_analyzing_queries = false;
        self.query_analysis_error = Some(message.to_owned());
    }

    pub(crate) fn clear_query_analysis(&mut self) {
        self.query_analysis = None;
        self.query_analysis_error = None;
        self.is_analyzing_queries = false;
    }

    /// `connectMcpServer` before IPC. `found == false` sets `mcpError` and
    /// returns the thrown message. A found config clears that server's error
    /// and does not set `mcpError`.
    pub(crate) fn begin_connect_mcp(&mut self, server_id: &str, found: bool) -> Result<(), String> {
        if !found {
            let msg = format!("{MCP_CONFIG_MISSING}{server_id}");
            self.mcp_error = Some(msg.clone());
            return Err(msg);
        }
        self.mcp_server_errors.remove(server_id);
        self.mcp_connecting = true;
        self.mcp_connecting_server_id = Some(server_id.to_owned());
        self.mcp_error = None;
        Ok(())
    }

    pub(crate) fn finish_connect_mcp(&mut self) {
        self.mcp_connecting = false;
        self.mcp_connecting_server_id = None;
    }

    /// Per-server error only. React does not set `mcpError` on this path.
    pub(crate) fn fail_connect_mcp(&mut self, server_id: &str, message: &str) -> String {
        self.mcp_connecting = false;
        self.mcp_connecting_server_id = None;
        self.mcp_server_errors
            .insert(server_id.to_owned(), message.to_owned());
        message.to_owned()
    }

    pub(crate) fn fail_disconnect_mcp(&mut self, message: &str) {
        self.mcp_error = Some(message.to_owned());
    }

    pub(crate) fn apply_mcp_servers(&mut self, servers: Vec<McpServerStatus>) {
        self.mcp_servers = servers;
    }

    /// Failed list: keep the previous servers. No `mcpError`.
    pub(crate) fn fail_load_mcp_servers(&mut self) {
        let _ = &self.mcp_servers;
    }

    pub(crate) fn apply_mcp_tools(&mut self, tools: Vec<String>) {
        self.mcp_tools = tools;
    }

    pub(crate) fn fail_load_mcp_tools(&mut self) {
        let _ = &self.mcp_tools;
    }

    pub(crate) fn fail_call_mcp(&mut self, message: &str) -> String {
        self.mcp_error = Some(message.to_owned());
        message.to_owned()
    }

    pub(crate) fn clear_mcp_error(&mut self) {
        self.mcp_error = None;
    }

    pub(crate) fn clear_mcp_server_error(&mut self, server_id: &str) {
        self.mcp_server_errors.remove(server_id);
    }

    /// `ai:chat-injection`. Ignored when the request id does not match.
    pub(crate) fn note_injection(&mut self, request_id: &str, snapshot: &str) -> bool {
        let Some(session) = self.chat_session.as_mut() else {
            return false;
        };
        if session.request_id.as_deref() != Some(request_id) {
            return false;
        }
        session.injection = Some(snapshot.to_owned());
        true
    }

    /// `ai:memory-candidates`. A zero count does not clear an existing notice.
    pub(crate) fn note_memory_candidates(&mut self, request_id: &str, count: u32) -> bool {
        let Some(session) = self.chat_session.as_ref() else {
            return false;
        };
        if session.request_id.as_deref() != Some(request_id) {
            return false;
        }
        if count == 0 {
            return false;
        }
        self.memory_candidate_notice = Some(MemoryCandidateNotice {
            request_id: request_id.to_owned(),
            count,
        });
        true
    }

    /// `ai:skill-used`. Blank `name` is ignored. The matching identity
    /// (`id`, or `name` when `id` is empty) is replaced and the new skill
    /// is appended. Returns [`ChatFollowUp::Persist`] when applied.
    pub(crate) fn note_skill_used(
        &mut self,
        request_id: &str,
        id: Option<&str>,
        name: &str,
        source: Option<&str>,
        loaded_at: &str,
    ) -> ChatFollowUp {
        if name.is_empty() {
            return ChatFollowUp::None;
        }
        let Some(session) = self.chat_session.as_mut() else {
            return ChatFollowUp::None;
        };
        if session.request_id.as_deref() != Some(request_id) {
            return ChatFollowUp::None;
        }
        let identity = match id {
            Some(value) if !value.is_empty() => value,
            _ => name,
        };
        session
            .loaded_skills
            .retain(|skill| skill_identity(skill) != identity);
        let source = source.and_then(|value| {
            if value.is_empty() {
                None
            } else {
                Some(value.to_owned())
            }
        });
        session.loaded_skills.push(LoadedSkill {
            id: id.unwrap_or("").to_owned(),
            name: name.to_owned(),
            source,
            loaded_at: loaded_at.to_owned(),
        });
        ChatFollowUp::Persist
    }

    fn settle_workflow_load(&mut self, now_ms: u64) -> Option<u64> {
        if let Some(shown) = self.workflow_shown_at {
            let until = shown + WORKFLOW_MIN_VISIBLE_MS;
            if now_ms < until {
                return Some(until);
            }
        }
        self.apply_workflow_outcome();
        None
    }

    fn apply_workflow_outcome(&mut self) {
        if let Some(list) = self.pending_workflows.take() {
            self.workflows = list;
        }
        if let Some(error) = self.pending_workflow_error.take() {
            self.workflow_error = Some(error);
        }
        self.workflows_loading = false;
        self.workflow_load_started = None;
        self.workflow_shown_at = None;
    }
}

/// `saveMcpClientServers` after the settings write: disconnect ids that
/// left the list, then disconnect+maybe-connect configs whose fingerprint
/// changed. Newly added configs are not connected here.
pub(crate) fn plan_mcp_save(
    connected_ids: &[String],
    prev: &[McpClientConfig],
    next: &[McpClientConfig],
) -> Vec<McpSaveStep> {
    let next_ids: BTreeMap<&str, ()> = next.iter().map(|config| (config.id.as_str(), ())).collect();
    let mut steps = Vec::new();
    for id in connected_ids {
        if !next_ids.contains_key(id.as_str()) {
            steps.push(McpSaveStep::Disconnect(id.clone()));
        }
    }
    for config in next {
        if !connected_ids.iter().any(|id| id == &config.id) {
            continue;
        }
        let changed = match prev.iter().find(|item| item.id == config.id) {
            None => true,
            Some(previous) => previous.fingerprint != config.fingerprint,
        };
        if !changed {
            continue;
        }
        steps.push(McpSaveStep::Disconnect(config.id.clone()));
        if config.enabled {
            steps.push(McpSaveStep::Connect(config.id.clone()));
        }
    }
    steps
}

fn blank_workflow_session(id: String) -> ChatSession {
    ChatSession {
        id,
        session_key: "workflow".to_owned(),
        connection_id: None,
        database: None,
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

fn skill_identity(skill: &LoadedSkill) -> &str {
    if !skill.id.is_empty() {
        skill.id.as_str()
    } else {
        skill.name.as_str()
    }
}
