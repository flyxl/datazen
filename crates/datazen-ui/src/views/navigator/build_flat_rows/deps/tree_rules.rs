//! The two tree rules from `@datazen/ui`'s `tree/geometry.ts` and
//! `tree/navigation.ts`, ported verbatim for T570. Removal stage: when the
//! shared tree kit lands a Rust `searchedRowLevels`/`effectiveExpanded`, this
//! file is the place they come from — the rules themselves are already
//! pinned by the row tests in `navigator_tree_row` and by the builder below.
//!
//! These two are the whole "a search removes exactly one level" and "a search
//! force-expands" story, in one place each, so the builder never spells
//! `has(key) || !!query` or `depth - 1` at its call sites.

/// React `searchedRowLevels`: the depth pair for a row painted at `painted`
/// while a search suppresses the header rows at and below `first_hidden_depth`.
///
/// Only rows at or below the hidden depth lose the level: a connection a
/// search moved up to depth 0 keeps announcing level 1 — there is no
/// announced parent left to hide it under.
pub fn searched_row_levels(
    painted: usize,
    first_hidden_depth: usize,
    searching: bool,
) -> (usize, usize) {
    let hidden = if searching && painted >= first_hidden_depth {
        1
    } else {
        0
    };
    // React `rowLevels(painted, hidden)`: levelDepth = max(0, painted - max(0, hidden)).
    (painted, painted.saturating_sub(hidden))
}

/// React `effectiveExpanded`: a forced expand is what a search does — every
/// match must be reachable without a second keystroke.
pub fn effective_expanded(own_expanded: bool, force_expand: bool) -> bool {
    own_expanded || force_expand
}
