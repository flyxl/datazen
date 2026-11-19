//! Input-area state — T009-S5 (`AiChatPanel.tsx:777–949`).
//!
//! The view-model placeholder for the `aiStore` slice (T088, not done):
//! the draft input, the streaming flag, the auto-context snapshot, the
//! loaded skills, the injection snapshot, the KV context key, the pending
//! draft conflict, and the egress fields the `AiEgressNotice` (T105)
//! reads. The builder lives in `ai_chat_input_area.rs`; the split keeps
//! each file under the 800-line cap. See that module's docs for the key
//! convention and the deliberate boundaries.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use rune::TextEdit;

use super::ai_chat_panel::{has_snapshot_content, AiChatContextSnapshot};
use crate::i18n::t;

/// One loaded skill (`AiLoadedSkill`, `types/ai.ts`): the chip shows the
/// name; the key is `skill.id ?? skill.name`, exactly as React keys the
/// chip (`AiChatPanel.tsx:793`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiLoadedSkill {
    pub id: Option<String>,
    pub name: String,
}

impl AiLoadedSkill {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
        }
    }

    pub(crate) fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// React's chip key: `skill.id ?? skill.name`.
    pub(crate) fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.name)
    }
}

/// React's `AiChatInjectionSnapshot` (`types/ai.ts:472`): the frozen
/// template / notes / global-instructions facts the panel shows as
/// read-only chips. Only the presence flags and the template name render;
/// `templateId` / `templateHash` never display.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AiInjection {
    pub template_name: Option<String>,
    pub has_connection_notes: bool,
    pub has_global_instructions: bool,
}

impl AiInjection {
    /// React's injection-row gate (`AiChatPanel.tsx:845–848`): any of the
    /// template name, the connection-notes flag, or the global-
    /// instructions flag shows the row.
    pub(crate) fn shows_row(&self) -> bool {
        self.template_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty())
            || self.has_connection_notes
            || self.has_global_instructions
    }

    /// React's priority-chip gate (`AiChatPanel.tsx:869–871`): global
    /// instructions show the priority chip only together with a template.
    pub(crate) fn shows_priority(&self) -> bool {
        self.has_global_instructions
            && self
                .template_name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())
    }
}

/// One pending draft-conflict request (`AiChatDraftRequest`,
/// `windows/connection/query/aiDraftBridge.ts:20`): the draft that arrived
/// while the input already held text. Only the request id (for the
/// `onDraftConsumed` ack, owned by the panel slice) and the content (what
/// replace / append prefills) render here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiPendingDraft {
    pub request_id: String,
    pub content: String,
}

impl AiPendingDraft {
    pub(crate) fn new(request_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            content: content.into(),
        }
    }
}

/// React's `resolveConflict` strategy (`AiChatPanel.tsx:269–289`):
/// `replace` swaps the input for the draft, `append` joins it after the
/// `chat.draft.appendSeparator` separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConflictResolution {
    Replace,
    Append,
}

/// The mutable state behind an [`AiChatInputArea`]: the view-model
/// placeholder for the `aiStore` slice (T088, not done). The caller owns
/// one of these (in `AppState` once the panel lands; in the probe state in
/// tests) and hands `&state` to the builder each frame; presses call back
/// into the caller, which drives the state machine. Defaults mirror
/// React's initials: an empty input, nothing streaming, no snapshot, no
/// skills, no injection, no KV key, no pending conflict, no egress.
#[derive(Clone, Debug, Default)]
pub(crate) struct AiChatInputAreaState {
    input: TextEdit,
    is_streaming: bool,
    snapshot: Option<AiChatContextSnapshot>,
    loaded_skills: Vec<AiLoadedSkill>,
    injection: Option<AiInjection>,
    kv_key_name: Option<String>,
    pending_conflict: Option<AiPendingDraft>,
    egress_summary: Option<String>,
    egress_database: Option<String>,
    egress_relaxed: bool,
    egress_has_context_files: bool,
    context_items_count: usize,
    /// `AiInput`'s `dbSessionId` prop (T139). Stored only until that slice
    /// lands; the placeholder field never reads it.
    db_session_id: Option<String>,
    /// `AiInput`'s `database` prop (T139). Stored only, as above.
    database: Option<String>,
}

impl AiChatInputAreaState {
    pub(crate) fn input_text(&self) -> &str {
        &self.input.text
    }

    /// The draft buffer, for the input field's edit binding.
    pub(crate) fn input(&self) -> &TextEdit {
        &self.input
    }

    pub(crate) fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    pub(crate) fn snapshot(&self) -> Option<&AiChatContextSnapshot> {
        self.snapshot.as_ref()
    }

    pub(crate) fn loaded_skills(&self) -> &[AiLoadedSkill] {
        &self.loaded_skills
    }

    pub(crate) fn injection(&self) -> Option<&AiInjection> {
        self.injection.as_ref()
    }

    pub(crate) fn kv_key_name(&self) -> Option<&str> {
        self.kv_key_name.as_deref()
    }

    pub(crate) fn pending_conflict(&self) -> Option<&AiPendingDraft> {
        self.pending_conflict.as_ref()
    }

    pub(crate) fn egress_summary(&self) -> Option<&str> {
        self.egress_summary.as_deref()
    }

    pub(crate) fn egress_database(&self) -> Option<&str> {
        self.egress_database.as_deref()
    }

    pub(crate) fn egress_relaxed(&self) -> bool {
        self.egress_relaxed
    }

    pub(crate) fn egress_has_context_files(&self) -> bool {
        self.egress_has_context_files
    }

    pub(crate) fn context_items_count(&self) -> usize {
        self.context_items_count
    }

    pub(crate) fn db_session_id(&self) -> Option<&str> {
        self.db_session_id.as_deref()
    }

    pub(crate) fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    /// React's `AiInput` send gate (`AiInput.tsx:70`): non-blank input
    /// while not streaming. `handleSend` (`AiChatPanel.tsx:325`) guards
    /// the same way, so the state machine — not the caller — owns it.
    pub(crate) fn can_send(&self) -> bool {
        !self.input.text.trim().is_empty() && !self.is_streaming
    }

    /// The edit accessor the input field binds: returns the draft buffer.
    pub(crate) fn input_edit(&mut self) -> &mut TextEdit {
        &mut self.input
    }

    /// React's loaded-skills gate (`AiChatPanel.tsx:782`): any skill
    /// shows the row. `normalizeLoadedSkills` already drops nameless
    /// entries, so the VM stores only displayable skills.
    pub(crate) fn show_loaded_skills(&self) -> bool {
        !self.loaded_skills.is_empty()
    }

    /// React's auto-context gate (`AiChatPanel.tsx:805`): S1's
    /// [`has_snapshot_content`] on the snapshot.
    pub(crate) fn show_auto_context(&self) -> bool {
        has_snapshot_content(self.snapshot.as_ref())
    }

    /// React's injection gate (`AiChatPanel.tsx:845–848`).
    pub(crate) fn show_injection(&self) -> bool {
        self.injection.as_ref().is_some_and(AiInjection::shows_row)
    }

    /// React's KV-chip gate (`AiChatPanel.tsx:885`): `kvContext` is
    /// non-null only when a key is selected
    /// (`lib/kvAiContext.ts:64` returns `null` for an empty key), so the
    /// VM shows the chip only for a non-blank key name.
    pub(crate) fn show_kv_chip(&self) -> bool {
        self.kv_key_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty())
    }

    /// React's conflict-bar gate (`AiChatPanel.tsx:898`): a pending draft
    /// conflict is showing.
    pub(crate) fn show_conflict_bar(&self) -> bool {
        self.pending_conflict.is_some()
    }

    /// `AiEgressNotice`'s show gate (`AiEgressNotice.tsx:26`): the
    /// notice renders when strict egress is relaxed, when context files
    /// are attached, or when a summary arrived. A database label alone
    /// does not show it — React checks the database only inside the
    /// rendered notice.
    pub(crate) fn show_egress_notice(&self) -> bool {
        self.egress_relaxed
            || self.egress_has_context_files
            || self
                .egress_summary
                .as_deref()
                .is_some_and(|summary| !summary.trim().is_empty())
    }

    /// `AiEgressNotice`'s summary line (`AiEgressNotice.tsx:17`): the
    /// trimmed summary, when non-empty.
    pub(crate) fn egress_summary_text(&self) -> Option<&str> {
        self.egress_summary
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
    }

    /// `AiEgressNotice`'s database line (`AiEgressNotice.tsx:21`): the
    /// trimmed database label, when non-empty.
    pub(crate) fn egress_database_label(&self) -> Option<String> {
        let database = self.egress_database.as_deref().map(str::trim)?;
        if database.is_empty() {
            return None;
        }
        Some(t("chat.egressDatabase").replacen("{database}", database, 1))
    }

    /// `AiEgressNotice`'s relaxed / context messages, in React's order
    /// (`AiEgressNotice.tsx:29–35`): the relaxed-egress note first, then
    /// the attached-context note.
    pub(crate) fn egress_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();
        if self.egress_relaxed {
            messages.push(t("chat.egressNoticeRelaxed").to_string());
        }
        if self.egress_has_context_files {
            messages.push(t("chat.egressNoticeContext").to_string());
        }
        messages
    }

    /// React's `AiInput` placeholder rule (`AiInput.tsx:162–164`): the
    /// panel's `chat.placeholder` shows only while no context tokens are
    /// attached — with tokens the placeholder is suppressed, and the
    /// field's own empty state is what shows. The text field only
    /// displays a placeholder while empty, so the count is the only
    /// extra condition the builder needs.
    pub(crate) fn input_placeholder(&self) -> Option<&str> {
        if self.context_items_count == 0 {
            Some(t("chat.placeholder"))
        } else {
            None
        }
    }

    /// React's `handleSend` guard + `setInput('')`
    /// (`AiChatPanel.tsx:325,352`): take the trimmed draft and clear the
    /// field, or `None` when [`AiChatInputAreaState::can_send`] is false.
    /// The send itself — `composeKvAiMessage`, the snapshot attach, the
    /// context split, clearing `contextItems` — is the panel slice's job;
    /// this only owns the draft.
    pub(crate) fn submit(&mut self) -> Option<String> {
        if !self.can_send() {
            return None;
        }
        let content = self.input.text.trim().to_string();
        self.input = TextEdit::new(String::new());
        Some(content)
    }

    /// React's `resolveConflict` (`AiChatPanel.tsx:269–289`): clear the
    /// pending draft, then replace the input with the draft content, or
    /// append it after the `chat.draft.appendSeparator` separator. The
    /// `onDraftConsumed` ack and the tab switch / focus are the panel
    /// slice's job; the request id is returned so the caller can ack it.
    /// Returns `None` when no conflict is pending.
    pub(crate) fn resolve_conflict(&mut self, strategy: ConflictResolution) -> Option<String> {
        let draft = self.pending_conflict.take()?;
        match strategy {
            ConflictResolution::Replace => {
                self.input = TextEdit::new(draft.content);
            }
            ConflictResolution::Append => {
                let separator = t("chat.draft.appendSeparator");
                self.input =
                    TextEdit::new(format!("{}{}{}", self.input.text, separator, draft.content));
            }
        }
        Some(draft.request_id)
    }

    /// React's `dismissConflict` (`AiChatPanel.tsx:291–293`): drop the
    /// pending draft, leaving the input untouched.
    pub(crate) fn dismiss_conflict(&mut self) {
        self.pending_conflict = None;
    }

    /// React's `removeSnapshotField` (`AiChatPanel.tsx:159–165`): delete
    /// one field; when nothing with content is left the snapshot itself
    /// goes `null`. Unknown field names are a no-op. Field names are
    /// React's `keyof AiChatContextSnapshot`: `editorSql`, `selection`,
    /// `lastError`, `resultPreview`.
    pub(crate) fn remove_snapshot_field(&mut self, field: &str) {
        let Some(snapshot) = self.snapshot.as_mut() else {
            return;
        };
        match field {
            "editorSql" => snapshot.editor_sql = None,
            "selection" => snapshot.selection = None,
            "lastError" => snapshot.last_error = None,
            "resultPreview" => snapshot.result_preview = None,
            _ => return,
        }
        if !has_snapshot_content(self.snapshot.as_ref()) {
            self.snapshot = None;
        }
    }

    // -- VM seeding: the store writes the panel slice will make once T088
    // lands. Until then tests (and the panel slice's placeholder) drive
    // these directly. --

    pub(crate) fn set_input(&mut self, input: impl Into<String>) {
        self.input = TextEdit::new(input.into());
    }

    pub(crate) fn set_streaming(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
    }

    pub(crate) fn set_snapshot(&mut self, snapshot: Option<AiChatContextSnapshot>) {
        self.snapshot = snapshot;
    }

    pub(crate) fn set_loaded_skills(&mut self, skills: Vec<AiLoadedSkill>) {
        self.loaded_skills = skills;
    }

    pub(crate) fn set_injection(&mut self, injection: Option<AiInjection>) {
        self.injection = injection;
    }

    pub(crate) fn set_kv_key_name(&mut self, key_name: Option<String>) {
        self.kv_key_name = key_name;
    }

    pub(crate) fn set_pending_conflict(&mut self, draft: Option<AiPendingDraft>) {
        self.pending_conflict = draft;
    }

    /// Seed the egress fields the `AiEgressNotice` (T105) reads: the host
    /// summary, the database label, whether strict egress is relaxed
    /// (`aiStrictEgress === false` in the settings store), and whether any
    /// attached context item is a file or directory. The VM carries the
    /// values because the settings store and the context-item split live
    /// outside this slice.
    pub(crate) fn set_egress(
        &mut self,
        summary: Option<String>,
        database: Option<String>,
        relaxed: bool,
        has_context_files: bool,
    ) {
        self.egress_summary = summary;
        self.egress_database = database;
        self.egress_relaxed = relaxed;
        self.egress_has_context_files = has_context_files;
    }

    pub(crate) fn set_context_items_count(&mut self, count: usize) {
        self.context_items_count = count;
    }

    /// `AiInput`'s `dbSessionId` prop (T139). Stored only.
    pub(crate) fn set_db_session_id(&mut self, db_session_id: Option<String>) {
        self.db_session_id = db_session_id;
    }

    /// `AiInput`'s `database` prop (T139). Stored only.
    pub(crate) fn set_database(&mut self, database: Option<String>) {
        self.database = database;
    }
}
