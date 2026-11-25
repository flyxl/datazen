//! Port of `packages/frontend/src/lib/structureEditor/ddlApplyWarnings.ts`.
//!
//! Pure value logic: a risky-alter predicate, an inclusive large-table row
//! threshold, the alter-only confirmation gate, and the warning-message
//! builder. No Rune tree, no state, no IPC.
//!
//! i18n note: `crate::i18n::t()` has no params API — it returns the translated
//! text with `{risks}`/`{rows}` placeholders intact. Following the project's
//! existing convention (see `views/connections_home.rs`), placeholders are
//! replaced at the call site with the exact values. Leftover: when the
//! structure-editor view lands, either add a params-aware `t(key, params)` in
//! `i18n.rs` or keep these call-site replacements; the three keys themselves
//! already exist in the zh-CN pack, so no new key was invented.

use crate::i18n::t;
use crate::structure_editor::{StatementRisk, StructureChangeMode, StructureChangePlan};

/// Row estimate at or above this threshold triggers an online DDL warning.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub const LARGE_TABLE_ROW_THRESHOLD: u64 = 100_000;

/// Port of `planHasRiskyAlter(plan)`: any statement with a destructive or
/// rewrite risk. Additive-only plans stay false.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn plan_has_risky_alter(plan: &StructureChangePlan) -> bool {
    plan.statements
        .iter()
        .any(|statement| statement.risk.is_risky())
}

/// Port of `isLargeTableEstimate(rows)`: `null`/`undefined` (both `None` here)
/// are false; the threshold is inclusive.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn is_large_table_estimate(rows: Option<u64>) -> bool {
    rows.is_some_and(|rows| rows >= LARGE_TABLE_ROW_THRESHOLD)
}

/// Port of `shouldConfirmAlterApply(args)`: mode must be `alter`, then either a
/// risky statement or a large row estimate confirms.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn should_confirm_alter_apply(
    mode: StructureChangeMode,
    plan: &StructureChangePlan,
    estimated_rows: Option<u64>,
) -> bool {
    if mode != StructureChangeMode::Alter {
        return false;
    }
    plan_has_risky_alter(plan) || is_large_table_estimate(estimated_rows)
}

/// Port of `buildAlterApplyWarningMessage(args)`.
///
/// Parts are joined with `\n\n`. The risky list is de-duplicated in first-seen
/// order; the React `Set` keeps the literal risk strings (`destructive`,
/// `rewrite`), which is what `StatementRisk::as_str()` returns. `t()` fills in
/// the zh-CN text and the placeholders are replaced with the exact values.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn build_alter_apply_warning_message(
    plan: &StructureChangePlan,
    estimated_rows: Option<u64>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if plan_has_risky_alter(plan) {
        let mut risks: Vec<&'static str> = Vec::new();
        for statement in &plan.statements {
            if statement.risk.is_risky() {
                let risk = statement.risk.as_str();
                if !risks.contains(&risk) {
                    risks.push(risk);
                }
            }
        }
        parts.push(
            t("structEditor.ddlWarn.risky").replace("{risks}", &risks.join(", ")),
        );
    }

    if is_large_table_estimate(estimated_rows) {
        // `is_large_table_estimate` is true only for `Some(rows >= threshold)`,
        // so the `if let` always runs here — no unwrap/expect needed.
        if let Some(rows) = estimated_rows {
            parts.push(
                t("structEditor.ddlWarn.largeTable").replace("{rows}", &rows.to_string()),
            );
        }
    }

    parts.push(t("structEditor.ddlWarn.footer").to_owned());
    parts.join("\n\n")
}
