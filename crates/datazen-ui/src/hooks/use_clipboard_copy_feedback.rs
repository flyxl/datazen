//! Port of `packages/frontend/src/hooks/useClipboardCopyFeedback.ts`.
//!
//! Request-bound "copy" feedback with the downgrade chain's guarantees
//! (DB-03 §3.5): optimistic feedback with rollback, every click owns its
//! whole window, and a late rejection cannot erase a newer success. The
//! actual write — `copyToClipboard`'s three-step chain
//! (`navigator.clipboard` → Tauri `write_clipboard` → failure) or the
//! rich two-flavor write — is host transport; this module is the
//! feedback machine the host drives.
//!
//! ## What maps where
//!
//! - `useState(copied)` + the `setTimeout` clear become the `copied` flag
//!   with a deadline: [`ClipboardCopyFeedback::copy`] arms
//!   `now + feedback_ms`, [`ClipboardCopyFeedback::tick`] clears it when
//!   due (the host turns the deadline into `Tx::wake_at`, the
//!   session_cancel pattern).
//! - `requestIdRef` is the monotonic request id; the rejection path only
//!   rolls back when the id is still the newest one
//!   ([`ClipboardCopyFeedback::on_copy_result`]) — an older failure
//!   leaves the newer window (and its timer) alone.
//! - Empty text **and** no html stays a failure with no state change:
//!   `copyToClipboard('')` returns `false` by contract, and lighting up
//!   "copied" for a write that put nothing on the clipboard would be a
//!   lie.
//! - The unmount cleanup (clear the timer) is [`ClipboardCopyFeedback::dispose`].
//!
//! Known deltas: none behavioural.

/// The hook's state: the copied flag, the newest request id, and the
/// clear deadline the host wakes at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipboardCopyFeedback {
    copied: bool,
    /// Monotonic id of the newest request; older ones are stale and must
    /// not roll back.
    request_id: u64,
    /// The `setTimeout` clear, as a deadline (epoch ms).
    clear_at_ms: Option<i64>,
}

/// What [`ClipboardCopyFeedback::copy`] asks the host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardCopyWrite {
    /// The plain write (`copyToClipboard(text)`).
    Plain { text: String },
    /// The rich write (`copyRichTextToClipboard(html, text)`): both
    /// flavors — `text/html` for targets that render tables and the plain
    /// text as the fallback everything else reads.
    Rich { html: String, text: String },
}

impl ClipboardCopyFeedback {
    /// `copied`: whether the last copy succeeded (the window is still
    /// live).
    pub fn copied(&self) -> bool {
        self.copied
    }

    /// The clear deadline, when armed (the host's `wake_at`).
    pub fn clear_at_ms(&self) -> Option<i64> {
        self.clear_at_ms
    }

    /// `copy(text, rich?)`: the click. Empty text with no html is an
    /// immediate failure with no state change; otherwise the feedback
    /// lights optimistically, the window (re)arms, and the host performs
    /// the returned write, reporting back through
    /// [`Self::on_copy_result`] with the returned request id.
    pub fn copy(
        &mut self,
        text: &str,
        rich_html: Option<&str>,
        now_ms: i64,
        feedback_ms: i64,
    ) -> Result<(u64, ClipboardCopyWrite), ()> {
        if text.is_empty() && rich_html.is_none_or(str::is_empty) {
            // The existing helper's semantics, kept: an empty copy is not
            // a success — and never lights up.
            return Err(());
        }
        self.request_id += 1;
        // Optimistic: the click has to read as instant. Rolled back by
        // on_copy_result if the write fails.
        self.copied = true;
        self.clear_at_ms = Some(now_ms + feedback_ms);
        let write = match rich_html.filter(|html| !html.is_empty()) {
            Some(html) => ClipboardCopyWrite::Rich { html: html.to_owned(), text: text.to_owned() },
            None => ClipboardCopyWrite::Plain { text: text.to_owned() },
        };
        Ok((self.request_id, write))
    }

    /// The write's outcome (`ok` is `Promise.resolve(false)`'s or the
    /// rejection's `catch(() => false)` collapsed). A failure rolls back
    /// only when it is still the newest request; a success keeps the
    /// window (the timer clears it).
    pub fn on_copy_result(&mut self, request_id: u64, ok: bool) {
        if !ok && request_id == self.request_id {
            self.copied = false;
            self.clear_at_ms = None;
        }
    }

    /// The clock tick: a due deadline clears the feedback (the host calls
    /// this from its `wake_at`).
    pub fn tick(&mut self, now_ms: i64) {
        if self.clear_at_ms.is_some_and(|clear_at| now_ms >= clear_at) {
            self.copied = false;
            self.clear_at_ms = None;
        }
    }

    /// The unmount cleanup: the timer never fires after the hook is gone.
    pub fn dispose(&mut self) {
        self.clear_at_ms = None;
        self.copied = false;
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn lights_up_optimistically_and_clears_at_the_window_edge() {
        let mut feedback = ClipboardCopyFeedback::default();
        assert!(!feedback.copied());

        let (id, write) = feedback.copy("SELECT 1", None, 1_000, 1_500).expect("arms");
        assert_eq!(id, 1);
        assert_eq!(write, ClipboardCopyWrite::Plain { text: "SELECT 1".to_owned() });
        assert!(feedback.copied());
        assert_eq!(feedback.clear_at_ms(), Some(2_500));

        // Not before the edge.
        feedback.tick(2_499);
        assert!(feedback.copied());
        feedback.tick(2_500);
        assert!(!feedback.copied());
        assert_eq!(feedback.clear_at_ms(), None);
        // The window owns the whole click: the success result changes
        // nothing (the timer is the only clear).
        let (id, _) = feedback.copy("SELECT 2", None, 3_000, 500).expect("arms");
        feedback.on_copy_result(id, true);
        assert!(feedback.copied());
        feedback.tick(3_500);
        assert!(!feedback.copied());
    }

    #[test]
    fn an_empty_copy_fails_without_lighting_up() {
        let mut feedback = ClipboardCopyFeedback::default();
        assert_eq!(feedback.copy("", None, 0, 1_000), Err(()));
        // Whitespace is still text: it copies (React trims nothing).
        assert!(feedback.copy("  ", None, 0, 1_000).is_ok());
        // An empty html with empty text is the same failure.
        let mut rich_only_empty = ClipboardCopyFeedback::default();
        assert_eq!(rich_only_empty.copy("", Some(""), 0, 1_000), Err(()));
        assert!(!rich_only_empty.copied());
        assert_eq!(rich_only_empty.clear_at_ms(), None);
    }

    #[test]
    fn a_late_rejection_of_an_older_copy_cannot_erase_a_newer_success() {
        let mut feedback = ClipboardCopyFeedback::default();
        let (first, _) = feedback.copy("older", None, 0, 10_000).expect("arms");
        let (second, _) = feedback.copy("newer", None, 1_000, 10_000).expect("arms");
        // The older write fails after the newer one armed: its rollback
        // is bound to its own request and must leave the newer window.
        feedback.on_copy_result(first, false);
        assert!(feedback.copied(), "the newer window survives");
        assert_eq!(feedback.clear_at_ms(), Some(11_000));
        // The newest failure still rolls back.
        feedback.on_copy_result(second, false);
        assert!(!feedback.copied());
        assert_eq!(feedback.clear_at_ms(), None);
    }

    #[test]
    fn a_rich_payload_takes_the_two_flavor_write() {
        let mut feedback = ClipboardCopyFeedback::default();
        let (_, write) =
            feedback.copy("a\tb", Some("<table><tr><td>a</td></tr></table>"), 0, 1_000)
                .expect("arms");
        assert_eq!(
            write,
            ClipboardCopyWrite::Rich {
                html: "<table><tr><td>a</td></tr></table>".to_owned(),
                text: "a\tb".to_owned(),
            }
        );
        // Empty html falls back to the plain write.
        let (_, plain) = feedback.copy("plain", Some(""), 0, 1_000).expect("arms");
        assert_eq!(plain, ClipboardCopyWrite::Plain { text: "plain".to_owned() });
    }

    #[test]
    fn every_click_rearms_its_own_window() {
        let mut feedback = ClipboardCopyFeedback::default();
        feedback.copy("one", None, 0, 1_000).expect("arms");
        assert_eq!(feedback.clear_at_ms(), Some(1_000));
        feedback.copy("two", None, 5_000, 1_000).expect("arms");
        assert_eq!(feedback.clear_at_ms(), Some(6_000));
        // The first window is gone: only the newest deadline stands.
        feedback.tick(1_500);
        assert!(feedback.copied());
        feedback.tick(6_000);
        assert!(!feedback.copied());
    }

    #[test]
    fn dispose_stops_the_window() {
        let mut feedback = ClipboardCopyFeedback::default();
        feedback.copy("x", None, 0, 60_000).expect("arms");
        feedback.dispose();
        assert!(!feedback.copied());
        assert_eq!(feedback.clear_at_ms(), None);
        // A late timer wake is inert.
        feedback.tick(120_000);
        assert!(!feedback.copied());
    }
}
