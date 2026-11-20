//! Port of `packages/frontend/src/windows/schema-diff/useSchemaDiffClipboardFeedback.ts`
//! (T372, riding with the table-names full port).
//!
//! The copy-button feedback state: one active kind (`summary` / `sql` /
//! `config`) that clears itself 2000ms after it was shown.
//!
//! ## What maps where
//!
//! - `useState<FeedbackKind>(null)` + the `setTimeout(() => setFeedback(null),
//!   2000)` become [`SchemaDiffClipboardFeedback::show`] arming a deadline
//!   and [`SchemaDiffClipboardFeedback::tick`] clearing it — the host turns
//!   the deadline into `Tx::wake_at` (the session_cancel pattern).
//! - The kind union (minus `null`, which is `Option::None`) is
//!   [`SchemaDiffFeedbackKind`], the canonical enum the execution panels
//!   re-export (T075 declared it locally first; this module is its single
//!   source now).
//!
//! ## Keys
//!
//! No DOM of its own (a hook); the copy sites own their keys.

/// React's `FeedbackKind` without the `null` arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffFeedbackKind {
    Summary,
    Sql,
    Config,
}

/// The 2000ms feedback window (`window.setTimeout(…, 2000)`).
pub(crate) const SCHEMA_DIFF_FEEDBACK_MS: i64 = 2_000;

/// The feedback state machine.
#[derive(Default)]
pub(crate) struct SchemaDiffClipboardFeedback {
    feedback: Option<SchemaDiffFeedbackKind>,
    clear_at_ms: Option<i64>,
}

impl SchemaDiffClipboardFeedback {
    /// `showFeedback(kind)`: set the kind and arm the 2000ms clear.
    pub(crate) fn show(&mut self, kind: SchemaDiffFeedbackKind, now_ms: i64) {
        self.feedback = Some(kind);
        self.clear_at_ms = Some(now_ms + SCHEMA_DIFF_FEEDBACK_MS);
    }

    /// The clock tick: a due deadline clears the feedback (the host calls
    /// this from its `wake_at`).
    pub(crate) fn tick(&mut self, now_ms: i64) {
        if self.clear_at_ms.is_some_and(|clear_at| now_ms >= clear_at) {
            self.feedback = None;
            self.clear_at_ms = None;
        }
    }

    /// The active kind (`null` is `None`).
    pub(crate) fn feedback(&self) -> Option<SchemaDiffFeedbackKind> {
        self.feedback
    }

    /// The clear deadline the host should `wake_at` (when armed).
    pub(crate) fn clear_at_ms(&self) -> Option<i64> {
        self.clear_at_ms
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn shows_a_kind_and_clears_after_the_window() {
        let mut feedback = SchemaDiffClipboardFeedback::default();
        assert_eq!(feedback.feedback(), None);
        assert_eq!(feedback.clear_at_ms(), None);

        feedback.show(SchemaDiffFeedbackKind::Summary, 1_000);
        assert_eq!(feedback.feedback(), Some(SchemaDiffFeedbackKind::Summary));
        assert_eq!(feedback.clear_at_ms(), Some(1_000 + SCHEMA_DIFF_FEEDBACK_MS));
        // Before the deadline: stays; at/after: clears.
        feedback.tick(2_999);
        assert_eq!(feedback.feedback(), Some(SchemaDiffFeedbackKind::Summary));
        feedback.tick(3_000);
        assert_eq!(feedback.feedback(), None);
        assert_eq!(feedback.clear_at_ms(), None);
        // A later tick is inert.
        feedback.tick(10_000);
        assert_eq!(feedback.feedback(), None);
    }

    #[test]
    fn a_new_show_replaces_the_window() {
        let mut feedback = SchemaDiffClipboardFeedback::default();
        feedback.show(SchemaDiffFeedbackKind::Sql, 0);
        feedback.show(SchemaDiffFeedbackKind::Config, 5_000);
        assert_eq!(feedback.feedback(), Some(SchemaDiffFeedbackKind::Config));
        assert_eq!(feedback.clear_at_ms(), Some(5_000 + SCHEMA_DIFF_FEEDBACK_MS));
        // The first window is gone.
        feedback.tick(2_000);
        assert_eq!(feedback.feedback(), Some(SchemaDiffFeedbackKind::Config));
    }
}
