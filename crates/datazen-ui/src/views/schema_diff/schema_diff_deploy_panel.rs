//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffDeployPanel.tsx`.
//!
//! The schema-diff deploy review panel: the target/tables/statements summary
//! box, the transaction and rollback checkboxes with their dialect hints,
//! the destructive-plan confirm gate, the deploy button and the deploy
//! result receipt.
//!
//! ## What maps where
//!
//! - The wire vocabulary (`SchemaDiffPlan`, `PlanStatement`,
//!   `SchemaDiffDeployResult`, …) lives in `@datazen/backend-client`; the
//!   Rust host converts it into the `*Vm` projections in
//!   `commands/schema_diff` (the `PromptTemplateVm` pattern of the AI
//!   settings port) because `datazen-ui` does not depend on the client
//!   crate. That module (this segment's T377) absorbed the panel's former
//!   seam types and helpers — its single source — and this panel re-exports
//!   them so the audit's import path keeps resolving.
//! - `planHasDestructive` / `dialectSupportsTransactionalDdl` /
//!   `DESTRUCTIVE_CONFIRM_TOKEN` are 1:1 copies of
//!   `commands/schemaDiff.ts`'s pure helpers (the T377 seam).
//! - `canConfirmDestructiveDeploy` / `canRunDeploy` are 1:1 copies of
//!   `lib/schemaDiffConfirm.ts` — that file is another segment's task
//!   (T591), so the seam re-points when it lands.
//! - The five derived booleans React recomputes every render
//!   (`hasDestructive`, `txSupported`, `transactionRequired`,
//!   `transactionEnabled`, `hasUnsupportedRequirement`) plus `canRun` are
//!   one pure evaluation, [`deploy_readiness`] — the mutation-proved core.
//!   `canRun` is React's expression verbatim:
//!   `!result && !hasUnsupportedRequirement && (!transactionRequired ||
//!   (txSupported && transactionEnabled)) && (!requireRollback ||
//!   (txSupported && transactionEnabled)) && canRunDeploy(…)`.
//! - The controlled `confirmText` input is a `TextEdit` projection
//!   ([`SchemaDiffDeployPanel::on_confirm_edit`], the `ui/field.rs`
//!   convention); the two checkboxes are `ui/toggle.rs` `Checkbox`es whose
//!   toggles forward to the host (React's `onChange` value is the flipped
//!   state the host owns).
//! - `Button variant="run"` is the kit's `Submit` variant (the query-run
//!   green); `ErrorBanner as="p"` is `ui/banner.rs`'s `ErrorBanner` as a
//!   plain element; each result error is a `CopyableError`.
//! - The deploy status renders the wire string verbatim
//!   ([`DeployStatusVm::wire_str`]), exactly as React interpolates
//!   `{result.status}`.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim (`schema-diff-deploy`,
//! `-result`, `-status`, `-count`, `-errors`); the rest derive from the
//! component key (default `"schema-diff-deploy-panel"`): the summary box
//! `<key>-summary` with `-target`/`-tables`/`-statements` lines, the
//! transaction row `<key>-transaction` (+ `-required-hint` /
//! `-unsupported-hint`), the rollback row `<key>-rollback`, the incomplete
//! banner `<key>-rollback-incomplete`, the confirm block `<key>-confirm` /
//! `<key>-confirm-input`, and each copyable error `<key>-error-{i}`.
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75); the `list-disc` error bullets render as plain stacked rows (no
//! bullet glyph in the kit stack); the status text keeps the wire
//! vocabulary ('committed', not a translated label — React does the same).

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::copyable::CopyableError;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

// The wire vocabulary + the two pure helpers live in `commands/schema_diff`
// (T377, their single source) — re-exported here so the audit's import path
// from the panel module keeps resolving. Test-tree-consumed: the lib build
// alone sees them unused.
#[allow(unused_imports)]
pub use crate::commands::schema_diff::wire::{
    DeployStatusVm, PlanRequirementKindVm, PlanRequirementVm, PlanStatementVm, RollbackCompletenessVm,
    SchemaDiffDeployResultVm, SchemaDiffPlanVm, StatementRiskVm,
};
#[allow(unused_imports)]
pub use crate::commands::schema_diff::{
    DESTRUCTIVE_CONFIRM_TOKEN, dialect_supports_transactional_ddl, plan_has_destructive,
};

// ---- the wire projections' single source: commands/schema_diff (T377) ----
// (The panel's former local seam types moved there; the re-exports above
// keep this module's and the audit's paths valid.)

// ---- the pure helpers (1:1 from lib/schemaDiffConfirm.ts, the T591 seam) ----

/// React `canConfirmDestructiveDeploy`.
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub fn can_confirm_destructive_deploy(typed: &str, has_destructive: bool) -> bool {
    if !has_destructive {
        return true;
    }
    typed.trim() == DESTRUCTIVE_CONFIRM_TOKEN
}

/// React `canRunDeploy`.
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub fn can_run_deploy(
    has_destructive: bool,
    confirm_text: &str,
    require_rollback: bool,
    rollback_complete: bool,
    statement_count: usize,
) -> bool {
    if statement_count == 0 {
        return false;
    }
    if require_rollback && !rollback_complete {
        return false;
    }
    can_confirm_destructive_deploy(confirm_text, has_destructive)
}

// ---- the pure evaluation (React's five derived booleans + canRun) ----------

/// Everything the panel derives from the plan and its controlled props in
/// one render, the mutation-proved core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub struct DeployReadiness {
    pub has_destructive: bool,
    pub tx_supported: bool,
    pub transaction_required: bool,
    pub transaction_enabled: bool,
    pub has_unsupported_requirement: bool,
    /// React's `canRun`.
    pub can_run: bool,
}

/// React's render-time derivation, verbatim. `result_present` is
/// `result !== null`.
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub fn deploy_readiness(
    plan: &SchemaDiffPlanVm,
    use_transaction: bool,
    require_rollback: bool,
    confirm_text: &str,
    result_present: bool,
) -> DeployReadiness {
    let has_destructive = plan_has_destructive(plan);
    let tx_supported = dialect_supports_transactional_ddl(&plan.target_dialect);
    let transaction_required = plan
        .statements
        .iter()
        .any(|s| s.requires_transaction.unwrap_or(false));
    let transaction_enabled = transaction_required || (use_transaction && tx_supported);
    // Same rule as the wizard footer: `Backfill` is a post-deploy follow-up,
    // only `Unsupported` makes the plan deployless.
    let has_unsupported_requirement =
        plan.requirements.iter().any(|r| r.kind == PlanRequirementKindVm::Unsupported);
    let can_run = !result_present
        && !has_unsupported_requirement
        && (!transaction_required || (tx_supported && transaction_enabled))
        && (!require_rollback || (tx_supported && transaction_enabled))
        && can_run_deploy(
            has_destructive,
            confirm_text,
            require_rollback,
            plan.rollback_completeness.complete,
            plan.statements.len(),
        );
    DeployReadiness {
        has_destructive,
        tx_supported,
        transaction_required,
        transaction_enabled,
        has_unsupported_requirement,
        can_run,
    }
}

// ---- the builder -------------------------------------------------------------

type ToggleFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type DeployFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The panel builder, generic over the owning state `S` (the ported-dialog
/// pattern).
#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
pub struct SchemaDiffDeployPanel<'a, S: 'static> {
    key: &'a str,
    plan: &'a SchemaDiffPlanVm,
    target_label: &'a str,
    use_transaction: bool,
    require_rollback: bool,
    confirm_text: &'a TextEdit,
    deploying: bool,
    hide_deploy_button: bool,
    result: Option<&'a SchemaDiffDeployResultVm>,
    on_use_transaction: Option<ToggleFn<S>>,
    on_require_rollback: Option<ToggleFn<S>>,
    on_deploy: Option<DeployFn<S>>,
    confirm_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
}

#[allow(dead_code)] // reusable port; wired by the schema-diff wizard track
impl<'a, S: 'static> SchemaDiffDeployPanel<'a, S> {
    pub fn new(
        key: &'a str,
        plan: &'a SchemaDiffPlanVm,
        target_label: &'a str,
        use_transaction: bool,
        require_rollback: bool,
        confirm_text: &'a TextEdit,
        deploying: bool,
        result: Option<&'a SchemaDiffDeployResultVm>,
    ) -> Self {
        Self {
            key,
            plan,
            target_label,
            use_transaction,
            require_rollback,
            confirm_text,
            deploying,
            hide_deploy_button: false,
            result,
            on_use_transaction: None,
            on_require_rollback: None,
            on_deploy: None,
            confirm_edit: None,
        }
    }

    /// React `hideDeployButton` (default false).
    pub fn hide_deploy_button(mut self, hide: bool) -> Self {
        self.hide_deploy_button = hide;
        self
    }

    /// The transaction checkbox's `onUseTransactionChange` (the host flips
    /// its own flag — the new value is the flipped one).
    pub fn on_use_transaction(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_use_transaction = Some(Rc::new(f));
        self
    }

    /// The rollback checkbox's `onRequireRollbackChange`.
    pub fn on_require_rollback(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_require_rollback = Some(Rc::new(f));
        self
    }

    /// The confirm input's edit projection into `S`.
    pub fn on_confirm_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.confirm_edit = Some(Box::new(move |field| field.on_edit(f)));
        self
    }

    /// The deploy action.
    pub fn on_deploy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_deploy = Some(Rc::new(f));
        self
    }

    /// Paint the panel (React's `space-y-3 text-sm` column).
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        // The confirm edit leaves via take(), so the later &this
        // sub-builders stay whole (a partial move would not).
        let mut this = self;
        let confirm_edit = this.confirm_edit.take();
        let readiness = deploy_readiness(
            this.plan,
            this.use_transaction,
            this.require_rollback,
            &this.confirm_text.text,
            this.result.is_some(),
        );

        let mut panel = col().key(this.key).w_full().gap(12.0).font(type_sm());

        // --- The summary box (rounded border, surface-alt, text-xs). ---
        panel = panel.child(this.summary_box(&k));

        // --- The transaction checkbox + its dialect hints. ---
        panel = panel.child(this.transaction_row(&k, readiness));

        // --- The rollback checkbox. ---
        panel = panel.child(this.rollback_row(&k));

        // --- The incomplete-rollback banner. ---
        if this.require_rollback && !this.plan.rollback_completeness.complete {
            let missing = this.plan.rollback_completeness.missing.join("; ");
            if let Some(banner) = Banner::new(
                format!("{}-rollback-incomplete", this.key),
                format!("{}: {missing}", t("schemaDiff.rollbackIncomplete")),
            )
            .variant(BannerVariant::Plain)
            .as_element(BannerElement::P) // React <ErrorBanner as="p">
            .build(k)
            {
                panel = panel.child(banner);
            }
        }

        // --- The destructive confirm gate. ---
        if readiness.has_destructive {
            panel = panel.child(this.confirm_block(&k, dark, confirm_edit));
        }

        // --- The deploy button. ---
        if !this.hide_deploy_button {
            let disabled = !readiness.can_run || this.deploying;
            let mut deploy = Button::new(if this.deploying {
                t("schemaDiff.deploying").to_owned()
            } else {
                t("schemaDiff.deploy").to_owned()
            })
            .variant(ButtonVariant::Submit) // React variant="run"
            .control_size(ControlSize::Regular) // React size="md"
            .disabled(disabled);
            if !disabled {
                if let Some(deploy_cb) = &this.on_deploy {
                    let deploy_cb = Rc::clone(deploy_cb);
                    deploy = deploy.on_press(move |s: &mut S| deploy_cb(s));
                }
            }
            panel = panel.child(deploy.build(&k).key("schema-diff-deploy"));
        }

        // --- The result receipt. ---
        if let Some(result) = this.result {
            panel = panel.child(this.result_box(&k, result));
        }

        panel
    }

    /// `schemaDiff.reviewTarget/reviewTables/statements` in the bordered
    /// summary box.
    fn summary_box(&self, k: &Kit) -> El {
        col()
            .key(format!("{}-summary", self.key))
            .w_full()
            .gap(3.0)
            .padding(9.0) // React p-3
            .radius(3.0) // React rounded (4px = 3pt)
            .border(0.75, k.c.border_base)
            .fill(k.c.layer_01)
            .font(type_xs())
            .child(
                row()
                    .key(format!("{}-summary-target", self.key))
                    .gap(4.0)
                    .child(text(format!("{}:", t("schemaDiff.reviewTarget"))).font(type_xs()).color(k.c.text_muted))
                    .child(
                        text(self.target_label.to_owned())
                            .key(format!("{}-summary-target-value", self.key))
                            .font(type_xs().mono())
                            .color(k.c.text_base),
                    ),
            )
            .child(
                row()
                    .key(format!("{}-summary-tables", self.key))
                    .gap(4.0)
                    .child(text(format!("{}:", t("schemaDiff.reviewTables"))).font(type_xs()).color(k.c.text_muted))
                    .child(
                        text(self.plan.tables.join(", "))
                            .key(format!("{}-summary-tables-value", self.key))
                            .font(type_xs().mono())
                            .color(k.c.text_base),
                    ),
            )
            .child(
                row()
                    .key(format!("{}-summary-statements", self.key))
                    .gap(4.0)
                    .child(
                        text(format!("{}: {}", t("schemaDiff.statements"), self.plan.statements.len()))
                            .key(format!("{}-summary-statements-value", self.key))
                            .font(type_xs())
                            .color(k.c.text_muted),
                    ),
            )
    }

    /// The `useTransaction` checkbox: force-checked and disabled while any
    /// statement requires a transaction; disabled with a hint when the
    /// dialect has no transactional DDL.
    fn transaction_row(&self, k: &Kit, readiness: DeployReadiness) -> El {
        let mut checkbox = Checkbox::new(format!("{}-transaction", self.key), readiness.transaction_enabled)
            .disabled(!readiness.tx_supported || readiness.transaction_required);
        if let Some(on_tx) = &self.on_use_transaction {
            let on_tx = Rc::clone(on_tx);
            checkbox = checkbox.on_toggle(move |s: &mut S| on_tx(s));
        }
        let mut row = row()
            .key(format!("{}-transaction-row", self.key))
            .gap(8.0)
            .center_y()
            .child(checkbox.build(k))
            .child(text(t("schemaDiff.useTransaction").to_owned()).font(type_sm()).color(k.c.text_base));
        if readiness.transaction_required {
            row = row.child(
                text(format!("({})", t("schemaDiff.transactionRequired")))
                    .key(format!("{}-required-hint", self.key))
                    .font(type_xs())
                    .color(k.c.text_muted),
            );
        }
        if !readiness.tx_supported {
            row = row.child(
                text(format!("({})", t("schemaDiff.txUnsupported")))
                    .key(format!("{}-unsupported-hint", self.key))
                    .font(type_xs())
                    .color(k.c.text_muted),
            );
        }
        row
    }

    /// The `requireRollback` checkbox.
    fn rollback_row(&self, k: &Kit) -> El {
        let mut checkbox = Checkbox::new(format!("{}-rollback", self.key), self.require_rollback);
        if let Some(on_rb) = &self.on_require_rollback {
            let on_rb = Rc::clone(on_rb);
            checkbox = checkbox.on_toggle(move |s: &mut S| on_rb(s));
        }
        row()
            .key(format!("{}-rollback-row", self.key))
            .gap(8.0)
            .center_y()
            .child(checkbox.build(k))
            .child(text(t("schemaDiff.requireRollback").to_owned()).font(type_sm()).color(k.c.text_base))
    }

    /// The destructive plan's confirm label + DEPLOY input.
    fn confirm_block(
        &self,
        k: &Kit,
        dark: bool,
        confirm_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
    ) -> El {
        let mut field = TextField::new(format!("{}-confirm-input", self.key), self.confirm_text)
            .placeholder(DESTRUCTIVE_CONFIRM_TOKEN)
            .w_full();
        if let Some(edit) = confirm_edit {
            field = edit(field);
        }
        col()
            .key(format!("{}-confirm", self.key))
            .w_full()
            .gap(4.0) // React space-y-1
            .child(
                text(t("schemaDiff.confirmDeploy").replace("{token}", DESTRUCTIVE_CONFIRM_TOKEN))
                    .font(type_sm())
                    .color(k.c.text_muted),
            )
            .child(field.build(dark))
    }

    /// The result receipt: status line, executed count, copyable errors.
    fn result_box(&self, k: &Kit, result: &SchemaDiffDeployResultVm) -> El {
        let mut box_ = col()
            .key("schema-diff-deploy-result")
            .w_full()
            .gap(3.0)
            .padding(9.0) // React p-3
            .radius(3.0)
            .border(0.75, k.c.border_base)
            .fill(k.c.layer_01)
            .font(type_xs())
            .child(
                text(format!("{}: {}", t("schemaDiff.deployStatus"), result.status.wire_str()))
                    .key("schema-diff-deploy-status")
                    .font(TextStyle::new(9.0, 500.0))
                    .color(k.c.text_base),
            )
            .child(
                text(format!(
                    "{}/{} {}",
                    result.executed_count,
                    result.statement_count,
                    t("schemaDiff.executed")
                ))
                .key("schema-diff-deploy-count")
                .font(type_xs())
                .color(k.c.text_muted),
            );
        if !result.errors.is_empty() {
            let mut errors = col()
                .key("schema-diff-deploy-errors")
                .w_full()
                .gap(3.0)
                .mt(6.0); // React mt-2
            for (index, message) in result.errors.iter().enumerate() {
                errors = errors.child(
                    CopyableError::new(format!("{}-error-{index}", self.key), message.clone())
                        .copy_button(true)
                        .build::<S>(k),
                );
            }
            box_ = box_.child(errors);
        }
        box_
    }
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}
