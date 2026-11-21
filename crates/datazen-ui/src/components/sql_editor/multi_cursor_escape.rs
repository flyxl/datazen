//! Port of `components/sql-editor/paste/multiCursorEscape.ts` (T283,
//! split) — the dedicated Escape that leaves the multi-cursor state.
//!
//! ## Why a dedicated command instead of `simplifySelection`
//!
//! `simplifySelection` reduces the range count but keeps the surviving
//! range **as it is** — after a `Mod-d` chain (every range a selected
//! word) Escape leaves a still-highlighted word behind, and the exit
//! state is ambiguous. This command collapses to an EMPTY cursor
//! instead: exactly one bare cursor at the main range's head, dropping
//! any rectangular state — and a second Escape is a defined no-op.
//!
//! ## The state machine (the React file's table, verbatim)
//!
//! | state | Escape's owner |
//! |---|---|
//! | S0 one cursor (empty or not) | `simplifySelection` — this command returns false |
//! | S1 two or more cursors | THIS command: collapse to one empty cursor at the main head |
//! | S2 autocomplete open | `closeCompletion` (Prec.highest, above this) |
//! | S3 snippet session open | `clearSnippet` (Prec.highest, same shape) |
//! | S4 search panel focused | `closeSearchPanel` (scoped to the panel) |
//!
//! S2–S4 are the keymap precedence layer (T320's keymap task and
//! T335's registry); this module owns S1's decision and S0's refusal.
//! There is no zero-cursor state (a selection always holds at least one
//! range), so "nothing to exit" is exactly S0.
//!
//! Known deltas: none behavioural — the decision is the React
//! command's verbatim.

/// What `exitMultiCursor` decided for one Escape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EscapeDecision {
    /// Nothing to exit (one range): the command declines (`false`) and
    /// `simplifySelection` decides — a non-empty range becomes a
    /// cursor, an empty one is a no-op.
    Decline,
    /// Two or more ranges: collapse to exactly one bare cursor at the
    /// main range's head (the returned offset), dropping any
    /// rectangular state the selection carried.
    CollapseToMainHead { head: usize },
}

/// `exitMultiCursor`'s decision: `ranges.len() <= 1` declines; otherwise
/// the main range's head becomes the single empty cursor (the user's
/// last-moved range, so a Mod-d chain does not keep its highlight).
pub(crate) fn exit_multi_cursor_decision(
    range_count: usize,
    main_head: usize,
) -> EscapeDecision {
    if range_count <= 1 {
        return EscapeDecision::Decline;
    }
    EscapeDecision::CollapseToMainHead { head: main_head }
}
