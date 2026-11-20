//! Port of `packages/frontend/src/hooks/useFkNavigationShortcuts.ts`
//! (T275) — the keyboard of the foreign-key navigation (DB-07).
//!
//! `⌘/Ctrl+⌥+←` goes back, `⌘/Ctrl+⌥+→` goes forward. The handler is
//! bound to the **grid container**, never to the window (a global
//! listener would fight the SQL editor and every text field); in the
//! engine the grid's key path is the host's focus routing — this
//! module is the decision, [`FkNavigationShortcut::decide`].
//!
//! ## What maps where
//!
//! - The two guards are the `lib/gridEventGuards.ts` primitives,
//!   carried (segment 2 owns that file): an **IME composition** owns
//!   every key (a Chinese input mid-word must not navigate away; the
//!   keyCode-229 fallback included), and an **editable target** owns
//!   its keys (an open cell editor must not lose the arrows). The
//!   editable check is the DOM walk's decision as a boolean — the host
//!   answers "did the key land in an editor".
//! - The chord: mod (⌘ OR Ctrl) AND alt, then Left/Right. Anything
//!   else is not the navigation's key.
//! - When the navigation is disabled, the guards fail, or the chord
//!   does not match, the key is **left alone** — no preventDefault, no
//!   callback ([`FkNavigationShortcut::LeaveAlone`]). Swallowing a
//!   shortcut that did nothing is worse than letting the system have
//!   it.
//!
//! Known deltas: none behavioural.

/// The guard inputs (the `gridEventGuards` pair, as booleans the host
/// resolves: the composition state of the key event, and whether the
/// key landed inside an editor — an input, textarea, select,
/// contenteditable or the grid's open cell editor).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FkKeyContext {
    /// `isComposingEvent` (the IME composition, keyCode 229 included).
    pub composing: bool,
    /// `isEditableTarget` (the key landed in an editor).
    pub editable_target: bool,
    /// The mod key: ⌘ or Ctrl.
    pub mod_key: bool,
    /// Option/Alt.
    pub alt_key: bool,
    /// The arrow (None: some other key).
    pub arrow: Option<FkArrow>,
}

/// The two arrows the navigation claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FkArrow {
    Left,
    Right,
}

/// What the handler decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FkNavigationShortcut {
    /// Consume the key (the React `preventDefault`) and go back one
    /// jump.
    Back,
    /// Consume and go forward one jump.
    Forward,
    /// The key is not the navigation's: leave it alone — no
    /// preventDefault, no error, the system keeps the key.
    LeaveAlone,
}

/// `useFkNavigationShortcuts`'s handler, as the decision: `enabled`
/// gates whole (a tab that was never reached by a jump has no stack);
/// the composition and editable guards own their keys; the chord is
/// mod+alt+Left/Right.
pub(crate) fn decide_fk_navigation_shortcut(
    enabled: bool,
    context: &FkKeyContext,
) -> FkNavigationShortcut {
    if !enabled {
        return FkNavigationShortcut::LeaveAlone;
    }
    // An IME composition owns every key.
    if context.composing {
        return FkNavigationShortcut::LeaveAlone;
    }
    // An editable target owns its keys.
    if context.editable_target {
        return FkNavigationShortcut::LeaveAlone;
    }
    // The chord: mod AND alt, then the arrows.
    if !context.mod_key || !context.alt_key {
        return FkNavigationShortcut::LeaveAlone;
    }
    match context.arrow {
        Some(FkArrow::Left) => FkNavigationShortcut::Back,
        Some(FkArrow::Right) => FkNavigationShortcut::Forward,
        None => FkNavigationShortcut::LeaveAlone,
    }
}
