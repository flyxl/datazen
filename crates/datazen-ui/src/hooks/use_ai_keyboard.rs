//! Port of `packages/frontend/src/hooks/useAiKeyboard.ts`.
//!
//! Shared keyboard + IME composition state for AI input fields: Enter
//! submits, Shift+Enter inserts a newline, IME composition is respected.
//! React spreads the return onto `<textarea>`; here the state machine is
//! [`AiKeyboardInput`], embedded in the owning state `S`, and the host
//! feeds Rune's key events through [`AiKeyboardInput::on_key`].
//!
//! ## What maps where
//!
//! - `composingRef` + `compositionstart`/`compositionend` become the
//!   `composing` flag with [`AiKeyboardInput::composition_start`] /
//!   [`AiKeyboardInput::composition_end`].
//! - `compositionend`'s `requestAnimationFrame` clear — Chromium fires
//!   `compositionend` BEFORE the final keydown, WebKit/Firefox AFTER, and
//!   the rAF covers both — becomes [`AiKeyboardInput::end_of_frame`]: the
//!   flag clears only there, so a keydown racing the composition end (the
//!   Chromium order) still reads `composing == true`, exactly as React's
//!   pending rAF leaves the ref set through that keydown.
//! - `onKeyDown`'s four-part guard (`Enter` && !shift && !composingRef &&
//!   !`nativeEvent.isComposing`) is [`AiKeyboardInput::on_key`], returning
//!   whether the host must submit (and swallow the key — React's
//!   `preventDefault`); every other key returns `None`.
//!
//! Known deltas: none behavioural.

/// The hook's state: one composition flag (the rAF-clear window included).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AiKeyboardInput {
    composing: bool,
    /// `compositionend` fired; the clear is pending the frame boundary.
    clear_pending: bool,
}

impl AiKeyboardInput {
    /// `compositionstart`.
    pub(crate) fn composition_start(&mut self) {
        self.composing = true;
        self.clear_pending = false;
    }

    /// `compositionend` — the flag stays set until the frame boundary
    /// (React's `requestAnimationFrame` clear).
    pub(crate) fn composition_end(&mut self) {
        self.clear_pending = true;
    }

    /// The frame boundary: a pending composition clear lands here, after
    /// any keydown that raced the composition end.
    pub(crate) fn end_of_frame(&mut self) {
        if self.clear_pending {
            self.composing = false;
            self.clear_pending = false;
        }
    }

    /// `onKeyDown`: `Some(())` when the host must run `onSubmit` (and
    /// swallow the Enter); `None` for Shift+Enter (the newline), any
    /// composition-time Enter (both the tracked flag and the native
    /// `isComposing` guard), and every other key.
    pub(crate) fn on_key(&self, key: &rune::Key, shift: bool, native_is_composing: bool) -> Option<()> {
        if !matches!(key, rune::Key::Enter) {
            return None;
        }
        if shift || self.composing || native_is_composing {
            return None;
        }
        Some(())
    }

    /// Whether a composition is in flight (or its clear is pending — the
    /// rAF window still counts, like React's still-set ref).
    pub(crate) fn is_composing(&self) -> bool {
        self.composing
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn plain_enter_submits_and_shift_enter_does_not() {
        let input = AiKeyboardInput::default();
        assert_eq!(input.on_key(&rune::Key::Enter, false, false), Some(()));
        assert_eq!(input.on_key(&rune::Key::Enter, true, false), None, "Shift+Enter is a newline");
        assert_eq!(input.on_key(&rune::Key::Text("a".to_owned()), false, false), None);
        assert_eq!(input.on_key(&rune::Key::Escape, false, false), None);
    }

    #[test]
    fn composition_time_enter_never_submits() {
        let mut input = AiKeyboardInput::default();
        input.composition_start();
        assert!(input.is_composing());
        assert_eq!(input.on_key(&rune::Key::Enter, false, false), None);
        // The native isComposing guard, independently tracked.
        assert_eq!(
            AiKeyboardInput::default().on_key(&rune::Key::Enter, false, true),
            None
        );
    }

    #[test]
    fn the_composition_clear_lands_at_the_frame_boundary() {
        let mut input = AiKeyboardInput::default();
        input.composition_start();
        // Chromium order: compositionend BEFORE the final keydown — the
        // keydown still reads composing (the pending rAF has not run).
        input.composition_end();
        assert!(input.is_composing());
        assert_eq!(input.on_key(&rune::Key::Enter, false, false), None);
        // The frame boundary clears; the next plain Enter submits.
        input.end_of_frame();
        assert!(!input.is_composing());
        assert_eq!(input.on_key(&rune::Key::Enter, false, false), Some(()));
        // WebKit/Firefox order: keydown first, compositionend after —
        // same outcome, the flag is set through both.
        input.composition_start();
        assert_eq!(input.on_key(&rune::Key::Enter, false, false), None);
        input.composition_end();
        input.end_of_frame();
        assert!(!input.is_composing());
    }

    #[test]
    fn a_new_composition_cancels_a_pending_clear() {
        let mut input = AiKeyboardInput::default();
        input.composition_start();
        input.composition_end();
        // A new composition starts before the frame boundary: the pending
        // clear is superseded (composition_start resets it, like React's
        // ref being set back to true before the rAF runs).
        input.composition_start();
        input.end_of_frame();
        assert!(input.is_composing(), "the second composition is still live");
    }
}
