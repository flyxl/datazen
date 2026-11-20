//! The wording of the foreign-key jump entry (DB-07), ported from React
//! `packages/frontend/src/components/DataTable/fkCellText.ts` (tasklist T405).
//!
//! It uses the standalone translation function on purpose — the same reason
//! as `dirtyCellText`: the label is built per cell, per render, and handing
//! a translate function down to every row would make each row subscribe to
//! the locale.
//!
//! The target label comes from the foreign-key group: `customers.id` for a
//! single-column key, `sales.parents.(x, y)` for a composite one. A composite
//! key names the table **and** the columns, because "jump to sales.parents"
//! alone does not say which row is meant.

use crate::i18n::t;

/// `fkJumpLabel`: the wording of the jump entry for one target label.
///
/// The composite detection is the first `'.('` in the label: the table is
/// everything before it, the columns everything between it and the closing
/// bracket. A label without `'.('` is a single-column target, interpolated
/// as `{target}`.
pub(crate) fn fk_jump_label(target_label: &str) -> String {
    let Some(composite_at) = target_label.find(".(") else {
        return t("fkNav.jumpTo").replace("{target}", target_label);
    };
    // `table.(column, column)`: slice(0, compositeAt) and
    // slice(compositeAt + 2, -1) — everything before the bracket, and
    // everything inside it, closing bracket excluded.
    let table = &target_label[..composite_at];
    let columns = &target_label[composite_at + 2..target_label.len() - 1];
    t("fkNav.jumpToComposite")
        .replace("{table}", table)
        .replace("{columns}", columns)
}

/// `fkResolvingLabel`: the wording while the resolution of a target is in
/// flight.
pub(crate) fn fk_resolving_label() -> String {
    t("fkNav.resolving").to_owned()
}
