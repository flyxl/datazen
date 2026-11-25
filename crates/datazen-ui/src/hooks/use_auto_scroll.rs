//! Port of `packages/frontend/src/hooks/useAutoScroll.ts`.
//!
//! Tracks whether a scrollable container is pinned to the bottom, counts
//! new messages that arrived while the user was scrolled up, and provides
//! the "jump to bottom" action. React attaches `onScroll`/`containerRef`
//! to a DOM div; here the metrics arrive as arguments (the host owns the
//! scroll surface) and the state machine is embeddable in `S`.
//!
//! ## What maps where
//!
//! - `checkAtBottom` reads `scrollHeight - scrollTop - clientHeight` and
//!   compares against the threshold (default **120**): that arithmetic is
//!   [`AutoScrollState::on_scroll`], which also resets the unread count
//!   when the container is back at the bottom.
//! - `jumpToBottom` sets `atBottom` and clears the unread count — the
//!   actual `scrollTop = scrollHeight` write is the host's scroll command
//!   (Rune has no DOM); [`AutoScrollState::jump_to_bottom`] returns the
//!   request.
//! - `useTrackUnread` (the separate exported machine the consuming
//!   component drives off `messages.length`) is [`TrackUnread`]: a count
//!   growth adds the delta to its own unread while the container is not
//!   at the bottom, resets it while it is, and a flip back to bottom
//!   resets it (its second effect). Its `prevRef` is carried state, not
//!   an effect closure.
//!
//! Known deltas: none behavioural — every threshold, reset and delta rule
//! is the React file's verbatim.

/// The hook's default distance-from-bottom threshold (px).
pub const AUTO_SCROLL_THRESHOLD_DEFAULT: i64 = 120;

/// `useAutoScroll`'s state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoScrollState {
    threshold: i64,
    /// Whether the container is at (or near) the bottom.
    at_bottom: bool,
    /// New messages since the user scrolled up.
    unread_count: usize,
}

impl Default for AutoScrollState {
    fn default() -> Self {
        Self { threshold: AUTO_SCROLL_THRESHOLD_DEFAULT, at_bottom: true, unread_count: 0 }
    }
}

impl AutoScrollState {
    /// `useAutoScroll({ threshold })` — the hook's initial state.
    pub fn new(threshold: i64) -> Self {
        Self { threshold, ..Self::default() }
    }

    /// Whether the scroll container is at (or near) the bottom.
    pub fn at_bottom(&self) -> bool {
        self.at_bottom
    }

    /// Messages that arrived while the user was scrolled up.
    pub fn unread_count(&self) -> usize {
        self.unread_count
    }

    /// `onScroll` / `checkAtBottom`: the container's live metrics. At (or
    /// within the threshold of) the bottom re-pins and clears the unread
    /// count.
    pub fn on_scroll(&mut self, scroll_height: i64, scroll_top: i64, client_height: i64) {
        let distance_from_bottom = scroll_height - scroll_top - client_height;
        let is_at_bottom = distance_from_bottom <= self.threshold;
        self.at_bottom = is_at_bottom;
        if is_at_bottom {
            self.unread_count = 0;
        }
    }

    /// `jumpToBottom`: pin to the bottom and reset the unread count. The
    /// host performs the scroll itself (Rune owns the surface).
    pub fn jump_to_bottom(&mut self) {
        self.at_bottom = true;
        self.unread_count = 0;
    }

    /// The container grew: one more message arrived while the user is not
    /// at the bottom (the hook's own counter — the sibling
    /// [`TrackUnread`] is `useTrackUnread`'s separate machine).
    pub fn note_message_arrived(&mut self) {
        if !self.at_bottom {
            self.unread_count += 1;
        }
    }
}

/// `useTrackUnread(messageCount, atBottom)`: its own unread counter,
/// driven by message-count changes — `prevRef` + the two effects as one
/// step function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackUnread {
    prev_count: usize,
    unread: usize,
}

impl TrackUnread {
    /// The hook's initial state (prev 0, unread 0).
    pub fn new() -> Self {
        Self::default()
    }

    /// The tracked unread count.
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// One render's effect run: a grown count adds the delta unless the
    /// container is at the bottom (where it resets), and a flip to bottom
    /// resets the counter — both effects of the React hook, folded into
    /// the step the host calls when `messages.length` or `atBottom`
    /// changes.
    pub fn step(&mut self, message_count: usize, at_bottom: bool) {
        if message_count > self.prev_count {
            let delta = message_count - self.prev_count;
            if at_bottom {
                self.unread = 0;
            } else {
                self.unread += delta;
            }
        }
        self.prev_count = message_count;
        if at_bottom {
            self.unread = 0;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn starts_pinned_with_no_unread() {
        let state = AutoScrollState::default();
        assert!(state.at_bottom());
        assert_eq!(state.unread_count(), 0);
        assert_eq!(AutoScrollState::default().threshold, 120);
    }

    #[test]
    fn scroll_position_decides_at_bottom() {
        // The React test's mock container: 1000/900/100 → distance 0.
        let mut state = AutoScrollState::new(50);
        state.on_scroll(1000, 900, 100);
        assert!(state.at_bottom());
        // 1000/500/100 → distance 400, beyond the 50px threshold.
        state.on_scroll(1000, 500, 100);
        assert!(!state.at_bottom());
        // Within the threshold still counts (distance 50 == threshold).
        state.on_scroll(1000, 850, 100);
        assert!(state.at_bottom(), "distance == threshold is at bottom");
    }

    #[test]
    fn jump_to_bottom_resets_state() {
        let mut state = AutoScrollState::new(50);
        state.on_scroll(1000, 500, 100);
        assert!(!state.at_bottom());
        state.note_message_arrived();
        state.note_message_arrived();
        assert_eq!(state.unread_count(), 2);
        state.jump_to_bottom();
        assert!(state.at_bottom());
        assert_eq!(state.unread_count(), 0);
    }

    #[test]
    fn arriving_back_at_the_bottom_clears_the_unread() {
        let mut state = AutoScrollState::default();
        state.on_scroll(1000, 500, 100);
        state.note_message_arrived();
        assert_eq!(state.unread_count(), 1);
        state.on_scroll(1000, 900, 100);
        assert!(state.at_bottom());
        assert_eq!(state.unread_count(), 0, "checkAtBottom clears on re-pin");
    }

    #[test]
    fn track_unread_accumulates_deltas_while_scrolled_up() {
        let mut tracker = TrackUnread::new();
        // prevRef starts at 0: the first count is itself a growth (React's
        // mount run adds it when the container is scrolled up).
        tracker.step(5, false);
        assert_eq!(tracker.unread(), 5);
        tracker.step(8, false);
        assert_eq!(tracker.unread(), 8);
        tracker.step(9, false);
        assert_eq!(tracker.unread(), 9);
        // Shrinking counts are not negative deltas.
        tracker.step(2, false);
        assert_eq!(tracker.unread(), 9);
        // At the bottom: resets (both the growth effect and the flip).
        tracker.step(10, true);
        assert_eq!(tracker.unread(), 0);
        // And stays cleared while pinned.
        tracker.step(11, true);
        assert_eq!(tracker.unread(), 0);
        // Scrolled up again with no new messages: nothing accumulates.
        tracker.step(11, false);
        assert_eq!(tracker.unread(), 0);
        tracker.step(15, false);
        assert_eq!(tracker.unread(), 4);
        // A flip to bottom without growth still resets.
        tracker.step(15, true);
        assert_eq!(tracker.unread(), 0);
    }
}
