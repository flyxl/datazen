//! Port of `windows/connection/structure/StructurePlanPreview.tsx`.
//!
//! React previews a structure change plan: a header (`common.sqlPreview` +
//! close), an optional warnings block, the statement count and a scrollable
//! list of statements (risk label, summary, SQL). An empty plan shows
//! `structEditor.emptyPlan`. Risk maps to a semantic colour: additive ->
//! success, destructive -> danger, rewrite -> warning. This port keeps that
//! shape and routes the close action through an injected host port — no fake
//! IPC. A loading branch is a documented local stand-in (React's parent gates
//! mounting); it shows a spinner and `common.loading`.
//!
//! ## Keys
//!
//! `struct-plan-preview` is React's `testId`; the rest are named after its
//! elements (`struct-plan-header`, `struct-plan-warnings`, `struct-plan-statement-{i}`, ...).
//!
//! ## Local stand-ins (removal stage)
//!
//! No production host mounts this preview yet; the structure editor owns the
//! plan. Every reusable item carries a per-item `#[allow(dead_code)]` with
//! removal stage "drop when the structure editor host wires this preview".

use std::rc::Rc;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::structure_editor::{PlanStatement, StatementRisk, StructureChangePlan};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

pub const ROOT_KEY: &str = "struct-plan-preview";

/// A host-owned projection: the plan plus a loading flag the React parent
/// controls by mounting or not mounting this component.
#[allow(dead_code)] // VM surface; remove when the host wires the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructurePlanPreviewVm {
    pub plan: StructureChangePlan,
    pub loading: bool,
}

#[allow(dead_code)] // VM surface; remove when the host wires the preview.
impl Default for StructurePlanPreviewVm {
    fn default() -> Self {
        Self {
            plan: StructureChangePlan {
                statements: Vec::new(),
                warnings: None,
            },
            loading: false,
        }
    }
}

#[allow(dead_code)] // VM helper; remove with the host.
impl StructurePlanPreviewVm {
    /// React's warnings block gate.
    pub fn has_warnings(&self) -> bool {
        self.plan.warnings.as_ref().is_some_and(|warnings| !warnings.is_empty())
    }

    /// React's statement-count line: `"{count} {t('structEditor.statements')}"`.
    pub fn count_text(&self) -> String {
        format!(
            "{} {}",
            self.plan.statements.len(),
            t("structEditor.statements")
        )
    }

    /// The semantic colour token for a risk (React `riskClass`).
    pub fn risk_is_destructive(risk: StatementRisk) -> bool {
        risk == StatementRisk::Destructive
    }
}

/// The sole React callback (`onClose`).
#[allow(dead_code)] // host port; remove when the host wires the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructurePlanAction {
    Close,
}

#[allow(dead_code)] // host port; remove when the host wires the preview.
pub type StructurePlanActionFn<S> = Rc<dyn Fn(&mut S, StructurePlanAction) + 'static>;

/// Build the preview into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the preview.
pub fn build<S: 'static>(
    vm: &StructurePlanPreviewVm,
    on_action: &Option<StructurePlanActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .min_w(0.0)
        .fill(k.c.layer_02);
    if vm.loading {
        root = root.child(
            row()
                .key("struct-plan-loading")
                .w_full()
                .gap(8.0)
                .center_y()
                .px(16.0)
                .py(8.0)
                .child(Spinner::new("struct-plan-spinner").size(SpinnerSize::Md).build(dark))
                .child(
                    text(t("common.loading"))
                        .key("struct-plan-loading-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        layer.push(root);
        return;
    }

    let mut close = text(t("common.close"))
        .key("struct-plan-close")
        .role("button")
        .focusable(true)
        .label(t("common.close"))
        .font(k.ty.text_small)
        .color(k.c.text_muted);
    if let Some(callback) = on_action.clone() {
        close = close.on_press(move |s: &mut S| callback(s, StructurePlanAction::Close));
    }
    root = root.child(
        row()
            .key("struct-plan-header")
            .w_full()
            .min_w(0.0)
            .center_y()
            .justify_between()
            .px(16.0)
            .py(8.0)
            .child(
                text(t("common.sqlPreview"))
                    .key("struct-plan-title")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
            .child(close),
    );

    if vm.has_warnings() {
        let warnings = vm.plan.warnings.as_ref().map_or(&[][..], |w| w.as_slice());
        let mut block = col()
            .key("struct-plan-warnings")
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .mb(8.0)
            .px(16.0)
            .py(8.0)
            .radius(6.0)
            .fill(k.alpha(k.c.warning.fg, 0.10))
            .child(
                text(t("structEditor.warnings"))
                    .key("struct-plan-warnings-title")
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg),
            );
        for (at, warning) in warnings.iter().enumerate() {
            block = block.child(
                text(warning.clone())
                    .key(format!("struct-plan-warning-{at}"))
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg)
                    .wrap_anywhere(),
            );
        }
        root = root.child(block);
    }

    root = root.child(
        row()
            .key("struct-plan-count")
            .w_full()
            .px(16.0)
            .pb(4.0)
            .child(
                text(vm.count_text())
                    .key("struct-plan-count-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            ),
    );

    let mut list = col().key("struct-plan-statements").w_full().min_w(0.0).gap(8.0).px(16.0).pb(12.0);
    if vm.plan.statements.is_empty() {
        list = list.child(
            text(t("structEditor.emptyPlan"))
                .key("struct-plan-empty")
                .font(k.ty.body)
                .color(k.c.text_muted),
        );
    }
    for (at, stmt) in vm.plan.statements.iter().enumerate() {
        list = list.child(statement_row(stmt, at, dark));
    }
    layer.push(root.child(list));
}

fn statement_row(stmt: &PlanStatement, at: usize, dark: bool) -> El {
    let k = kit(dark);
    let risk_color = match stmt.risk {
        StatementRisk::Additive => k.c.success.fg,
        StatementRisk::Destructive => k.c.danger.fg,
        StatementRisk::Rewrite => k.c.warning.fg,
    };
    col()
        .key(format!("struct-plan-statement-{at}"))
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .padding(8.0)
        .radius(6.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .child(
            row()
                .key(format!("struct-plan-risk-{at}"))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    text(stmt.risk.as_str())
                        .key(format!("struct-plan-risk-text-{at}"))
                        .font(k.ty.text_xsmall)
                        .color(risk_color),
                )
                .child(
                    text(stmt.summary.clone())
                        .key(format!("struct-plan-summary-{at}"))
                        .font(k.ty.text_small)
                        .color(k.c.text_base),
                ),
        )
        .child(
            text(stmt.sql.clone())
                .key(format!("struct-plan-sql-{at}"))
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .wrap_anywhere()
                .min_w(0.0),
        )
}
