//! Port of `packages/frontend/src/windows/settings/SettingHint.tsx`
//! (T381).
//!
//! Help text outside the settings layout, available on hover and
//! keyboard focus.
//!
//! ## What maps where
//!
//! - The interaction (show on hover/focus, a **120 ms** close grace
//!   that a move onto the tooltip itself cancels, focus retention —
//!   the timer does not close on a trigger that still holds focus —
//!   and the Escape dismissal) is [`SettingHintState`], the embeddable
//!   state machine; the host feeds the DOM events and the timer tick
//!   ([`SettingHintState::on_close_timeout`]).
//! - The portal positioning (`place()`'s above-first, flip-below,
//!   8pt-margin clamp against the viewport) is [`place_hint`], pure —
//!   the same geometry the kit's overlay anchoring applies to the
//!   built view (the React component hand-rolls what the kit
//!   provides; the arithmetic stays audited either way).
//! - The view is the established "?" mark + compact tooltip pair the
//!   settings sections already use ([`crate::ui::tooltip::Tooltip`],
//!   `Placement::Top`, the 320pt compact measure = `w-80`), assembled
//!   by [`SettingHint::build`]; the trigger carries the React
//!   `aria-label`.
//!
//! Known deltas: `useId`/`aria-describedby` are the DOM's
//! accessibility wiring; the Rune kit's tooltip owns its own
//! described-by plumbing, so no id mirror is carried. `createPortal`
//! is the kit's layer push. The `resize`/`scroll` re-listeners are
//! the host's overlay lifecycle (the kit re-anchors), not state.

use rune::{row, text, tok, El};
use rune_kit::{Kit, Placement};

use crate::ui::tooltip::Tooltip;

/// The close grace (`scheduleClose`'s `setTimeout(…, 120)`).
pub(crate) const SETTING_HINT_CLOSE_DELAY_MS: u32 = 120;

/// The viewport margin every clamp respects (the React `8`s).
pub(crate) const SETTING_HINT_MARGIN: f32 = 8.0;

/// `SettingHint({ label, text })`'s interaction state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SettingHintState {
    /// `open`: the tooltip is shown.
    open: bool,
    /// A close is scheduled (the 120 ms grace is running).
    pending_close: bool,
    /// The trigger holds keyboard focus (`document.activeElement ===
    /// triggerRef.current`).
    trigger_focused: bool,
}

impl SettingHintState {
    /// `show` (mouse enter / focus on the trigger): cancel any
    /// pending close and open.
    pub(crate) fn show(&mut self) {
        self.pending_close = false;
        self.open = true;
    }

    /// `cancelClose` (mouse enter on the tooltip itself): the grace
    /// stops.
    pub(crate) fn cancel_close(&mut self) {
        self.pending_close = false;
    }

    /// `scheduleClose` (mouse leave / blur): restart the grace.
    pub(crate) fn schedule_close(&mut self) {
        self.pending_close = false; // clearTimeout(current)
        self.pending_close = true;
    }

    /// The trigger gained focus (React `onFocus` → `show`).
    pub(crate) fn focus(&mut self) {
        self.trigger_focused = true;
        self.show();
    }

    /// The trigger lost focus (React `onBlur` → `scheduleClose`).
    pub(crate) fn blur(&mut self) {
        self.trigger_focused = false;
        self.schedule_close();
    }

    /// The timer fired: close unless the trigger still holds focus
    /// (the React callback's `activeElement` check — a focus retained
    /// through the grace keeps the tooltip open).
    pub(crate) fn on_close_timeout(&mut self) {
        if !self.pending_close {
            return;
        }
        self.pending_close = false;
        if !self.trigger_focused {
            self.open = false;
        }
    }

    /// The Escape dismissal (`dismiss`).
    pub(crate) fn escape(&mut self) {
        self.open = false;
        self.pending_close = false;
    }

    /// Whether the tooltip is shown.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

/// The trigger's on-screen rectangle (CSS pixels / points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HintRect {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

/// The tooltip's measured size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HintSize {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// `place()`: above the trigger first (`rect.top - height - 8`), the
/// 8pt-margin clamp left/right; below (`rect.bottom + 8`) when above
/// would leave the margin; the clamp again top/bottom.
pub(crate) fn place_hint(trigger: HintRect, tooltip: HintSize, viewport: HintSize) -> (f32, f32) {
    let m = SETTING_HINT_MARGIN;
    let above_top = trigger.top - tooltip.height - m;
    let left = trigger.left.max(m).min((viewport.width - tooltip.width - m).max(m));
    let top_candidate = if above_top >= m { above_top } else { trigger.bottom + m };
    let top = top_candidate.max(m).min((viewport.height - tooltip.height - m).max(m));
    (left, top)
}

/// The `SettingHint` view: the "?" trigger plus its tooltip.
pub(crate) struct SettingHint {
    key: String,
    label: String,
    text: String,
    open: bool,
}

impl SettingHint {
    /// `SettingHint({ label, text })` with the element key.
    pub(crate) fn new(key: impl Into<String>, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self { key: key.into(), label: label.into(), text: text.into(), open: false }
    }

    /// The open flag from [`SettingHintState`].
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Build the trigger row; the tooltip pushes into `layer` while
    /// open (the kit's portal).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mark = row()
            .key(format!("{}-mark", self.key))
            .size(16.0, 16.0)
            .center()
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(text("?").font(tok::type_::LABEL).color(k.c.text_muted));
        Tooltip::new(format!("{}-tooltip", self.key))
            .trigger(mark)
            .label(self.label)
            .text(self.text)
            .placement(Placement::Top)
            .open(self.open)
            .build(k, layer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_close_grace_is_120ms() {
        assert_eq!(SETTING_HINT_CLOSE_DELAY_MS, 120);
    }

    #[test]
    fn show_opens_and_a_move_onto_the_tooltip_cancels_the_close() {
        let mut state = SettingHintState::default();
        assert!(!state.is_open());
        state.show();
        assert!(state.is_open());
        // Leaving the trigger schedules, entering the tooltip cancels.
        state.schedule_close();
        state.cancel_close();
        state.on_close_timeout();
        assert!(state.is_open(), "the cancelled grace must not close");
    }

    #[test]
    fn the_grace_closes_after_the_timeout_when_focus_left() {
        let mut state = SettingHintState::default();
        state.show();
        state.schedule_close();
        state.on_close_timeout();
        assert!(!state.is_open());
    }

    #[test]
    fn a_retained_focus_keeps_the_tooltip_open_through_the_grace() {
        let mut state = SettingHintState::default();
        state.focus();
        assert!(state.is_open());
        // The mouse leaves, but the keyboard focus stays: the timer's
        // `activeElement` check keeps it open.
        state.schedule_close();
        state.on_close_timeout();
        assert!(state.is_open());
        // The blur that follows closes on its own grace.
        state.blur();
        state.on_close_timeout();
        assert!(!state.is_open());
    }

    #[test]
    fn focus_opens_and_a_late_show_reopens_after_a_close() {
        let mut state = SettingHintState::default();
        state.focus();
        assert!(state.is_open());
        state.blur();
        state.on_close_timeout();
        assert!(!state.is_open());
        state.show();
        assert!(state.is_open());
    }

    #[test]
    fn escape_dismisses_immediately() {
        let mut state = SettingHintState::default();
        state.show();
        state.escape();
        assert!(!state.is_open());
        // A pending grace does not resurrect it.
        state.on_close_timeout();
        assert!(!state.is_open());
    }

    #[test]
    fn a_timeout_without_a_scheduled_close_is_inert() {
        let mut state = SettingHintState::default();
        state.show();
        state.on_close_timeout();
        assert!(state.is_open(), "only a scheduled grace may close");
    }

    #[test]
    fn place_prefers_above_and_flips_below_only_when_above_would_clip() {
        let trigger = HintRect { left: 100.0, top: 60.0, bottom: 76.0 };
        let tooltip = HintSize { width: 320.0, height: 40.0 };
        let viewport = HintSize { width: 1440.0, height: 816.0 };
        // Above fits: top = 60 - 40 - 8 = 12.
        assert_eq!(place_hint(trigger, tooltip, viewport), (100.0, 12.0));

        // Above would clip (top < 8): below = bottom + 8.
        let trigger = HintRect { left: 100.0, top: 40.0, bottom: 56.0 };
        assert_eq!(place_hint(trigger, tooltip, viewport), (100.0, 64.0));
    }

    #[test]
    fn place_clamps_to_the_eight_point_margins() {
        let tooltip = HintSize { width: 320.0, height: 40.0 };
        // A trigger at the left edge clamps left to 8.
        let trigger = HintRect { left: 0.0, top: 100.0, bottom: 116.0 };
        let viewport = HintSize { width: 1440.0, height: 816.0 };
        let (left, _) = place_hint(trigger, tooltip, viewport);
        assert_eq!(left, 8.0);
        // A wide tooltip in a narrow window still keeps the margin.
        let viewport = HintSize { width: 200.0, height: 816.0 };
        let (left, _) = place_hint(trigger, tooltip, viewport);
        assert_eq!(left, 8.0);
        // A tooltip taller than the viewport clamps top to 8.
        let tooltip = HintSize { width: 320.0, height: 900.0 };
        let viewport = HintSize { width: 1440.0, height: 816.0 };
        let (_, top) = place_hint(trigger, tooltip, viewport);
        assert_eq!(top, 8.0);
        // The bottom clamp bites when the flip-below candidate
        // exceeds the viewport: a tall tooltip near the bottom flips
        // below (64) but clamps to height - tooltip - 8 (28).
        let trigger = HintRect { left: 100.0, top: 40.0, bottom: 56.0 };
        let tooltip = HintSize { width: 320.0, height: 780.0 };
        let viewport = HintSize { width: 1440.0, height: 816.0 };
        let (_, top) = place_hint(trigger, tooltip, viewport);
        assert_eq!(top, 816.0 - 780.0 - 8.0);
        // And when above simply fits, it stays (the 752 case: the
        // min keeps the smaller, in-viewport candidate).
        let trigger = HintRect { left: 100.0, top: 800.0, bottom: 816.0 };
        let tooltip = HintSize { width: 320.0, height: 40.0 };
        let (_, top) = place_hint(trigger, tooltip, viewport);
        assert_eq!(top, 800.0 - 40.0 - 8.0);
    }
}
