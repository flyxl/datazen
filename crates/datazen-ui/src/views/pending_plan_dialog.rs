//! Port of `windows/connection/PendingPlanDialog.tsx`.
//!
//! React previews the plan a commit would execute: it joins the table name with
//! the three interpolated count phrases into the dialog description, then
//! lists the fingerprint and the SQL statements in execution order (inserts,
//! updates, deletes) with their parameter summaries and warnings. The SQL is a
//! preview only — the host rebuilds the bound statement before running it.
//! This port keeps that shape and routes the sole `onClose` action through an
//! injected host port; no fake IPC, timer or thread.
//!
//! ## Keys
//!
//! The React `testId` `pending-plan-dialog` is the dialog panel key. Body parts
//! carry Rune observation keys (`pending-plan-fingerprint`, `-statement-{i}`,
//! `-warning-{i}`, ...) named after their React elements; the dialog base adds
//! its own `-header` / `-body` / `-footer` keys.
//!
//! ## Local stand-ins (removal stage)
//!
//! The plan mirrors the few `RowChangePlan` fields React reads; the host owns
//! the real plan once the table-change panel lands. No production host mounts
//! this dialog yet, so every reusable item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the table-change
//! host wires the preview".

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

pub const DIALOG_KEY: &str = "pending-plan-dialog";
pub const CLOSE_KEY: &str = "pending-plan-close";

/// React `PlannedStatement`, reduced to the two fields the preview reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlannedStatement {
    pub sql_template: String,
    pub parameter_summary: Vec<String>,
}

/// React `ChangeWarning`.
#[allow(dead_code)] // VM mirror; remove when the host owns the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeWarning {
    pub code: String,
    pub message: String,
}

/// The `RowChangePlan` fields `PendingPlanDialog` reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowChangePlanVm {
    pub plan_id: String,
    pub fingerprint: String,
    /// `plan.table.table`.
    pub table: String,
    pub inserts: Vec<PlannedStatement>,
    pub updates: Vec<PlannedStatement>,
    pub deletes: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}

#[allow(dead_code)] // VM helper; remove with the host.
impl RowChangePlanVm {
    /// React's `inserts ?? []` is always a list on the wire; the mirror is too.
    pub fn statements(&self) -> Vec<&PlannedStatement> {
        self.inserts
            .iter()
            .chain(self.updates.iter())
            .chain(self.deletes.iter())
            .collect()
    }

    /// React's `description`: table then the three interpolated count phrases,
    /// joined with ` · `. The insert phrase is omitted for an empty list, and
    /// no raw count is prefixed — the wording already carries it.
    pub fn description(&self) -> String {
        let mut parts = vec![self.table.clone()];
        if !self.inserts.is_empty() {
            parts.push(
                t("tableData.pendingInserts").replace("{count}", &self.inserts.len().to_string()),
            );
        }
        parts.push(
            t("tableData.pendingUpdates").replace("{count}", &self.updates.len().to_string()),
        );
        parts.push(
            t("tableData.pendingDeletes").replace("{count}", &self.deletes.len().to_string()),
        );
        parts.join(" · ")
    }
}

/// The sole React callback (`onClose`), forwarded to the host.
#[allow(dead_code)] // host port; remove when the panel wires the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingPlanAction {
    Close,
}

#[allow(dead_code)] // host port; remove when the panel wires the dialog.
pub type PendingPlanActionFn<S> = Rc<dyn Fn(&mut S, PendingPlanAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<PendingPlanActionFn<S>>,
    action: PendingPlanAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the preview dialog into `layer`. A `None` plan mounts nothing, just
/// like React's `if (!plan) return null`; a closed dialog mounts nothing either
/// (`ui::dialog::Dialog` requires an open gate).
#[allow(dead_code)] // view entry; remove when the panel wires it.
pub fn build<S: 'static>(
    plan: Option<&RowChangePlanVm>,
    open: bool,
    on_action: &Option<PendingPlanActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let Some(plan) = plan else {
        return;
    };
    let k = kit(dark);

    // ── Body: fingerprint, statement list and warnings ───────────────
    let fingerprint = col()
        .key("pending-plan-fingerprint-block")
        .w_full()
        .gap(4.0)
        .child(
            text(t("tableData.previewFingerprint"))
                .key("pending-plan-fingerprint-label")
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
        .child(
            text(plan.fingerprint.clone())
                .key("pending-plan-fingerprint")
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .wrap_anywhere(),
        );

    let statements = plan.statements();
    let mut sql_list = col()
        .key("pending-plan-sql-list")
        .w_full()
        .gap(8.0);
    if statements.is_empty() {
        sql_list = sql_list.child(
            text(t("tableData.noPendingChanges"))
                .key("pending-plan-no-changes")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
    }
    for (at, statement) in statements.iter().enumerate() {
        let mut card = row()
            .key(format!("pending-plan-statement-{at}"))
            .w_full()
            .min_w(0.0)
            .padding(8.0)
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .child(
                text(statement.sql_template.clone())
                    .key(format!("pending-plan-statement-sql-{at}"))
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .wrap_anywhere()
                    .min_w(0.0),
            );
        if !statement.parameter_summary.is_empty() {
            card = card.child(
                text(format!(
                    "{}: {}",
                    t("tableData.previewParameters"),
                    statement.parameter_summary.join(", ")
                ))
                .key(format!("pending-plan-statement-params-{at}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap_anywhere()
                .min_w(0.0),
            );
        }
        sql_list = sql_list.child(card);
    }
    let sql_block = col()
        .key("pending-plan-sql-block")
        .w_full()
        .gap(4.0)
        .child(
            text(t("tableData.previewSql"))
                .key("pending-plan-sql-label")
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
        .child(sql_list);

    let mut body = col()
        .key("pending-plan-body-content")
        .w_full()
        .gap(12.0)
        .child(fingerprint)
        .child(sql_block);
    if !plan.warnings.is_empty() {
        let mut warnings = col()
            .key("pending-plan-warnings")
            .w_full()
            .gap(4.0)
            .padding(8.0)
            .radius(6.0)
            .fill(k.alpha(k.c.warning.fg, 0.10))
            .child(
                text(t("tableData.previewWarnings"))
                    .key("pending-plan-warnings-label")
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg),
            );
        for (at, warning) in plan.warnings.iter().enumerate() {
            warnings = warnings.child(
                text(warning.message.clone())
                    .key(format!("pending-plan-warning-{at}"))
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg)
                    .wrap_anywhere()
                    .min_w(0.0),
            );
        }
        body = body.child(warnings);
    }

    let close = Button::new(t("common.close"))
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .build(k)
        .key(CLOSE_KEY)
        .on_press(emit(on_action, PendingPlanAction::Close));

    Dialog::new(DIALOG_KEY, t("tableData.previewTitle"))
        .open(open)
        .description(plan.description())
        .width(520.0)
        .body_max_height(360.0)
        .body_pad((20.0, 16.0))
        .child(body)
        .footer(close)
        .on_close(emit(on_action, PendingPlanAction::Close))
        .build(k, layer);
}
