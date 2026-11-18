//! `AiWriteProposalHost` — T012.
//!
//! Ported from `packages/frontend/src/components/AiWriteProposalHost.tsx`
//! (the global host for T24 confirmed writes): the agent never applies a
//! write itself — `propose_write` emits `ai:write-proposal-requested`, this
//! dialog shows the exact SQL, the optional review diff and the static
//! impact, and only "Allow once" applies the write with the exact proposal
//! id, connection, database and SQL. Deny removes the proposal; expiry
//! disables the allow path and auto-denies.
//!
//! | React source | Rune item |
//! |---|---|
//! | `AiWriteProposal` / `AiWriteImpact` / `AiWriteStatement` / `AiWriteApplyReceipt` (`types/ai.ts:204–256`) | [`AiWriteProposal`] etc. below |
//! | `queue` / `current` (`AiWriteProposalHost.tsx:20–28`) | [`AiWriteProposalHostState::queue`] / [`AiWriteProposalHostState::current`] |
//! | `ai:write-proposal-requested` listener (36–48) | [`AiWriteProposalHostState::enqueue`] — the event edge lives outside this module (see below) |
//! | `ai:write-audit-error` listener (49–51) | [`AiWriteProposalHostState::set_audit_error`] |
//! | `deny` / `allow` (68–106) | [`AiWriteProposalHostState::deny`] / [`AiWriteProposalHostState::allow`] |
//! | expiry interval (108–130) | [`AiWriteProposalHostState::tick`] — driven once per frame by `AppState::sync_ai_write_proposals` |
//! | dialog / receipt / audit-error JSX (132–270) | `ai_write_proposal_host_view::AiWriteProposalHost::build` |
//!
//! # Deliberate boundaries (VM placeholder, like the T009 slices)
//!
//! * **No `ai_apply_write` / `ai_deny_write` adapter call yet.** The
//!   `datazen-ui-api` adapter has no such methods (the `aiCommands`
//!   IPC shell in `commands/ai.ts:281–287` has no Rune counterpart), so
//!   `allow()` / `deny()` record their exact request in the
//!   [`AiWriteProposalHostState::apply_requests`] /
//!   [`AiWriteProposalHostState::deny_requests`] outboxes and hold
//!   `responding` until the host answers through
//!   [`AiWriteProposalHostState::apply_succeeded`] /
//!   [`AiWriteProposalHostState::deny_succeeded`] (or
//!   `apply_failed` / `deny_failed`, which surfaces the error exactly as
//!   React's `catch` sets it — the host passes
//!   `t("ai.write.respondFailed")` for a transport failure). Draining the
//!   outboxes belongs to the task that wires the write commands; draining
//!   them early would destroy the history that task reads.
//! * **No cross-window event subscription.** Rune has no equivalent of
//!   `events.listenCrossWindow('ai:write-proposal-requested' | 'ai:write-audit-error')`
//!   yet, and the host only renders on the main window in React
//!   (`getWindowKind() !== 'main'` returns early). The edge that receives
//!   those events feeds this state through [`AiWriteProposalHostState::enqueue`]
//!   and [`AiWriteProposalHostState::set_audit_error`] (passing
//!   `t("ai.write.auditFailed")` when the payload is not a string, as
//!   React does); the host's `setWindowKind` gate travels with that edge.
//! * **Expiry is a poll tick, not an interval.** React's 1s `setInterval`
//!   becomes [`AiWriteProposalHostState::tick`], called from
//!   [`crate::app_poll`] on every frame with the wall clock. The
//!   auto-deny on expiry records a deny request exactly once per proposal
//!   (the `timed_out` set, mirroring React's `timedOutRef`).
//!
//! The view half lives in `ai_write_proposal_host_view` (dialog / receipt /
//! audit-error render, key convention, panel-height approximation).

// The production consumer is the `modal_layer` mount in `views/modals.rs`
// plus the `AppState::sync_ai_write_proposals` poll tick; the `aiCommands`
// outbox drain still has no caller.
#![allow(dead_code)]

use std::collections::HashSet;

/// One statement inside a write proposal (`types/ai.ts:204–212`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiWriteStatement {
    pub index: u32,
    pub verb: String,
    pub table: Option<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub sql_hash: String,
}

impl AiWriteStatement {
    pub(crate) fn new(index: u32, verb: impl Into<String>, sql_hash: impl Into<String>) -> Self {
        Self {
            index,
            verb: verb.into(),
            table: None,
            has_where: false,
            destructive: false,
            sql_hash: sql_hash.into(),
        }
    }
}

/// Static impact preview; no statement is executed to build it
/// (`types/ai.ts:214–223`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiWriteImpact {
    pub statement_count: u32,
    pub tables: Vec<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub writes_data: bool,
    pub schema_changes: bool,
    pub summary: String,
}

impl AiWriteImpact {
    pub(crate) fn new(summary: impl Into<String>) -> Self {
        Self {
            statement_count: 0,
            tables: Vec::new(),
            has_where: false,
            destructive: false,
            writes_data: false,
            schema_changes: false,
            summary: summary.into(),
        }
    }
}

/// A reviewable exact-text write proposal (`types/ai.ts:228–245`).
/// `proposal_id` doubles as the one-time apply token; the UI must send it
/// back with the exact SQL and target. `token` is carried for parity and
/// never displayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiWriteProposal {
    pub proposal_id: String,
    pub token: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    pub sql_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub diff: Option<String>,
    pub statements: Vec<AiWriteStatement>,
    pub impact: AiWriteImpact,
    pub production_protected: bool,
    pub apply_allowed: bool,
}

impl AiWriteProposal {
    pub(crate) fn new(
        proposal_id: impl Into<String>,
        connection_id: impl Into<String>,
        sql: impl Into<String>,
        expires_at: impl Into<String>,
    ) -> Self {
        Self {
            proposal_id: proposal_id.into(),
            token: String::new(),
            connection_id: connection_id.into(),
            database: String::new(),
            sql: sql.into(),
            sql_hash: String::new(),
            created_at: String::new(),
            expires_at: expires_at.into(),
            diff: None,
            statements: Vec::new(),
            impact: AiWriteImpact::new(String::new()),
            production_protected: false,
            apply_allowed: true,
        }
    }
}

/// Result of one successful apply (`types/ai.ts:248–255`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AiWriteApplyReceipt {
    pub proposal_id: String,
    pub connection_id: String,
    pub database: String,
    pub sql_hash: String,
    pub row_count: Option<i64>,
    pub result: serde_json::Value,
}

impl AiWriteApplyReceipt {
    pub(crate) fn new(proposal_id: impl Into<String>) -> Self {
        Self {
            proposal_id: proposal_id.into(),
            connection_id: String::new(),
            database: String::new(),
            sql_hash: String::new(),
            row_count: None,
            result: serde_json::Value::Null,
        }
    }
}

/// One recorded `allow()` — the exact fields React passes to
/// `aiCommands.aiApplyWrite` (`AiWriteProposalHost.tsx:88–93`), held for
/// the host drain (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiWriteApplyRequest {
    pub proposal_id: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
}

/// `expiresAt` is RFC3339 from the backend (`write_proposal.rs` emits
/// `to_rfc3339_opts(Secs, true)`); anything else parses as expired, exactly
/// as React's `Date.parse` returning `NaN` yields `remainingMs = 0`.
fn parse_expires_ms(expires_at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// The host's state: React's five `useState`s plus the `timedOutRef` set
/// (`AiWriteProposalHost.tsx:20–27`).
#[derive(Clone, Debug, Default)]
pub(crate) struct AiWriteProposalHostState {
    queue: Vec<AiWriteProposal>,
    remaining_ms: u64,
    responding: bool,
    error: Option<String>,
    receipt: Option<AiWriteApplyReceipt>,
    audit_error: Option<String>,
    timed_out: HashSet<String>,
    apply_requests: Vec<AiWriteApplyRequest>,
    deny_requests: Vec<String>,
    /// Bumped when a new proposal becomes current, so a reopened dialog
    /// scrolls to the top (the `Dialog::open_epoch` pattern).
    open_epoch: u64,
}

impl AiWriteProposalHostState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The proposal on screen (`queue[0]`), if any.
    pub(crate) fn current(&self) -> Option<&AiWriteProposal> {
        self.queue.first()
    }

    pub(crate) fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub(crate) fn remaining_ms(&self) -> u64 {
        self.remaining_ms
    }

    /// `Math.ceil(remainingMs / 1000)` (`AiWriteProposalHost.tsx:132`).
    pub(crate) fn remaining_seconds(&self) -> u64 {
        self.remaining_ms.div_ceil(1000)
    }

    /// `remainingMs <= 0` (`AiWriteProposalHost.tsx:133`).
    pub(crate) fn expired(&self) -> bool {
        self.remaining_ms == 0
    }

    pub(crate) fn responding(&self) -> bool {
        self.responding
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn receipt(&self) -> Option<&AiWriteApplyReceipt> {
        self.receipt.as_ref()
    }

    pub(crate) fn audit_error(&self) -> Option<&str> {
        self.audit_error.as_deref()
    }

    pub(crate) fn apply_requests(&self) -> &[AiWriteApplyRequest] {
        &self.apply_requests
    }

    pub(crate) fn deny_requests(&self) -> &[String] {
        &self.deny_requests
    }

    pub(crate) fn open_epoch(&self) -> u64 {
        self.open_epoch
    }

    /// Whether anything renders at all (`!current && !receipt &&
    /// !auditError` returns `null` in React, `AiWriteProposalHost.tsx:134`).
    pub(crate) fn is_visible(&self) -> bool {
        self.current().is_some() || self.receipt.is_some() || self.audit_error.is_some()
    }

    /// The `ai:write-proposal-requested` handler
    /// (`AiWriteProposalHost.tsx:38–47`): proposals without an id or
    /// without SQL are ignored, and a duplicate id never queues twice.
    /// Returns `true` when the proposal was queued.
    pub(crate) fn enqueue(&mut self, proposal: AiWriteProposal) -> bool {
        if proposal.proposal_id.is_empty() || proposal.sql.is_empty() {
            return false;
        }
        if self
            .queue
            .iter()
            .any(|item| item.proposal_id == proposal.proposal_id)
        {
            return false;
        }
        let becomes_current = self.queue.is_empty();
        self.queue.push(proposal);
        if becomes_current {
            self.open_epoch += 1;
        }
        true
    }

    /// The `ai:write-audit-error` handler (`AiWriteProposalHost.tsx:49–51`).
    /// The caller passes the string payload, or `t("ai.write.auditFailed")`
    /// when the payload is not a string.
    pub(crate) fn set_audit_error(&mut self, message: impl Into<String>) {
        self.audit_error = Some(message.into());
    }

    pub(crate) fn dismiss_receipt(&mut self) {
        self.receipt = None;
    }

    pub(crate) fn dismiss_audit_error(&mut self) {
        self.audit_error = None;
    }

    fn drop_by_id(&mut self, proposal_id: &str) {
        self.queue.retain(|item| item.proposal_id != proposal_id);
    }

    /// React's `deny` (`AiWriteProposalHost.tsx:68–81`): records the exact
    /// deny request and holds `responding` until the host answers (see the
    /// module docs). A deny with no current proposal, or while a response
    /// is already in flight (React's button is disabled then), is a no-op.
    pub(crate) fn deny(&mut self) {
        let Some(current) = self.queue.first() else {
            return;
        };
        if self.responding {
            return;
        }
        self.responding = true;
        self.error = None;
        self.deny_requests.push(current.proposal_id.clone());
    }

    /// React's `allow` (`AiWriteProposalHost.tsx:83–106`): records the exact
    /// apply request and holds `responding` until the host answers. The
    /// guards mirror the button's `disabled` (`responding || expired ||
    /// !applyAllowed`) plus React's early return on `!applyAllowed`.
    pub(crate) fn allow(&mut self) {
        let Some(current) = self.queue.first() else {
            return;
        };
        if self.responding || self.expired() || !current.apply_allowed {
            return;
        }
        self.responding = true;
        self.error = None;
        self.apply_requests.push(AiWriteApplyRequest {
            proposal_id: current.proposal_id.clone(),
            connection_id: current.connection_id.clone(),
            database: current.database.clone(),
            sql: current.sql.clone(),
        });
    }

    /// The host's answer to a recorded apply request: shows the receipt
    /// and drops the proposal, as React does after `aiApplyWrite` resolves
    /// (`AiWriteProposalHost.tsx:94–96`).
    pub(crate) fn apply_succeeded(&mut self, receipt: AiWriteApplyReceipt) {
        self.drop_by_id(&receipt.proposal_id);
        self.receipt = Some(receipt);
        self.responding = false;
    }

    /// The host's answer to a recorded apply request that failed: the
    /// error stays on the dialog and the proposal stays queued, as React's
    /// `catch` does (`AiWriteProposalHost.tsx:97–100`).
    pub(crate) fn apply_failed(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.responding = false;
    }

    /// The host's answer to a recorded deny request: drops the proposal,
    /// as React does after `aiDenyWrite` resolves
    /// (`AiWriteProposalHost.tsx:74–75`).
    pub(crate) fn deny_succeeded(&mut self, proposal_id: &str) {
        self.drop_by_id(proposal_id);
        self.responding = false;
    }

    /// The host's answer to a recorded deny request that failed: the error
    /// stays on the dialog and the proposal stays queued
    /// (`AiWriteProposalHost.tsx:76–79`).
    pub(crate) fn deny_failed(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.responding = false;
    }

    /// React's expiry effect (`AiWriteProposalHost.tsx:108–130`): recompute
    /// the remaining time from `expiresAt`, and auto-deny once when it
    /// reaches zero. Returns `true` when the remaining time changed or a
    /// deny was recorded, so the poll loop knows whether to redraw.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let Some(current) = self.queue.first() else {
            if self.remaining_ms != 0 {
                self.remaining_ms = 0;
                return true;
            }
            return false;
        };
        let expires_ms = parse_expires_ms(&current.expires_at).unwrap_or(0);
        let next = expires_ms.saturating_sub(now_ms as i64).max(0) as u64;
        let mut dirty = next != self.remaining_ms;
        self.remaining_ms = next;
        if next == 0 && !self.timed_out.contains(&current.proposal_id) {
            self.timed_out.insert(current.proposal_id.clone());
            self.deny();
            dirty = true;
        }
        dirty
    }
}
