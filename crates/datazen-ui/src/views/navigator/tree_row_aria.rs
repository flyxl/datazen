//! React `windows/connection/navigator/treeRowAria.ts` — the navigator's
//! ARIA, in one place, on the formal row model (T572).
//!
//! This used to be spelled out eleven times inside `NavigatorTreeRow` — once
//! per row variant — and every spelling was a place where "is this row a
//! branch?" could be answered differently. Two rules are worth naming,
//! because both look like omissions and are not:
//!
//! 1. **`aria-level` is the announced depth, not the painted one.** Under a
//!    search the builder shifts `level_depth` one rung up (the group header
//!    that used to sit between them is gone), while the indent keeps
//!    following `depth`. The two numbers are deliberately *not* harmonised:
//!    the indent is what the eye follows, the level is what the screen reader
//!    announces, and [`aria_level_of`] is the only conversion between them.
//! 2. **A leaf has no `aria-expanded` at all.** Not `false` — absent. A
//!    `false` on a row with no child list tells assistive tech the node is
//!    collapsed and therefore activatable, which is a lie about a table.
//!    "Does this row own a child list" is row-model knowledge, so the answer
//!    lives here rather than being defaulted by the shared shell.
//!
//! `empty-group` and `no-connections` are *decoration*: a hint block, not a
//! tree item, and they get no role and no level at all.
//!
//! Removal stage: the shared tree kit task ports `@datazen/ui`'s
//! `tree/{types,geometry}.ts` formally; until then `TREE_TOP_LEVEL` and the
//! level conversion live here (and, already noted, in T470's row VM module
//! for the paint side). When it lands both re-point, behaviour unchanged.

use super::types::NavigatorRow;

/// React `TREE_TOP_LEVEL` (`@datazen/ui` `tree/types.ts`): `aria-level` is
/// 1-based while a painted depth is 0-based, so the two are never the same
/// number.
pub const TREE_TOP_LEVEL: usize = 1;

/// React `VirtualTreeItemAria` — what a tree row announces. `expanded` is
/// `None` when the attribute is absent, which is not the same as `Some(false)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualTreeItemAria {
    pub role: &'static str,
    pub level: usize,
    pub expanded: Option<bool>,
}

/// React `ariaLevelOf` — the level a screen reader announces for `row`.
///
/// Follows the row's *logical* parent rather than its painted indent: a row
/// that declares a `level_depth` announces at `level_depth + 1`, which is how
/// a tree whose parent row was filtered away still describes a real hierarchy
/// instead of a hole.
pub fn aria_level_of(row: &NavigatorRow) -> usize {
    row.level_depth() + TREE_TOP_LEVEL
}

/// React `navigatorRowAria` — the ARIA for one row, or `None` for the two
/// decoration variants (they stay out of the tree structure entirely).
pub fn navigator_row_aria(row: &NavigatorRow) -> Option<VirtualTreeItemAria> {
    match row {
        NavigatorRow::Section { expanded, .. } | NavigatorRow::Group { expanded, .. } => {
            // The roots: there is no parent rung above them, so they announce
            // at `TREE_TOP_LEVEL` exactly as they always did.
            Some(VirtualTreeItemAria {
                role: "treeitem",
                level: TREE_TOP_LEVEL,
                expanded: Some(*expanded),
            })
        }
        NavigatorRow::Connection { .. } => {
            // The one place where "is a branch" and "announces a disclosure"
            // are allowed to disagree — see [`connection_expanded_state`].
            Some(VirtualTreeItemAria {
                role: "treeitem",
                level: aria_level_of(row),
                expanded: connection_expanded_state(row),
            })
        }
        NavigatorRow::Db { expanded, .. }
        | NavigatorRow::Schema { expanded, .. }
        | NavigatorRow::Category { expanded, .. }
        | NavigatorRow::NamespaceNode { expanded, .. } => {
            Some(branch_aria(row, aria_level_of(row), *expanded))
        }
        NavigatorRow::Table { .. }
        | NavigatorRow::Object { .. }
        | NavigatorRow::KvDb { .. }
        | NavigatorRow::DbLoading { .. }
        | NavigatorRow::DbError { .. } => Some(VirtualTreeItemAria {
            role: "treeitem",
            level: aria_level_of(row),
            expanded: None,
        }),
        NavigatorRow::EmptyGroup { .. } | NavigatorRow::NoConnections { .. } => None,
    }
}

/// React `branchAria` — announce `aria-expanded` only on a row that can
/// actually own a child list.
///
/// This exists because the two functions in the React file were answering the
/// same question two different ways: `navigatorRowAria` wrote `aria-expanded`
/// for every `namespace-node` it saw, while `isNavigatorBranch` — which the
/// shell uses to decide the same thing for navigation — read `isLeaf`. A
/// namespace leaf therefore announced itself as a collapsed disclosure of a
/// list it does not have, which is precisely the lie rule 2 above exists to
/// prevent.
///
/// Deriving the announcement from [`is_navigator_branch`] rather than from a
/// second copy of the leaf test makes the disagreement unrepresentable. Every
/// variant that can be a branch or a leaf routes through here, so a future
/// one is correct by construction instead of by a fourth spelling of the rule.
fn branch_aria(row: &NavigatorRow, level: usize, expanded: bool) -> VirtualTreeItemAria {
    if !is_navigator_branch(row) {
        return VirtualTreeItemAria { role: "treeitem", level, expanded: None };
    }
    VirtualTreeItemAria { role: "treeitem", level, expanded: Some(expanded) }
}

/// React `connectionExpandedState` — whether a connection row may announce a
/// disclosure.
///
/// Only a connected or connecting connection owns one. An idle or failed
/// connection has nothing to expand, and `aria-expanded` on it would offer the
/// user a disclosure that does nothing. The chevron button and the treeitem
/// both read this, which is why it is a function and not an inline ternary.
pub fn connection_expanded_state(row: &NavigatorRow) -> Option<bool> {
    let NavigatorRow::Connection { status, expanded, .. } = row else {
        return None;
    };
    if status == "connected" || status == "connecting" {
        Some(*expanded)
    } else {
        None
    }
}

/// React `isNavigatorBranch` — branch probe for the shared shell: does this
/// row own a child list?
pub fn is_navigator_branch(row: &NavigatorRow) -> bool {
    match row {
        NavigatorRow::Section { .. }
        | NavigatorRow::Group { .. }
        | NavigatorRow::Connection { .. }
        | NavigatorRow::Db { .. }
        | NavigatorRow::Schema { .. }
        | NavigatorRow::Category { .. } => true,
        NavigatorRow::NamespaceNode { is_leaf, .. } => !is_leaf,
        _ => false,
    }
}
