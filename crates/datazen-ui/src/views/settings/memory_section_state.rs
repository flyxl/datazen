//! The MemorySection state machine (the state half of
//! `windows/settings/MemorySection.tsx`'s port, T360 — split out of
//! [`super::memory_section`] for the file-size rule, the
//! `model_profile_form` precedent). The mapping table lives in the
//! section module's header.
//!
//! Carried note: React seeds its state with `normalizeMemorySettings`
//! (`lib/aiMemorySettings.ts`) — the JSON-level partial filler for
//! configs written before Memory settings existed. The Rust side's
//! [`AiMemorySettings`] is a total struct and the api crate's ruling
//! is that the HOST normalizes before delivery (`ai_types/memory.rs`'s
//! own note), so the seed/resync is the delivered value verbatim; the
//! rule degenerates for well-typed configs (the typed enum cannot hold
//! the invalid `cloudPolicy` the filler guarded) — documented, not
//! silently dropped.

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::collections::BTreeSet;

use datazen_ui_api::{AiMemoryCloudPolicy, AiMemoryEntry, AiMemoryKind, AiMemorySettings, AiMemoryStats, AiMemoryStatus};
use rune::TextEdit;

use crate::commands::ai::wire::AiRequest;
use crate::commands::ai::wire::{memory_confirm_request, memory_delete_request, memory_export_request, memory_list_request, memory_reject_request, memory_search_request, memory_stats_request, memory_update_request, org_memory_list_request};

/// `KINDS` — the kind filter's closed list (the enum's table order).
pub(crate) const MEMORY_KINDS: [AiMemoryKind; 8] = [
    AiMemoryKind::Preference,
    AiMemoryKind::Glossary,
    AiMemoryKind::Relationship,
    AiMemoryKind::Pattern,
    AiMemoryKind::Correction,
    AiMemoryKind::Environment,
    AiMemoryKind::Negative,
    AiMemoryKind::SchemaNote,
];

/// `STATUSES` — the status filter's closed list.
pub(crate) const MEMORY_STATUSES: [AiMemoryStatus; 5] = [
    AiMemoryStatus::Candidate,
    AiMemoryStatus::Active,
    AiMemoryStatus::Rejected,
    AiMemoryStatus::Archived,
    AiMemoryStatus::Stale,
];

/// `CLOUD_POLICIES` — the select's closed list.
pub(crate) const MEMORY_CLOUD_POLICIES: [AiMemoryCloudPolicy; 3] = [
    AiMemoryCloudPolicy::NormalOnly,
    AiMemoryCloudPolicy::AllowInternal,
    AiMemoryCloudPolicy::DisableCloud,
];

/// `EMPTY_STATS`.
pub(crate) fn empty_memory_stats() -> AiMemoryStats {
    AiMemoryStats { total: 0, by_status: Default::default(), by_kind: Default::default(), by_scope: Default::default() }
}

/// `MEMORY_EXTRACTION_TOKEN_ESTIMATE`: conservative pre-flight
/// estimate for one automatic extraction pass.
pub(crate) const MEMORY_EXTRACTION_TOKEN_ESTIMATE: u64 = 320;

/// The load effect's debounce (`setTimeout(…, 200)`), in seconds.
pub(crate) const MEMORY_LOAD_DEBOUNCE: f64 = 0.2;
/// The load effect's fixed page (`limit: 200`).
pub(crate) const MEMORY_PAGE_LIMIT: u64 = 200;

/// The in-flight edit (`{ id, subject, content }`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MemoryRowEdit {
    pub id: String,
    pub subject: TextEdit,
    pub content: TextEdit,
}

/// Which of the section's four selects holds its listbox open (the
/// kit's select takes the open flag from state; one at a time, the
/// widget-editor-drawer convention).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemorySelect {
    CloudPolicy,
    Connection,
    Kind,
    Status,
}

/// The export format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoryExportFormat {
    Json,
    Markdown,
}

impl MemoryExportFormat {
    /// The wire spelling (`'json' | 'markdown'`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "markdown",
        }
    }

    /// The download's file extension.
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}

/// What the load walk asks the host for (the debounce's answer): the
/// search-vs-list face, both with the status filter and the limit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MemoryLoadPlan {
    /// `search.trim()` non-empty: `memorySearch({ status?, limit, query })`
    /// — the T12 search IPC predates the connection/kind filters, they
    /// apply client-side after the page lands (see
    /// [`MemorySectionState::apply_loaded`]).
    Search { request: AiRequest, query: String },
    /// `memoryList({ status?, limit, connectionId?, kind? })`.
    List { request: AiRequest },
    /// The stats read that always follows the page.
    Stats { request: AiRequest },
}

/// The section's state: the settings draft plus React's `useState`
/// fields and the `deletedIds` ref.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct MemorySectionState {
    /// `memory` (the normalized settings draft; the host delivers
    /// `config.memory` and the resync replaces).
    pub memory: AiMemorySettings,
    /// `pendingEnable` — the first-use privacy card.
    pub pending_enable: bool,
    /// `entries` (the personal list).
    pub entries: Vec<AiMemoryEntry>,
    /// `organizationEntries` (the read-only org list; a local-only
    /// host answers empty).
    pub organization_entries: Vec<AiMemoryEntry>,
    /// `stats`.
    pub stats: AiMemoryStats,
    /// `loading`.
    pub loading: bool,
    /// `error`.
    pub error: Option<String>,
    /// `notice` (the export's transient line).
    pub notice: Option<String>,
    /// `search`.
    pub search: TextEdit,
    /// `connectionFilter` (`''` = all).
    pub connection_filter: String,
    /// `kindFilter` (`''` = all).
    pub kind_filter: String,
    /// `statusFilter` (`''` = all).
    pub status_filter: String,
    /// `selected`.
    pub selected: BTreeSet<String>,
    /// `editing`.
    pub editing: Option<MemoryRowEdit>,
    /// `refreshToken` — bumped by every mutation; the host re-arms the
    /// load.
    pub refresh_token: u64,
    /// The load debounce's deadline (`setTimeout(200)`): the host
    /// calls [`Self::load_plan`] when `now` passes it.
    pub load_deadline: Option<f64>,
    /// `deletedIds` (the ref): a deleted row must never flash back if
    /// a host refresh races the delete or a stale list response is
    /// still in flight.
    pub deleted_ids: BTreeSet<String>,
    /// The one open select (the kit's open flag; `None` = all closed).
    pub open_select: Option<MemorySelect>,
}

impl MemorySectionState {
    /// React's `useState(() => normalizeMemorySettings(config.memory))`
    /// — the delivered value (the host normalized; see the header).
    pub(crate) fn new(memory: AiMemorySettings) -> Self {
        Self { memory, ..Default::default() }
    }

    /// The `[config.memory]` resync effect: a changed delivered config
    /// replaces the draft (the host calls it on every delivery).
    pub(crate) fn resync_memory(&mut self, memory: &AiMemorySettings) {
        self.memory = memory.clone();
    }

    // ---- the load walk ----

    /// `refresh()`: bump the token and arm the debounce.
    pub(crate) fn refresh(&mut self, now: f64) {
        self.refresh_token += 1;
        self.load_deadline = Some(now + MEMORY_LOAD_DEBOUNCE);
    }

    /// A filter changed: the effect re-runs (the 200ms debounce).
    pub(crate) fn note_filters_changed(&mut self, now: f64) {
        self.load_deadline = Some(now + MEMORY_LOAD_DEBOUNCE);
    }

    /// Whether the debounced load is due.
    pub(crate) fn load_due(&self, now: f64) -> bool {
        self.load_deadline.is_some_and(|at| now >= at)
    }

    /// The debounced effect's request plan: the search-vs-list face
    /// with the status filter, both `limit: 200`. Call this when
    /// [`Self::load_due`]; the host runs the returned requests in
    /// order, then the org list, then answers through
    /// [`Self::begin_load`] → [`Self::apply_loaded`] /
    /// [`Self::deliver_org`] / [`Self::deliver_stats`] /
    /// [`Self::deliver_load_error`].
    pub(crate) fn load_plan(&mut self) -> Vec<MemoryLoadPlan> {
        self.load_deadline = None;
        let status = if self.status_filter.is_empty() {
            None
        } else {
            Some(serde_json::json!(self.status_filter))
        };
        let mut params = serde_json::json!({ "limit": MEMORY_PAGE_LIMIT });
        if let Some(status) = &status {
            params["status"] = status.clone();
        }
        let query = self.search.text.trim().to_owned();
        let mut plan = Vec::new();
        if !query.is_empty() {
            params["query"] = serde_json::json!(query);
            plan.push(MemoryLoadPlan::Search { request: memory_search_request(&params), query });
        } else {
            if !self.connection_filter.is_empty() {
                params["connectionId"] = serde_json::json!(self.connection_filter);
            }
            if !self.kind_filter.is_empty() {
                params["kind"] = serde_json::json!(self.kind_filter);
            }
            plan.push(MemoryLoadPlan::List { request: memory_list_request(&params) });
        }
        plan.push(MemoryLoadPlan::Stats { request: memory_stats_request() });
        plan
    }

    /// The load begins (`setLoading(true); setError(null)`).
    pub(crate) fn begin_load(&mut self) {
        self.loading = true;
        self.error = None;
    }

    /// The page landed: the deleted-ids guard, the search-mode
    /// client-side connection/kind filters (T12's search predates
    /// them), the entries replace, and `selected` prunes to the
    /// visible rows.
    pub(crate) fn apply_loaded(&mut self, page_entries: Vec<AiMemoryEntry>, search_mode: bool) {
        let mut next: Vec<AiMemoryEntry> = page_entries
            .into_iter()
            .filter(|entry| !self.deleted_ids.contains(&entry.id))
            .collect();
        if search_mode {
            if !self.connection_filter.is_empty() {
                next.retain(|entry| entry.connection_id.as_deref() == Some(self.connection_filter.as_str()));
            }
            if !self.kind_filter.is_empty() {
                next.retain(|entry| entry.kind.as_str() == self.kind_filter);
            }
        }
        self.selected.retain(|id| next.iter().any(|entry| &entry.id == id));
        self.entries = next;
    }

    /// The stats answer (a failed stats read is the empty stats —
    /// `(await aiCommands.memoryStats()) ?? EMPTY_STATS`).
    pub(crate) fn deliver_stats(&mut self, stats: Option<AiMemoryStats>) {
        self.stats = stats.unwrap_or_else(empty_memory_stats);
    }

    /// The load's `finally`.
    pub(crate) fn load_finished(&mut self) {
        self.loading = false;
    }

    /// The load threw.
    pub(crate) fn deliver_load_error(&mut self, message: String) {
        self.error = Some(message);
        self.loading = false;
    }

    /// The org list's answer (a local-only host shows the empty
    /// read-only list — the catch's `[]`).
    pub(crate) fn deliver_org(&mut self, entries: Vec<AiMemoryEntry>) {
        self.organization_entries = entries;
    }

    // ---- the settings draft ----

    /// `persist`: the optimistic write with the revert on failure —
    /// the HOST runs `saveSettingsConfig({...config, memory: next})`
    /// and answers; `false` restores the previous draft.
    pub(crate) fn note_persist_result(&mut self, next: &AiMemorySettings, ok: bool) {
        if ok {
            self.memory = next.clone();
        }
        // A failed write leaves the previous draft in place (the
        // caller applied nothing).
    }

    /// `toggleEnabled`: enabling without the privacy acknowledgement
    /// opens the first-use card instead of writing.
    pub(crate) fn toggle_enabled(&mut self, enabled: bool) -> bool {
        if enabled && !self.memory.privacy_acknowledged {
            self.pending_enable = true;
            return false;
        }
        true // the host persists {...memory, enabled}
    }

    /// `confirmEnable`: the card closes and the write lands with the
    /// acknowledgement set.
    pub(crate) fn confirm_enable(&mut self) -> AiMemorySettings {
        self.pending_enable = false;
        AiMemorySettings { enabled: true, privacy_acknowledged: true, ..self.memory }
    }

    /// The card's cancel.
    pub(crate) fn cancel_enable(&mut self) {
        self.pending_enable = false;
    }

    /// `setCloudPolicy`: only a member of the closed list writes (the
    /// typed enum makes the guard total; the shape stays).
    pub(crate) fn set_cloud_policy(&mut self, policy: AiMemoryCloudPolicy) -> AiMemorySettings {
        AiMemorySettings { cloud_policy: policy, ..self.memory }
    }

    // ---- the projections ----

    /// `candidateEntries`.
    pub(crate) fn candidate_entries(&self) -> Vec<&AiMemoryEntry> {
        self.entries.iter().filter(|entry| entry.status == AiMemoryStatus::Candidate).collect()
    }

    /// `candidateCount` (`stats.byStatus.candidate ?? 0`).
    pub(crate) fn candidate_count(&self) -> u64 {
        self.stats.by_status.get("candidate").copied().unwrap_or(0)
    }

    /// `selectedCandidates`.
    pub(crate) fn selected_candidates(&self) -> Vec<&AiMemoryEntry> {
        self.candidate_entries()
            .into_iter()
            .filter(|entry| self.selected.contains(&entry.id))
            .collect()
    }

    /// `allSelected`.
    pub(crate) fn all_selected(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|entry| self.selected.contains(&entry.id))
    }

    /// `toggleSelected`.
    pub(crate) fn toggle_selected(&mut self, id: &str) {
        if !self.selected.remove(id) {
            self.selected.insert(id.to_owned());
        }
    }

    /// `toggleAll`.
    pub(crate) fn toggle_all(&mut self) {
        if self.all_selected() {
            self.selected.clear();
        } else {
            self.selected = self.entries.iter().map(|entry| entry.id.clone()).collect();
        }
    }

    /// `connectionOptions`: the known connections, plus any
    /// connectionId the entries carry that the store does not know
    /// (the id doubles as the label).
    pub(crate) fn connection_options(
        &self,
        connections: &[(String, String)],
    ) -> Vec<(String, String)> {
        let mut known: Vec<(String, String)> = connections.to_vec();
        for entry in &self.entries {
            if let Some(id) = &entry.connection_id {
                if !known.iter().any(|(known_id, _)| known_id == id) {
                    known.push((id.clone(), id.clone()));
                }
            }
        }
        known
    }

    // ---- the row actions (plans the host runs) ----

    /// `updateRow`'s request: `memoryUpdate(id, patch)` — the editing
    /// row's subject/content.
    pub(crate) fn update_row_request(&self, id: &str, subject: &str, content: &str) -> AiRequest {
        memory_update_request(
            id,
            &serde_json::json!({ "subject": subject, "content": content }),
        )
    }

    /// The edit saved: the form closes and the list refreshes.
    pub(crate) fn note_row_updated(&mut self, now: f64) {
        self.editing = None;
        self.refresh(now);
    }

    /// `confirmAll`: every candidate's confirm request (the host runs
    /// them; `selected` clears and the list refreshes).
    pub(crate) fn confirm_all_requests(&self) -> Vec<AiRequest> {
        self.candidate_entries()
            .iter()
            .map(|entry| memory_confirm_request(&entry.id))
            .collect()
    }

    pub(crate) fn note_confirmed_all(&mut self, now: f64) {
        self.selected.clear();
        self.refresh(now);
    }

    /// `deleteOne`.
    pub(crate) fn delete_one_request(&self, id: &str) -> AiRequest {
        memory_delete_request(id)
    }

    /// The delete landed: the tombstone guards the refresh race, the
    /// row and its selection drop, the list refreshes.
    pub(crate) fn note_deleted(&mut self, ids: &[String], now: f64) {
        for id in ids {
            self.deleted_ids.insert(id.clone());
            self.selected.remove(id);
        }
        self.entries.retain(|entry| !ids.contains(&entry.id));
        self.refresh(now);
    }

    /// `deleteSelected`.
    pub(crate) fn delete_selected_requests(&self) -> Vec<AiRequest> {
        self.selected.iter().map(|id| memory_delete_request(id)).collect()
    }

    /// `mergeSelected`: the merge plan — nothing below two chosen
    /// candidates; the merged content dedups the trimmed contents
    /// (first-seen order) joined by a blank line, the first row keeps
    /// its subject and takes `max(confidence)` + `status: active`, the
    /// rest delete.
    pub(crate) fn merge_selected_plan(&self) -> MemoryMergePlan {
        let chosen = self.selected_candidates();
        if chosen.len() < 2 {
            return MemoryMergePlan::NeedTwo;
        }
        let mut seen: Vec<String> = Vec::new();
        for entry in &chosen {
            let trimmed = entry.content.trim().to_owned();
            if !trimmed.is_empty() && !seen.iter().any(|s| *s == trimmed) {
                seen.push(trimmed);
            }
        }
        let merged_content = seen.join("\n\n");
        let confidence = chosen.iter().map(|entry| entry.confidence).fold(f64::MIN, f64::max);
        let first = chosen[0];
        MemoryMergePlan::Merge {
            update: memory_update_request(
                &first.id,
                &serde_json::json!({
                    "subject": first.subject,
                    "content": merged_content,
                    "confidence": confidence,
                    "status": "active",
                }),
            ),
            delete_rest: chosen[1..].iter().map(|entry| memory_delete_request(&entry.id)).collect(),
            merged_ids: chosen.iter().map(|entry| entry.id.clone()).collect(),
        }
    }

    /// The merge landed: the merged-away rows tombstone, the selection
    /// clears, the list refreshes.
    pub(crate) fn note_merged(&mut self, merged_ids: &[String], now: f64) {
        for id in &merged_ids[1..] {
            self.deleted_ids.insert(id.clone());
        }
        self.selected.clear();
        self.refresh(now);
    }

    /// One candidate's confirm / reject buttons.
    pub(crate) fn confirm_request(&self, id: &str) -> AiRequest {
        memory_confirm_request(id)
    }

    pub(crate) fn reject_request(&self, id: &str) -> AiRequest {
        memory_reject_request(id)
    }

    /// The stale row's "mark reviewed" write.
    pub(crate) fn mark_reviewed_request(&self, id: &str) -> AiRequest {
        memory_update_request(id, &serde_json::json!({ "status": "active" }))
    }

    /// `exportMemories`: the notice arms and the export request rides
    /// with the CURRENT status filter; the host downloads the file
    /// (`datazen-memory.{json|md}`) — the browser download is the
    /// host's, the `jsdom` guard with it.
    pub(crate) fn begin_export(
        &mut self,
        format: MemoryExportFormat,
    ) -> AiRequest {
        self.notice = Some("settings.ai.memory.exportNotice".to_owned());
        let mut params = serde_json::json!({ "format": format.as_str() });
        if !self.status_filter.is_empty() {
            params["status"] = serde_json::json!(self.status_filter);
        }
        memory_export_request(&params)
    }

    /// The org list's mount request (`orgMemoryList({ limit: 200 })`,
    /// re-run with every refresh token).
    pub(crate) fn org_load_request(&self) -> AiRequest {
        org_memory_list_request(&serde_json::json!({ "limit": MEMORY_PAGE_LIMIT }))
    }

    // ---- the edit form ----

    /// The pencil press: seed the edit from the row.
    pub(crate) fn begin_edit(&mut self, entry: &AiMemoryEntry) {
        self.editing = Some(MemoryRowEdit {
            id: entry.id.clone(),
            subject: TextEdit::new(entry.subject.clone()),
            content: TextEdit::new(entry.content.clone()),
        });
    }

    /// The form's cancel.
    pub(crate) fn cancel_edit(&mut self) {
        self.editing = None;
    }

    /// One select's trigger press: open it (closing any other).
    pub(crate) fn toggle_select(&mut self, which: MemorySelect) {
        self.open_select = if self.open_select == Some(which) { None } else { Some(which) };
    }

    /// Close the open listbox (an outside press / Escape).
    pub(crate) fn close_select(&mut self) {
        self.open_select = None;
    }
}

/// `mergeSelected`'s plan.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MemoryMergePlan {
    /// `chosen.length < 2`: nothing happens.
    NeedTwo,
    /// The first row's update (subject kept, merged content, max
    /// confidence, active), the rest's deletes.
    Merge {
        update: AiRequest,
        delete_rest: Vec<AiRequest>,
        /// Every chosen id (the note_merged half needs them).
        merged_ids: Vec<String>,
    },
}
