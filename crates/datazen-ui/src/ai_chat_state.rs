//! Host sync for the AI assistant panel — T009-S7, the `Section`
//! wiring for T009.
//!
//! React's `AiChatPanel` is a pure render + event router: its `useEffect`s
//! are the host's job. In immediate mode there is no effect runner, so
//! this `impl AppState` block is the runner — [`AppState::sync_ai_chat_panel`]
//! is called from [`crate::app_poll`] once per frame and performs the
//! effect table from `ai_chat_panel_state.rs`:
//!
//! | React effect | Host step here |
//! |---|---|
//! | session-key effect (197–201) | `ensure_session` after the connection id write |
//! | snapshot sync (130–157) | `sync_auto_context` from the live editor / result grid |
//! | draft bridge (221–249) | `handle_draft_request`, re-issued on the transitions below |
//! | unmount cancel (371–379) | `cancel_in_flight` once, on the mounted → unmounted transition |
//! | `loadConfig` (166–168) | — the host seeds `is_configured` directly once T088 lands |
//!
//! # What the host does NOT own yet (T088)
//!
//! `aiStore` has no Rune slice, so the sync writes only what it has a
//! source for: the live connection id, the host-owned memory switch,
//! the editor / result snapshot, the draft the host holds, and the
//! consumed-draft acks. It deliberately does **not** write `database`,
//! `database_type`, `sql_dialect` or `is_configured` — no Rune source
//! exists for any of them (the settings page's `ai` section is still a
//! placeholder, the workspace tracks no current database) — and it does
//! **not** drain the `sent_messages` / `cancelled_request_ids` /
//! `memory_create_requests` outboxes: those have no host consumer until
//! T088 wires the real store, and draining them would destroy the
//! history T088 reads. The only outbox the host consumes is
//! `consumed_draft_ids`, because clearing the pending draft is the
//! host's own ack (React's `onDraftConsumed`).
//!
//! # Draft re-issue (tester S6 non-blocking #4)
//!
//! React's draft effect re-runs when `input` changes, so a draft that
//! parked as a conflict (non-empty input) prefills as soon as the input
//! clears. The VM's `handle_draft_request` is one-shot, so the host
//! re-issues it when the issue key `(request id, input-was-empty,
//! was-configured)` changes. Re-issuing on every poll would be wrong:
//! dismissing the conflict while the input stays non-empty must stick
//! (React does not re-run the effect then — its deps did not change),
//! and a re-issue after a stream ends is a VM no-op (the processed
//! mark blocks it), so streaming is not part of the key.
//!
//! # Memory seeding (tester S6 non-blocking #3)
//!
//! The panel VM defaults `memory_enabled` to `true` (effective
//! `'auto'`), but React's memory is opt-in:
//! `defaultAiMemorySettings.enabled: false`, so an unset config sends
//! `'off'`. The sync writes `state.ai_memory_enabled` (host-owned,
//! default `false`) into the panel on every poll, so the effective
//! mode never depends on the VM default.

use rune::Tx;

use crate::components::ai::{
    has_snapshot_content, AiChatContextSnapshot, AiChatResultPreview, AiPendingDraft,
};
use crate::sql_editor::SqlEditor;
use crate::{query_selected, Section};
use serde_json::Value;

/// React's auto-context caps (`ContentViewDrawers.tsx:186–224`):
/// 20 columns, 5 rows, 20 cells per row, 500 characters per cell.
const SNAPSHOT_MAX_COLUMNS: u32 = 20;
const SNAPSHOT_MAX_ROWS: u32 = 5;
const SNAPSHOT_MAX_CELL_CHARS: usize = 500;

impl crate::AppState {
    /// The toolbar toggle (`ContentToolbar.tsx:228–240`, testid
    /// `conn-toolbar-ai`, label `AI`, secondary variant while open).
    /// Opening resets the draft issue key so a still-pending draft is
    /// re-issued on the next sync, as React's remount re-runs the
    /// draft effect from a fresh ref. Closing cancels nothing here:
    /// the unmount cancel lives in exactly one place —
    /// [`AppState::sync_ai_chat_panel`]'s mounted → unmounted
    /// transition — so one close yields one receipt however the panel
    /// was hidden (toggle or section switch).
    pub(crate) fn toggle_ai_chat(&mut self) {
        let open = !self.ai_chat_open;
        if open {
            self.ai_draft_last_issue = None;
        }
        self.ai_chat_open = open;
    }

    /// The host entry for a draft request (React `ContentView`'s
    /// `openAiChatDraft`: store the request and open the panel). No
    /// production caller yet: the only producer is the query-error
    /// panel's Ask-AI action (`queryErrorChatPrompt.ts`), and Rune's
    /// query error surface is still a one-line `query_notice` — that
    /// action wires this entry when it lands.
    #[allow(dead_code)]
    pub(crate) fn open_ai_chat_draft(&mut self, draft: AiPendingDraft) {
        self.ai_pending_draft = Some(draft);
        if !self.ai_chat_open {
            self.ai_chat_open = true;
            self.ai_draft_last_issue = None;
        }
    }

    /// The `onInsertSql` host prop (React `updateQuerySql`): *replace*
    /// the active query editor's SQL. It fires only with a query tab
    /// selected — React only calls it for a `query` panel — so the
    /// document is untouched otherwise. [`SqlEditor::from_doc`] resets
    /// the caret and undo, the closest honest equivalent of the store
    /// replacing the pane's SQL.
    pub(crate) fn insert_ai_sql(&mut self, sql: &str) {
        if query_selected(self) {
            self.sql = SqlEditor::from_doc(sql);
        }
    }

    /// The `onNewQuery` host prop: open a new query tab prefilled with
    /// the code block. The editor document is replaced only when the
    /// tab actually opened (`Tabs::open` fails only on an empty title,
    /// and the title here is constant).
    pub(crate) fn open_ai_new_query(&mut self, sql: &str) {
        if self.open_query_tab("Query").is_some() {
            self.sql = SqlEditor::from_doc(sql);
        }
    }

    /// One poll's host sync for the panel. Returns whether anything
    /// changed (the caller redraws on `true`). Mount tracking runs on
    /// every section so leaving the workspace cancels the in-flight
    /// run exactly once; the prop writes run only while mounted.
    pub(crate) fn sync_ai_chat_panel(&mut self, tx: &mut Tx) -> bool {
        let mounted = self.active == Section::Workspace && self.ai_chat_open;
        if !mounted {
            if self.ai_chat_mounted {
                self.ai_chat_mounted = false;
                self.ai_chat_panel.cancel_in_flight();
                return true;
            }
            return false;
        }
        self.ai_chat_mounted = true;
        let mut changed = false;

        // The live connection id re-keys the session (React's
        // session-key effect); the template reset rides on the change.
        let connection_before = self.ai_chat_panel.connection_id().map(str::to_string);
        self.ai_chat_panel
            .set_connection_id(self.desktop_connection.clone());
        if self.ai_chat_panel.connection_id().map(str::to_string) != connection_before {
            changed = true;
        }

        // S6 non-blocking #3: the host-owned memory switch seeds the
        // panel every poll — never the VM's `true` default.
        if self.ai_chat_panel.memory_enabled() != self.ai_memory_enabled {
            self.ai_chat_panel
                .set_memory_enabled(self.ai_memory_enabled);
            changed = true;
        }

        if self.ai_chat_panel.ensure_session() {
            changed = true;
        }

        // The auto-context snapshot follows the live query panel.
        let snapshot_before = self.ai_chat_panel.area().snapshot().cloned();
        let snapshot = self.ai_chat_snapshot();
        self.ai_chat_panel.sync_auto_context(snapshot.as_ref());
        if self.ai_chat_panel.area().snapshot().cloned() != snapshot_before {
            changed = true;
        }

        // S6 non-blocking #4: re-issue a parked draft when its issue
        // key changes (new draft, input emptiness flipped, or the
        // configured flag flipped).
        if let Some(draft) = self.ai_pending_draft.clone() {
            let input_empty = self.ai_chat_panel.area().input_text().trim().is_empty();
            let issue = (
                draft.request_id.clone(),
                input_empty,
                self.ai_chat_panel.is_configured(),
            );
            if self.ai_draft_last_issue.as_ref() != Some(&issue) {
                self.ai_draft_last_issue = Some(issue);
                let input_before = self.ai_chat_panel.area().input_text().to_string();
                let conflict_before = self
                    .ai_chat_panel
                    .area()
                    .pending_conflict()
                    .map(|pending| pending.request_id.clone());
                self.ai_chat_panel.handle_draft_request(Some(&draft));
                let input_after = self.ai_chat_panel.area().input_text().to_string();
                let conflict_after = self
                    .ai_chat_panel
                    .area()
                    .pending_conflict()
                    .map(|pending| pending.request_id.clone());
                if input_after != input_before || conflict_after != conflict_before {
                    changed = true;
                }
            }
        }

        // A prefill or a conflict resolution asks for the input focus;
        // the host focuses `ai-input-text` (the S5 field key).
        if self.ai_chat_panel.take_focus_input_request() {
            tx.focus("ai-input-text", true);
            changed = true;
        }

        // The consumed-draft outbox is the host's ack: a newly acked id
        // clears the pending draft (React's `onDraftConsumed`).
        let acks = self.ai_chat_panel.consumed_draft_ids();
        if acks.len() > self.ai_draft_acks_applied {
            let fresh = &acks[self.ai_draft_acks_applied..];
            if let Some(draft) = &self.ai_pending_draft {
                if fresh.iter().any(|id| id == &draft.request_id) {
                    self.ai_pending_draft = None;
                    self.ai_draft_last_issue = None;
                    changed = true;
                }
            }
            self.ai_draft_acks_applied = acks.len();
        }

        changed
    }

    /// React's `autoContext` memo (`ContentViewDrawers.tsx:186–224`),
    /// read off the live query panel: the editor's trimmed SQL, the
    /// editor selection, and the result head (20 columns / 5 rows /
    /// 500 chars per cell). `None` when the workspace shows no query
    /// tab or nothing has content — the same `undefined` React passes
    /// then. Two honest gaps, both T088-owned: `last_error` has no
    /// Rune source (no query-error field in `AppState`), and grid
    /// cells are already strings, so every cell is a truncated string
    /// instead of React's number / boolean / JSON distinction.
    fn ai_chat_snapshot(&self) -> Option<AiChatContextSnapshot> {
        if !query_selected(self) {
            return None;
        }
        let editor_sql = self.sql.doc.trim();
        let editor_sql = if editor_sql.is_empty() {
            None
        } else {
            Some(editor_sql.to_string())
        };
        let selection = self.sql.selected_text();
        let selection = selection.trim();
        let selection = if selection.is_empty() {
            None
        } else {
            Some(selection.to_string())
        };
        let result_preview = self.result_grid.as_ref().and_then(|grid| {
            if grid.cols == 0 && grid.rows == 0 {
                return None;
            }
            let columns: Vec<String> = (0..grid.cols.min(SNAPSHOT_MAX_COLUMNS))
                .map(|col| grid.column_name(col).unwrap_or("").to_string())
                .collect();
            let rows: Vec<Vec<Value>> = (0..grid.rows.min(SNAPSHOT_MAX_ROWS))
                .map(|row| {
                    (0..grid.cols.min(SNAPSHOT_MAX_COLUMNS))
                        .map(|col| {
                            let cell = grid.cell(row, col);
                            let truncated: String =
                                cell.chars().take(SNAPSHOT_MAX_CELL_CHARS).collect();
                            Value::String(truncated)
                        })
                        .collect()
                })
                .collect();
            Some(AiChatResultPreview { columns, rows })
        });
        let snapshot = AiChatContextSnapshot {
            editor_sql,
            selection,
            last_error: None,
            result_preview,
        };
        if has_snapshot_content(Some(&snapshot)) {
            Some(snapshot)
        } else {
            None
        }
    }
}
