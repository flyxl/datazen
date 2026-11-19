//! Panel shell + session wiring state — T009-S6, the sixth slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 93–404: the panel props, the `aiStore` subscription surface, the
//! auto-context snapshot sync, the AI draft bridge, send / stop /
//! cancel, and the unconfigured early return. The render half (the
//! header row, the tab switch, the message list, the input area) lives
//! in [`super::ai_chat_panel_shell::AiChatPanel`], which assembles the
//! S3 header, S4 message list and S5 input area from this state.
//!
//! # State ownership
//!
//! Like the S3–S5 slices, the panel is a pure render + event router:
//! the caller owns one [`AiChatPanelState`] (in `AppState` once the
//! panel lands in the `Section` dispatch in S7; in the probe state in
//! tests) and hands `&state` to the builder each frame; presses call
//! back into the caller, which drives this state machine.
//!
//! # View-model boundary (aiStore slice T088 not done)
//!
//! `aiStore` has no Rune slice yet, so this state is a view-model
//! placeholder, not a store binding:
//!
//! * the session, the messages, the streaming fields, the notices and
//!   the profiles / templates / sharing flags live in the three owned
//!   sub-states ([`AiChatHeaderState`], [`AiChatMessageListState`],
//!   [`AiChatInputAreaState`]); the panel slice wires them to the real
//!   store once T088 lands;
//! * `sendMessage` / `cancelChat` / `initChat` / `clearChat` /
//!   `loadConfig` / `aiCommands.memoryCreate` / `openDocsWindow` /
//!   `openSettingsWindow` are host / store actions, so the panel
//!   records what it *would* call — [`AiSentMessage`] (the
//!   `sendMessage` payload), [`AiMemoryCreateRequest`] (the
//!   `memoryCreate` payload), `cancelled_request_ids` (the `cancelChat`
//!   receipts) and `consumed_draft_ids` (the `onDraftConsumed` acks) —
//!   in outbox vectors the host (S7) reads, instead of pretending a
//!   round-trip happened;
//! * `useAiEditorGrant` is not wired: `sharing` / `can_share` /
//!   `share_error` are seeded on the header state, and `claimForSend`
//!   always yields `None` (`target_claim` on the sent record), so a
//!   send never claims an editor session until the grant hook lands;
//! * `AiSessionHistory` (T195) and `WorkflowPanel` (T334) are separate
//!   tasks: the shell renders keyed mount shells for them and nothing
//!   more — see the shell's module docs;
//! * assistant message content is still S2's plain-text placeholder;
//! * the 4000ms auto-clear of the remember notice (`window.setTimeout`
//!   in `handleRemember`) has no VM clock behind it: the notice is set
//!   and stays until the host dismisses it (`clear_chat` never
//!   touches it — the store `clearChat` only clears the store-owned
//!   candidate notice).
//!
//! # React effects, as explicit methods
//!
//! React's `useEffect`s become methods the host calls when the matching
//! input changes — there is no effect runner in immediate mode:
//!
//! | React effect | Panel method | When the host calls it |
//! |---|---|---|
//! | session-key effect (197–201) | [`AiChatPanelState::ensure_session`] | after `connectionId` / `dbSessionId` / `database` change |
//! | snapshot sync (130–157) | [`AiChatPanelState::sync_auto_context`] | when the host `autoContext` prop changes |
//! | template reset (188–190) | inside `set_connection_id` / `set_database_type` | on the prop write itself |
//! | draft bridge (221–249) | [`AiChatPanelState::handle_draft_request`] | when a `draftRequest` arrives |
//! | unmount cancel (371–379) | [`AiChatPanelState::cancel_in_flight`] | on unmount / pane / connection switch |
//! | `loadConfig` (166–168) | — | the host seeds `is_configured` directly |
//!
//! # Panel-slice decisions (S4's open points, decided here)
//!
//! * **`memoryUsed` `[]` vs absent — kept as `Vec`, no `Option<Vec>`
//!   migration.** React's message rule (`AiChatPanel.tsx:696–702`)
//!   uses `||`, so an explicit `[]` is truthy and *blocks* the
//!   session-memory inheritance, while `undefined` on the last
//!   assistant message inherits it. Rune's
//!   [`AiChatMessageListState::effective_message`] treats an empty vec
//!   as absent and inherits. The divergence only matters when the store
//!   sends an explicit empty `memoryUsed` on the last assistant message
//!   while session memory exists — no producer does that today (the
//!   store seeds `memoryUsed: null`, `stores/ai/sessions.ts:343`, and a
//!   message carries memory only when the backend reports some), and
//!   the same class is already pinned by S4's audit D2 for the stream
//!   fallback. Migrating the field to `Option<Vec<…>>` would ripple
//!   through `ChatBubble`, the list state and the S2/S4 test and audit
//!   files that pin the current behaviour, so it stays a documented
//!   gap for the T088 wiring to revisit if a producer ever
//!   distinguishes the two.
//! * **Streaming without a session is never seeded.**
//!   [`AiChatPanelState::set_streaming`] ignores `true` while the list
//!   has no session (`ensure_session` has not run), so a VM-seeded
//!   stream cannot render a streaming bubble or thinking row over an
//!   empty, session-less panel — React's `chatSession?.isStreaming`
//!   is `undefined` before `initChat` for the same reason.
//! * **S3's D1 (empty-string error values) is fixed at this seam.**
//!   [`AiChatPanelState::set_session_migration_error`] and
//!   [`AiChatPanelState::set_share_error`] normalise `Some("")` to
//!   `None` before forwarding to the header state, matching React's
//!   `&&` truthiness. The S3 audit tests pin the header's old
//!   behaviour and are untouched.
//!
//! # Deliberate boundaries
//!
//! * S5's D1/D2/D3/D5 (whitespace-only template name, whitespace-only
//!   snapshot field, no `opacity-50` dim while streaming, egress lines
//!   at full amber-200) are **not** fixed here: each fix flips a
//!   tester-pinned assertion in S5's audit file and touches S1/S5
//!   production code, so they stay deferred to a later slice with a
//!   tester re-audit rather than being snuck into the panel wiring.
//! * S2's D1 (`open_memory_citation("")` opens the list while React
//!   returns early) is unchanged: the placeholder content has no
//!   links, so the path cannot be triggered from the UI today.
//! * The draft bridge's textarea focus (`inputRef.current?.focus()`
//!   after render) has no Rune hook from a state method: prefill and
//!   conflict resolution set `focus_input_requested`, and the host
//!   consumes it with [`AiChatPanelState::take_focus_input_request`]
//!   and focuses `ai-input-text` (the S5 field key).

// The panel view (S7) is the production consumer. The dead-code lint
// stays silenced here rather than in a crate-wide allow: the
// T088/T169-owned surface (store seeders, on_run_code / on_new_query
// pass-throughs) still has no production caller.
#![allow(dead_code)]

use super::ai_chat_header::{AiChatHeaderState, AiChatTab};
use super::ai_chat_input_area_state::{AiChatInputAreaState, AiPendingDraft, ConflictResolution};
use super::ai_chat_message_list_state::AiChatMessageListState;
use super::ai_chat_panel::{has_snapshot_content, AiChatContextSnapshot};
use super::ai_chat_panel_types::{
    AiMemoryCreateRequest, AiMemoryMode, AiMemoryScope, AiSentMessage,
};
use super::chat_bubble::{AiChatMessage, AiChatRole};
use crate::i18n::t;

/// The mutable state behind an [`AiChatPanel`]: the view-model
/// placeholder for the `aiStore` slice (T088, not done), owning the S3
/// header, S4 message-list and S5 input-area states plus the
/// panel-level props and outboxes. Defaults mirror React's initials:
/// chat tab, ask mode, no template, nothing shared, no session yet
/// (`initChat` not run), AI unconfigured until the host seeds it, and
/// the panel's explicit `'auto'` + `'user'` memory default (see the
/// props comment at `AiChatPanel.tsx:108–112`).
#[derive(Clone, Debug)]
pub(crate) struct AiChatPanelState {
    db_session_id: String,
    database: Option<String>,
    database_type: Option<String>,
    sql_dialect: Option<String>,
    connection_id: Option<String>,
    editor_session_id: Option<String>,
    memory_mode: Option<AiMemoryMode>,
    memory_scope: AiMemoryScope,
    /// Stand-in for `settingsConfig.memory.enabled`: `true` until the
    /// settings store lands, so the effective mode defaults to `'auto'`
    /// exactly as the panel's comment promises.
    memory_enabled: bool,
    kv_key_name: Option<String>,
    kv_block: Option<String>,
    is_configured: bool,
    session_key: Option<String>,
    request_id: Option<String>,
    previous_auto_context: Option<AiChatContextSnapshot>,
    last_processed_draft: Option<String>,
    focus_input_requested: bool,
    header: AiChatHeaderState,
    list: AiChatMessageListState,
    area: AiChatInputAreaState,
    sent_messages: Vec<AiSentMessage>,
    consumed_draft_ids: Vec<String>,
    cancelled_request_ids: Vec<String>,
    memory_create_requests: Vec<AiMemoryCreateRequest>,
    clear_chat_presses: u32,
}

impl AiChatPanelState {
    /// `dbSessionId` is the panel's only required prop. The list starts
    /// session-less — `chatSession` is `undefined` before `initChat`,
    /// so no welcome line renders until [`AiChatPanelState::ensure_session`]
    /// first runs — even though the list state's own default is
    /// `has_session: true` for its standalone tests.
    pub(crate) fn new(db_session_id: impl Into<String>) -> Self {
        let mut state = Self {
            db_session_id: db_session_id.into(),
            database: None,
            database_type: None,
            sql_dialect: None,
            connection_id: None,
            editor_session_id: None,
            memory_mode: None,
            memory_scope: AiMemoryScope::default(),
            memory_enabled: true,
            kv_key_name: None,
            kv_block: None,
            is_configured: false,
            session_key: None,
            request_id: None,
            previous_auto_context: None,
            last_processed_draft: None,
            focus_input_requested: false,
            header: AiChatHeaderState::default(),
            list: AiChatMessageListState::default(),
            area: AiChatInputAreaState::default(),
            sent_messages: Vec::new(),
            consumed_draft_ids: Vec::new(),
            cancelled_request_ids: Vec::new(),
            memory_create_requests: Vec::new(),
            clear_chat_presses: 0,
        };
        state.list.set_has_session(false);
        state
    }

    pub(crate) fn db_session_id(&self) -> &str {
        &self.db_session_id
    }

    pub(crate) fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    pub(crate) fn database_type(&self) -> Option<&str> {
        self.database_type.as_deref()
    }

    pub(crate) fn sql_dialect(&self) -> Option<&str> {
        self.sql_dialect.as_deref()
    }

    pub(crate) fn connection_id(&self) -> Option<&str> {
        self.connection_id.as_deref()
    }

    pub(crate) fn editor_session_id(&self) -> Option<&str> {
        self.editor_session_id.as_deref()
    }

    pub(crate) fn memory_scope(&self) -> AiMemoryScope {
        self.memory_scope
    }

    pub(crate) fn memory_enabled(&self) -> bool {
        self.memory_enabled
    }

    pub(crate) fn kv_key_name(&self) -> Option<&str> {
        self.kv_key_name.as_deref()
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.is_configured
    }

    /// The live session key, `None` until [`AiChatPanelState::ensure_session`]
    /// first runs (`chatSession` is `undefined` before `initChat`).
    pub(crate) fn session_key(&self) -> Option<&str> {
        self.session_key.as_deref()
    }

    pub(crate) fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub(crate) fn header(&self) -> &AiChatHeaderState {
        &self.header
    }

    pub(crate) fn list(&self) -> &AiChatMessageListState {
        &self.list
    }

    pub(crate) fn area(&self) -> &AiChatInputAreaState {
        &self.area
    }

    pub(crate) fn header_mut(&mut self) -> &mut AiChatHeaderState {
        &mut self.header
    }

    pub(crate) fn list_mut(&mut self) -> &mut AiChatMessageListState {
        &mut self.list
    }

    pub(crate) fn area_mut(&mut self) -> &mut AiChatInputAreaState {
        &mut self.area
    }

    pub(crate) fn sent_messages(&self) -> &[AiSentMessage] {
        &self.sent_messages
    }

    pub(crate) fn consumed_draft_ids(&self) -> &[String] {
        &self.consumed_draft_ids
    }

    pub(crate) fn cancelled_request_ids(&self) -> &[String] {
        &self.cancelled_request_ids
    }

    pub(crate) fn memory_create_requests(&self) -> &[AiMemoryCreateRequest] {
        &self.memory_create_requests
    }

    pub(crate) fn clear_chat_presses(&self) -> u32 {
        self.clear_chat_presses
    }

    /// The effective memory mode: the prop wins, else
    /// `memoryModeFor(settings.memory)` — `'auto'` while the memory
    /// switch is on, `'off'` while it is off.
    pub(crate) fn effective_memory_mode(&self) -> AiMemoryMode {
        match self.memory_mode {
            Some(mode) => mode,
            None => {
                if self.memory_enabled {
                    AiMemoryMode::Auto
                } else {
                    AiMemoryMode::Off
                }
            }
        }
    }

    // -- Props: the host writes these; the template reset lives on the
    // writes React's `useEffect([connectionId, databaseType])` watches. --

    pub(crate) fn set_database(&mut self, database: Option<String>) {
        self.database = database;
    }

    /// React's template-reset effect (`AiChatPanel.tsx:188–190`): a
    /// connection or database-type change returns to automatic default
    /// selection instead of carrying a stale pick. A database change
    /// alone does not reset — it is not in the effect's deps.
    pub(crate) fn set_connection_id(&mut self, connection_id: Option<String>) {
        if self.connection_id != connection_id {
            self.connection_id = connection_id;
            self.header.choose_template("");
        } else {
            self.connection_id = connection_id;
        }
    }

    pub(crate) fn set_database_type(&mut self, database_type: Option<String>) {
        if self.database_type != database_type {
            self.database_type = database_type;
            self.header.choose_template("");
        } else {
            self.database_type = database_type;
        }
    }

    pub(crate) fn set_sql_dialect(&mut self, sql_dialect: Option<String>) {
        self.sql_dialect = sql_dialect;
    }

    pub(crate) fn set_editor_session_id(&mut self, editor_session_id: Option<String>) {
        self.editor_session_id = editor_session_id;
    }

    pub(crate) fn set_memory_mode(&mut self, memory_mode: Option<AiMemoryMode>) {
        self.memory_mode = memory_mode;
    }

    pub(crate) fn set_memory_scope(&mut self, memory_scope: AiMemoryScope) {
        self.memory_scope = memory_scope;
    }

    pub(crate) fn set_memory_enabled(&mut self, memory_enabled: bool) {
        self.memory_enabled = memory_enabled;
    }

    /// The host-owned KV context (`kvContext` prop): the key name the
    /// chip shows and the block `composeKvAiMessage` appends. The area
    /// only carries the key name; the block lives here with the send.
    pub(crate) fn set_kv_context(&mut self, key_name: Option<String>, block: Option<String>) {
        self.kv_key_name = key_name.clone();
        self.kv_block = block;
        self.area.set_kv_key_name(key_name);
    }

    pub(crate) fn set_is_configured(&mut self, is_configured: bool) {
        self.is_configured = is_configured;
    }

    /// The in-flight run's request id, for `handleStop` / unmount
    /// cancel. Seeded by the host; the VM never invents one.
    pub(crate) fn set_request_id(&mut self, request_id: Option<String>) {
        self.request_id = request_id;
    }

    pub(crate) fn set_session_history_open(&mut self, open: bool) {
        self.header.set_session_history_open(open);
    }

    // -- Session wiring --

    /// React's `computeSessionKey` (`stores/ai/sessions.ts:67–73`):
    /// `{connectionId || dbSessionId || 'default'}::{database || ''}`.
    /// JS `||` is first-non-empty, so an empty connection id falls
    /// through to the db session id, and an empty db session id to
    /// `'default'`.
    pub(crate) fn desired_session_key(&self) -> String {
        let owner = first_non_empty([
            self.connection_id.as_deref(),
            Some(self.db_session_id.as_str()),
        ])
        .unwrap_or("default");
        let database = self.database.as_deref().unwrap_or("");
        format!("{owner}::{database}")
    }

    /// React's session-key effect (`AiChatPanel.tsx:197–201`): when the
    /// desired key differs from the live session's, `initChat` starts a
    /// fresh session. A reconnect changes `dbSessionId` but must not
    /// switch the conversation when the persisted `connectionId` is
    /// the key's owner; a real connection change re-keys it. Returns
    /// whether a fresh session started.
    pub(crate) fn ensure_session(&mut self) -> bool {
        let desired = self.desired_session_key();
        if self.session_key.as_deref() == Some(desired.as_str()) {
            return false;
        }
        self.session_key = Some(desired);
        self.list.set_has_session(true);
        self.list.set_messages(Vec::new());
        self.list.set_streaming(false);
        self.list.set_stream_content(String::new());
        self.list.set_stream_reasoning(String::new());
        self.list.set_stream_mcp_tool_name(None);
        self.list.set_stream_memory_used(Vec::new());
        self.list.set_session_memory_used(Vec::new());
        self.list.set_memory_candidate_count(None);
        self.set_session_migration_error(None);
        self.set_session_migration_success(0);
        self.area.set_streaming(false);
        self.area.set_loaded_skills(Vec::new());
        self.area.set_egress(
            None,
            self.area.egress_database().map(str::to_string),
            self.area.egress_relaxed(),
            self.area.egress_has_context_files(),
        );
        self.header.set_has_messages(false);
        self.request_id = None;
        true
    }

    // -- Snapshot sync --

    /// React's snapshot-sync effect (`AiChatPanel.tsx:130–157`): keep
    /// the chips in sync with the host snapshot, but never resurrect a
    /// field the user explicitly removed unless its value changes.
    /// Each field is compared with the previous host snapshot; only a
    /// changed field is written (set when the new value is truthy,
    /// deleted otherwise — an empty string deletes, exactly as JS
    /// `if (autoContext?.editorSql)` does). The snapshot itself goes
    /// `null` when nothing with content is left.
    pub(crate) fn sync_auto_context(&mut self, next: Option<&AiChatContextSnapshot>) {
        let previous = self.previous_auto_context.clone();
        self.previous_auto_context = next.cloned();
        let mut current = self.area.snapshot().cloned().unwrap_or_default();
        let mut changed = false;
        if next.and_then(|s| s.editor_sql.as_deref())
            != previous.as_ref().and_then(|s| s.editor_sql.as_deref())
        {
            changed = true;
            match next.and_then(|s| s.editor_sql.as_deref()) {
                Some(value) if !value.is_empty() => {
                    current.editor_sql = Some(value.to_string());
                }
                _ => current.editor_sql = None,
            }
        }
        if next.and_then(|s| s.selection.as_deref())
            != previous.as_ref().and_then(|s| s.selection.as_deref())
        {
            changed = true;
            match next.and_then(|s| s.selection.as_deref()) {
                Some(value) if !value.is_empty() => {
                    current.selection = Some(value.to_string());
                }
                _ => current.selection = None,
            }
        }
        if next.and_then(|s| s.last_error.as_deref())
            != previous.as_ref().and_then(|s| s.last_error.as_deref())
        {
            changed = true;
            match next.and_then(|s| s.last_error.as_deref()) {
                Some(value) if !value.is_empty() => {
                    current.last_error = Some(value.to_string());
                }
                _ => current.last_error = None,
            }
        }
        if next.and_then(|s| s.result_preview.as_ref())
            != previous.as_ref().and_then(|s| s.result_preview.as_ref())
        {
            changed = true;
            current.result_preview = next.and_then(|s| s.result_preview.clone());
        }
        if changed {
            let snapshot = if has_snapshot_content(Some(&current)) {
                Some(current)
            } else {
                None
            };
            self.area.set_snapshot(snapshot);
        }
    }

    // -- Draft bridge (S3-B2) --

    /// React's draft-request effect (`AiChatPanel.tsx:221–249`).
    /// Unconfigured or already-processed requests are ignored. A
    /// non-empty input parks the draft as a pending conflict and acks
    /// nothing. An empty input prefills the draft, switches to the chat
    /// tab and marks it processed; the `onDraftConsumed` ack is
    /// recorded only when not streaming — a streaming prefill defers
    /// the ack, and the processed mark means the effect's early return
    /// skips it on later runs, exactly as React's does.
    pub(crate) fn handle_draft_request(&mut self, draft: Option<&AiPendingDraft>) {
        let Some(draft) = draft else {
            return;
        };
        if !self.is_configured {
            return;
        }
        if self.last_processed_draft.as_deref() == Some(draft.request_id.as_str()) {
            return;
        }
        if !self.area.input_text().trim().is_empty() {
            self.area.set_pending_conflict(Some(draft.clone()));
            return;
        }
        self.last_processed_draft = Some(draft.request_id.clone());
        self.area.set_input(draft.content.clone());
        self.header.set_tab(AiChatTab::Chat);
        self.focus_input_requested = true;
        if !self.list.is_streaming() {
            self.consumed_draft_ids.push(draft.request_id.clone());
        }
    }

    /// Consume the focus request a prefill or a conflict resolution
    /// left behind: the host focuses `ai-input-text` (the S5 field
    /// key) when this returns `true`.
    pub(crate) fn take_focus_input_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_input_requested)
    }

    /// React's `resolveConflict` (`AiChatPanel.tsx:269–289`): clear the
    /// pending draft, replace or append the input, switch to the chat
    /// tab, and ack the request id. Returns the acked id, or `None`
    /// when no conflict was pending.
    pub(crate) fn resolve_conflict(&mut self, strategy: ConflictResolution) -> Option<String> {
        let request_id = self.area.resolve_conflict(strategy)?;
        self.header.set_tab(AiChatTab::Chat);
        self.focus_input_requested = true;
        self.consumed_draft_ids.push(request_id.clone());
        Some(request_id)
    }

    /// React's `dismissConflict` (`AiChatPanel.tsx:291–293`).
    pub(crate) fn dismiss_conflict(&mut self) {
        self.area.dismiss_conflict();
    }

    // -- Send / stop / cancel --

    /// React's `handleSend` (`AiChatPanel.tsx:310–353`): take the
    /// trimmed draft (nothing when blank or streaming), compose the KV
    /// block, freeze the snapshot onto the send, record the
    /// `sendMessage` payload, append the user's message, and clear the
    /// input, the snapshot and the context items. The store round-trip
    /// is the outbox: `sent_messages` is what the host sends once
    /// T088 lands.
    pub(crate) fn send(&mut self) -> Option<AiSentMessage> {
        let raw = self.area.submit()?;
        let composed = match (
            self.kv_key_name
                .as_deref()
                .filter(|name| !name.trim().is_empty()),
            self.kv_block.as_deref(),
        ) {
            (Some(_), Some(block)) => format!("{raw}\n\n{block}"),
            _ => raw.clone(),
        };
        let template_id = {
            let id = self.header.template_id();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        };
        let record = AiSentMessage {
            content: composed,
            raw_content: raw.clone(),
            agent_mode: self.header.agent_mode(),
            template_id,
            memory_mode: self.effective_memory_mode(),
            memory_scope: self.memory_scope,
            auto_context: self
                .area
                .snapshot()
                .filter(|snapshot| has_snapshot_content(Some(*snapshot)))
                .cloned(),
            context_items: self.area.context_items_count(),
            target_claim: None,
        };
        self.sent_messages.push(record.clone());
        self.append_user_message(raw);
        self.area.set_snapshot(None);
        self.area.set_context_items_count(0);
        self.header.set_has_messages(true);
        Some(record)
    }

    /// React's `onAnswerQuestions` (`AiChatPanel.tsx:710–718`): submit
    /// the message's question block and send the formatted answers as
    /// a plain message — no snapshot, no template, no KV composition.
    /// Returns the recorded send, or `None` when the block has no
    /// non-empty answer (or the message index is unknown).
    pub(crate) fn send_answers(&mut self, index: usize) -> Option<AiSentMessage> {
        let questions = self.list.messages().get(index)?.questions.clone();
        let payload = self
            .list_mut()
            .question_state_mut(index)
            .submit(&questions)?;
        let record = AiSentMessage {
            content: payload.clone(),
            raw_content: payload.clone(),
            agent_mode: self.header.agent_mode(),
            template_id: None,
            memory_mode: self.effective_memory_mode(),
            memory_scope: self.memory_scope,
            auto_context: None,
            context_items: 0,
            target_claim: None,
        };
        self.sent_messages.push(record.clone());
        self.append_user_message(payload);
        Some(record)
    }

    /// React's `handleStop` (`AiChatPanel.tsx:366–369`, BUG-01):
    /// `cancelChat(chatSession?.requestId ?? undefined)`. The VM
    /// records the receipt only when a run is actually in flight.
    pub(crate) fn stop(&mut self) {
        self.cancel_in_flight();
    }

    /// React's unmount effect (`AiChatPanel.tsx:371–379`): unmount, or
    /// switching pane / connection, cancels the in-flight run the
    /// panel started, so a hidden stream cannot keep consuming tokens.
    /// Same receipt as [`AiChatPanelState::stop`].
    pub(crate) fn cancel_in_flight(&mut self) {
        if let Some(request_id) = self.request_id.clone() {
            self.cancelled_request_ids.push(request_id);
        }
    }

    /// React's `clearChat` (`stores/aiStore.ts:723`): clear the
    /// messages, stop the stream, drop the stream fields, the session
    /// memory, the loaded skills, the egress summary and the candidate
    /// notice, and forget the request id.
    pub(crate) fn clear_chat(&mut self) {
        self.clear_chat_presses += 1;
        self.list.set_messages(Vec::new());
        self.list.set_streaming(false);
        self.list.set_stream_content(String::new());
        self.list.set_stream_reasoning(String::new());
        self.list.set_stream_mcp_tool_name(None);
        self.list.set_stream_memory_used(Vec::new());
        self.list.set_session_memory_used(Vec::new());
        self.list.set_memory_candidate_count(None);
        self.area.set_streaming(false);
        self.area.set_loaded_skills(Vec::new());
        self.area.set_egress(
            None,
            self.area.egress_database().map(str::to_string),
            self.area.egress_relaxed(),
            self.area.egress_has_context_files(),
        );
        self.header.set_has_messages(false);
        self.request_id = None;
    }

    /// React's `handleRemember` (`AiChatPanel.tsx:296–326`): blank
    /// content is a no-op; a disabled memory switch shows the
    /// `chat.memoryDisabled` notice and records nothing; otherwise the
    /// `memoryCreate` payload is recorded and the `chat.remembered`
    /// notice shows. Returns whether a request was recorded. A host
    /// failure is reported back through
    /// [`AiChatPanelState::report_remember_failed`].
    pub(crate) fn handle_remember(&mut self, message: &AiChatMessage) -> bool {
        let content = message.content.trim();
        if content.is_empty() {
            return false;
        }
        if !self.memory_enabled {
            self.list
                .set_remember_notice(Some(t("chat.memoryDisabled").to_string()));
            return false;
        }
        let first_line = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or(content);
        let subject: String = first_line.trim().chars().take(80).collect();
        let has_connection = self
            .connection_id
            .as_deref()
            .is_some_and(|id| !id.is_empty());
        self.memory_create_requests.push(AiMemoryCreateRequest {
            scope_type: if has_connection { "connection" } else { "user" },
            connection_id: if has_connection {
                self.connection_id.clone()
            } else {
                None
            },
            kind: "preference",
            subject,
            content: content.to_string(),
            source: "explicit",
            source_ref: self.session_key.clone(),
            status: "candidate",
            sensitivity: "normal",
            confidence: 0.6,
        });
        self.list
            .set_remember_notice(Some(t("chat.remembered").to_string()));
        true
    }

    /// The `catch` branch of `handleRemember`: the command failed, so
    /// the `chat.rememberFailed` notice shows instead.
    pub(crate) fn report_remember_failed(&mut self) {
        self.list
            .set_remember_notice(Some(t("chat.rememberFailed").to_string()));
    }

    /// Append one user message, as the store does after a send.
    fn append_user_message(&mut self, content: String) {
        let mut messages = self.list.messages().to_vec();
        messages.push(AiChatMessage::new(AiChatRole::User, content));
        self.set_messages(messages);
    }
}

/// JS `||` over a list of candidates: the first `Some` non-empty
/// string, mirroring `connectionId || dbSessionId || 'default'`.
fn first_non_empty<'a>(candidates: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
}
