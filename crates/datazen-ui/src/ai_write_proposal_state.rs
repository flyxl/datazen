//! Host sync for the write-proposal host (T012).
//!
//! [`AppState::sync_ai_write_proposals`] is called from
//! [`crate::app_poll`] once per frame and performs the one effect the host
//! owns today — React's 1s expiry interval
//! (`AiWriteProposalHost.tsx:108–130`), here a [`AiWriteProposalHostState::tick`]
//! with the wall clock. It returns `true` when the tick moved state, so the
//! poll loop knows whether to redraw.
//!
//! # What the host does NOT own yet
//!
//! The cross-window `ai:write-proposal-requested` / `ai:write-audit-error`
//! subscriptions and the `ai_apply_write` / `ai_deny_write` drain have no
//! Rune source yet (see the component module's docs): there is no event to
//! feed [`AiWriteProposalHostState::enqueue`] and no adapter method to drain
//! the apply / deny outboxes. The sync therefore deliberately does **not**
//! touch the queue, the receipt, or the outboxes — draining them would
//! destroy the history the wiring task reads.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::state_def::AppState;

impl AppState {
    /// The expiry tick for the write-proposal host (T012).
    pub(crate) fn sync_ai_write_proposals(&mut self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.ai_write_proposal.tick(now_ms)
    }
}
