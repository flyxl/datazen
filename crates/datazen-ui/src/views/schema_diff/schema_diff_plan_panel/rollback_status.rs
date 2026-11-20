//! `SchemaDiffPlanPanel`'s rollback status line (T178, the React file's
//! `PlanRollbackStatus` internal component).
//!
//! The plan's rollback completeness verdict below the statements: nothing
//! when there are no statements, the green available line when nothing is
//! missing, the red none line when everything is missing (the
//! `ErrorBanner as="p"` semantics), and the yellow partial line with the
//! missing count in between.
//!
//! ## Keys
//!
//! The line keeps React's `data-testid` `schema-diff-rollback-status`
//! verbatim; the text is observable as `-text` (derived).
//!
//! Deltas: the ✅ / ⚠ / ❌ emoji become registry icons ([`ICON_CHECK`] /
//! [`ICON_ALERT_TRIANGLE`] / [`ICON_X`]) in the matching tone — the
//! error-boundary precedent; the `text-emerald-400` / `text-yellow-400`
//! palette classes map to the kit's success / warning inks; `{count}` is
//! formatted by the caller (React's `t(key, { count })`), exactly as the
//! i18n table's contract says.

use rune::{col, icon, row, text, El, TextStyle};

use crate::commands::schema_diff::rollback_completeness_counts;
use crate::commands::schema_diff::wire::SchemaDiffPlanVm;
use crate::i18n::t;
use crate::icons::{ICON_ALERT_TRIANGLE, ICON_CHECK, ICON_X};
use crate::ui::kit::kit;

/// The rollback status line (React's `PlanRollbackStatus`): `None` when
/// the plan has no statements.
pub(super) fn plan_rollback_status(plan: &SchemaDiffPlanVm, dark: bool) -> Option<El> {
    let total = plan.statements.len();
    if total == 0 {
        return None;
    }
    let k = kit(dark);
    let counts = rollback_completeness_counts(plan);

    // The none line is React's `<ErrorBanner as="p">` (the alert role);
    // the others are plain lines.
    let (icon_el, label, color, alert) = if counts.missing == 0 {
        (
            icon(&ICON_CHECK, 10.5).color(k.c.success.fg),
            t("schemaDiff.rollback.available").to_owned(),
            k.c.success.fg,
            false,
        )
    } else if counts.complete == 0 {
        (
            icon(&ICON_X, 10.5).color(k.c.danger.fg),
            t("schemaDiff.rollback.none").to_owned(),
            k.c.danger.fg,
            true,
        )
    } else {
        (
            icon(&ICON_ALERT_TRIANGLE, 10.5).color(k.c.warning.fg),
            t("schemaDiff.rollback.partial")
                .replace("{count}", &counts.missing.to_string()),
            k.c.warning.fg,
            false,
        )
    };

    let root = if alert { col() } else { row() };
    let mut line = root
        .key("schema-diff-rollback-status")
        .w_full()
        .gap(4.0)
        .center_y()
        .font(type_sm());
    if alert {
        line = line.role("alert");
    }
    Some(
        line.child(icon_el)
            .child(
                text(label)
                    .key("schema-diff-rollback-status-text")
                    .color(color),
            ),
    )
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}
