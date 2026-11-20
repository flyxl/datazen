//! `SchemaDiffPlanPanel`'s requirements section (T178, the React file's
//! `PlanRequirements` internal component).
//!
//! Backfill requirements render as the yellow advisory card (⚠ title, mono
//! target, hint); unsupported ones as the boxed error banner (❌ title,
//! `target: reason`). An empty list renders nothing, like React's `null`.
//!
//! ## Keys
//!
//! React keys are `backfill-{target}-{reason}` / `unsupported-{target}-{reason}`
//! with `target = table.column` (or the bare table when the column is
//! empty — [`requirement_target`]); those stay verbatim. The section root
//! keeps React's `data-testid` `schema-diff-plan-requirements`.
//!
//! Deltas: the ⚠ / ❌ emoji become registry icons
//! ([`ICON_ALERT_TRIANGLE`] / [`ICON_X`]) in the matching tone — the
//! error-boundary precedent; the unsupported banner reproduces the kit
//! `ErrorBanner` Boxed chrome by hand (the banner atom is message-shaped,
//! the React banner carries two styled rows); pixel classes → points
//! (×0.75).

use rune::{col, icon, row, text, Color, El, TextStyle};

use crate::commands::schema_diff::wire::{
    PlanRequirementKindVm, PlanRequirementVm, StatementRiskVm,
};
use crate::i18n::t;
use crate::icons::{ICON_ALERT_TRIANGLE, ICON_X};
use crate::ui::kit::kit;

/// React `requirementTarget`: `table.column` when the column is non-empty,
/// else the bare table.
pub(super) fn requirement_target(req: &PlanRequirementVm) -> String {
    if req.column.is_empty() {
        req.table.clone()
    } else {
        format!("{}.{}", req.table, req.column)
    }
}

/// React `riskClass`: additive → success, destructive → danger,
/// rewrite → warning.
pub(super) fn risk_color(risk: StatementRiskVm, dark: bool) -> Color {
    let k = kit(dark);
    match risk {
        StatementRiskVm::Additive => k.c.success.fg,
        StatementRiskVm::Destructive => k.c.danger.fg,
        StatementRiskVm::Rewrite => k.c.warning.fg,
    }
}

/// The requirements section (React's `PlanRequirements`): `None` when the
/// list is empty, the section element otherwise.
pub(super) fn plan_requirements(requirements: &[PlanRequirementVm], dark: bool) -> Option<El> {
    if requirements.is_empty() {
        return None;
    }
    let k = kit(dark);
    let mut section = col()
        .key("schema-diff-plan-requirements")
        .w_full()
        .gap(6.0); // React space-y-2
    for req in requirements {
        let target = requirement_target(req);
        if req.kind == PlanRequirementKindVm::Backfill {
            // The yellow advisory card: ⚠ title, mono target, hint.
            section = section.child(
                col()
                    .key(format!("backfill-{target}-{}", req.reason))
                    .w_full()
                    .gap(2.0)
                    .padding(6.0) // React p-2
                    .radius(3.0) // React rounded
                    // border-yellow-500/30 + bg-yellow-500/10 + text-yellow-400
                    .border(0.75, k.alpha(k.c.warning.fg, 0.30))
                    .fill(k.alpha(k.c.warning.fg, 0.10))
                    .child(
                        row()
                            .key(format!("backfill-{target}-title"))
                            .gap(4.0)
                            .center_y()
                            .child(icon(&ICON_ALERT_TRIANGLE, 9.0).color(k.c.warning.fg))
                            .child(
                                text(t("schemaDiff.requirement.backfillTitle").to_owned())
                                    .font(type_sm_medium())
                                    .color(k.c.warning.fg),
                            ),
                    )
                    .child(
                        text(target.clone())
                            .key(format!("backfill-{target}-name"))
                            .font(type_xs().mono())
                            .color(k.c.warning.fg),
                    )
                    .child(
                        text(t("schemaDiff.requirement.backfillHint").to_owned())
                            .key(format!("backfill-{target}-hint"))
                            .font(type_xs())
                            .color(k.c.warning.fg)
                            .mt(3.0), // React mt-1
                    ),
            );
        } else {
            // The boxed ErrorBanner with its two styled rows (the kit
            // banner is message-shaped, so the Boxed chrome is reproduced:
            // radius/fill/border/padding from `BannerVariant::Boxed`).
            section = section.child(
                col()
                    .key(format!("unsupported-{target}-{}", req.reason))
                    .w_full()
                    .role("alert")
                    .radius(k.radius.md)
                    .fill(k.c.danger.bg)
                    .border(1.0, k.c.danger.border)
                    .padding(8.0)
                    .child(
                        row()
                            .key(format!("unsupported-{target}-title"))
                            .gap(4.0)
                            .center_y()
                            .child(icon(&ICON_X, 9.0).color(k.c.danger.fg))
                            .child(
                                text(t("schemaDiff.requirement.unsupportedTitle").to_owned())
                                    .font(type_sm_medium())
                                    .color(k.c.danger.fg),
                            ),
                    )
                    .child(
                        text(format!("{target}: {}", req.reason))
                            .key(format!("unsupported-{target}-detail"))
                            .font(type_xs())
                            .color(k.c.danger.fg),
                    ),
            );
        }
    }
    Some(section)
}

/// React `text-sm` (14px → 10.5pt).
pub(super) fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-sm font-medium` (10.5pt, weight 500).
pub(super) fn type_sm_medium() -> TextStyle {
    TextStyle::new(10.5, 500.0)
}

/// React `text-xs` (12px → 9pt).
pub(super) fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}
