//! Port of `packages/frontend/src/windows/connection/TablePendingChangesBar.tsx`.
//!
//! The React file owns two always-nearby table bars: the staged-change summary
//! (`TablePendingChangesBar`, preview / commit / rollback) and the manual
//! transaction toolbar (`TableTransactionControls`, begin / commit / rollback
//! plus lost-session recovery). Every `data-testid` is an E2E anchor, kept
//! verbatim. The transaction commands and execution polling stay with the host;
//! this port renders whatever snapshot the host feeds and emits intents through
//! an injected host port — no fake IPC, promise or timer lives here. No
//! confirmation dialog exists in the React file, so none is added.
//!
//! ## Keys
//!
//! `pending-changes-bar`, `pending-cells-count`, `pending-preview`,
//! `pending-commit`, `pending-rollback`, `table-transaction-error`,
//! `table-session-open-new`, `table-tx-controls`, `table-tx-begin`,
//! `table-tx-commit` and `table-tx-rollback` are React's test ids.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts these bars yet; the table-panel host owns the
//! VMs, runs the platform commands and maps the intents. Until then every
//! reusable item carries a per-item `#[allow(dead_code)]` with this stage.

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;

// ── TablePendingChangesBar ──────────────────────────────────────────────────

pub const PENDING_ROOT: &str = "pending-changes-bar";
pub const PENDING_TITLE_KEY: &str = "pending-changes-title";
pub const PENDING_INSERTS_KEY: &str = "pending-changes-inserts";
pub const PENDING_UPDATES_KEY: &str = "pending-changes-updates";
pub const PENDING_DELETES_KEY: &str = "pending-changes-deletes";
pub const PENDING_CELLS_KEY: &str = "pending-cells-count";
pub const PENDING_OFFPAGE_KEY: &str = "pending-changes-offpage";
pub const PENDING_PREVIEW_KEY: &str = "pending-preview";
pub const PENDING_COMMIT_KEY: &str = "pending-commit";
pub const PENDING_ROLLBACK_KEY: &str = "pending-rollback";

/// React `DirtyCounts` fields the bar reads.
#[allow(dead_code)] // VM mirror; supplied by the host dirty index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtyCountsVm {
    pub total_cells: usize,
    pub cells_on_page: usize,
}

impl DirtyCountsVm {
    /// React `totalCells - cellsOnPage`, clamped at zero.
    #[allow(dead_code)] // pure helper; consumed by the off-page segment and tests.
    pub fn cells_off_page(&self) -> usize {
        self.total_cells.saturating_sub(self.cells_on_page)
    }
}

/// React `TablePendingChangesBarProps` as a cloneable VM.
#[allow(dead_code)] // VM surface; owned by the table-panel host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingChangesVm {
    pub pending_count: usize,
    pub update_count: usize,
    pub delete_count: usize,
    pub insert_count: usize,
    pub counts: Option<DirtyCountsVm>,
    pub busy: bool,
}

impl PendingChangesVm {
    /// React `t('tableData.pendingChanges', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn summary_text(&self) -> String {
        t("tableData.pendingChanges").replace("{count}", &self.pending_count.to_string())
    }

    /// React `t('tableData.pendingInserts', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn inserts_text(&self) -> String {
        t("tableData.pendingInserts").replace("{count}", &self.insert_count.to_string())
    }

    /// React `t('tableData.pendingUpdates', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn updates_text(&self) -> String {
        t("tableData.pendingUpdates").replace("{count}", &self.update_count.to_string())
    }

    /// React `t('tableData.pendingDeletes', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn deletes_text(&self) -> String {
        t("tableData.pendingDeletes").replace("{count}", &self.delete_count.to_string())
    }

    /// React `t('tableData.pendingCells', { count })`, or None when absent/0.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn cells_text(&self) -> Option<String> {
        let total = self.counts.as_ref()?.total_cells;
        (total > 0).then(|| t("tableData.pendingCells").replace("{count}", &total.to_string()))
    }

    /// React `t('tableData.pendingOffPage', { count })`, or None when absent/0.
    #[allow(dead_code)] // pure helper; consumed by the summary and tests.
    pub fn offpage_text(&self) -> Option<String> {
        let off = self.counts.as_ref()?.cells_off_page();
        (off > 0).then(|| t("tableData.pendingOffPage").replace("{count}", &off.to_string()))
    }
}

// ── TableTransactionControls ────────────────────────────────────────────────

pub const TX_ERROR_KEY: &str = "table-transaction-error";
pub const TX_OPEN_NEW_KEY: &str = "table-session-open-new";
pub const TX_CONTROLS_KEY: &str = "table-tx-controls";
pub const TX_BEGIN_KEY: &str = "table-tx-begin";
pub const TX_COMMIT_KEY: &str = "table-tx-commit";
pub const TX_ROLLBACK_KEY: &str = "table-tx-rollback";

/// React `TableTransactionControls` snapshot (session/controller state).
#[allow(dead_code)] // VM surface; owned by the table-panel host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionControlsVm {
    pub is_editable: bool,
    pub loading: bool,
    pub in_tx: bool,
    pub tx_busy: bool,
    pub tx_lost: bool,
    pub tx_error: String,
}

// ── Host actions ────────────────────────────────────────────────────────────

#[allow(dead_code)] // host port; remove when the table-panel host lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingAction {
    Preview,
    Commit,
    Rollback,
    BeginTx,
    CommitTx,
    RollbackTx,
    OpenNewSession,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, PendingAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: PendingAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action);
        }
    }
}

fn action_button<S: 'static>(
    key: &str,
    label: String,
    variant: ButtonVariant,
    disabled: bool,
    action: PendingAction,
    on_action: &Option<ActionFn<S>>,
    k: &Kit,
) -> El {
    let mut button = Button::new(label)
        .variant(variant)
        .control_size(ControlSize::Small)
        .disabled(disabled);
    // A disabled button is inert: React's `disabled` blocks the click entirely.
    if !disabled {
        button = button.on_press(emit(on_action, action));
    }
    button.build(k).key(key)
}

/// Build the staged-change bar. React renders nothing when `pendingCount === 0`.
#[allow(dead_code)] // view entry; remove when the table-panel host lands.
pub fn build_pending<S: 'static>(
    vm: &PendingChangesVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    if vm.pending_count == 0 {
        return col().key("pending-changes-none").w_full().h_full().a11y_hidden();
    }
    let k = kit(dark);
    let mut bar = row()
        .key(PENDING_ROOT)
        .label(if vm.busy { "true" } else { "false" })
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .px(12.0)
        .py(6.0);

    bar = bar.child(
        text(vm.summary_text())
            .key(PENDING_TITLE_KEY)
            .font(k.ty.text_small)
            .color(k.c.text_accent),
    );
    if vm.insert_count > 0 {
        bar = bar.child(text(vm.inserts_text()).key(PENDING_INSERTS_KEY).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    if vm.update_count > 0 {
        bar = bar.child(text(vm.updates_text()).key(PENDING_UPDATES_KEY).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    if vm.delete_count > 0 {
        bar = bar.child(text(vm.deletes_text()).key(PENDING_DELETES_KEY).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    if let Some(cells) = vm.cells_text() {
        bar = bar.child(text(cells).key(PENDING_CELLS_KEY).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    if let Some(offpage) = vm.offpage_text() {
        bar = bar.child(text(offpage).key(PENDING_OFFPAGE_KEY).font(k.ty.text_xsmall).color(k.c.text_muted));
    }

    let mut actions = row().key("pending-actions").w_full().gap(6.0).center_y().justify_end();
    actions = actions.child(action_button(
        PENDING_PREVIEW_KEY,
        t("tableData.preview").to_owned(),
        ButtonVariant::Ghost,
        vm.busy,
        PendingAction::Preview,
        &on_action,
        k,
    ));
    actions = actions.child(action_button(
        PENDING_COMMIT_KEY,
        t("tableData.commit").to_owned(),
        ButtonVariant::Contrast,
        vm.busy,
        PendingAction::Commit,
        &on_action,
        k,
    ));
    actions = actions.child(action_button(
        PENDING_ROLLBACK_KEY,
        t("tableData.rollback").to_owned(),
        ButtonVariant::Ghost,
        vm.busy,
        PendingAction::Rollback,
        &on_action,
        k,
    ));
    bar.child(row().key("pending-actions-wrap").w_full().min_w(0.0).grow(1.0).child(actions))
}

/// Build the manual transaction toolbar. The error banner (with the optional
/// open-new-session recovery) sits above the controls, which are mounted only
/// for an editable panel.
#[allow(dead_code)] // view entry; remove when the table-panel host lands.
pub fn build_tx<S: 'static>(
    vm: &TransactionControlsVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut root = col().key("table-transaction-controls").w_full().min_w(0.0).gap(0.0);

    if !vm.tx_error.is_empty() {
        let mut banner = row()
            .key(TX_ERROR_KEY)
            .w_full()
            .min_w(0.0)
            .px(12.0)
            .py(4.0)
            .child(text(vm.tx_error.clone()).font(k.ty.text_xsmall).color(k.c.text_accent));
        if vm.tx_lost {
            let mut open_new = row()
                .key(TX_OPEN_NEW_KEY)
                .focusable(true)
                .label(t("query.session.openNew"))
                .child(text(t("query.session.openNew")).font(k.ty.text_xsmall).color(k.c.text_accent));
            open_new = open_new.on_press(emit(&on_action, PendingAction::OpenNewSession));
            banner = banner.child(row().key("tx-open-new-wrap").px(8.0).child(open_new));
        }
        root = root.child(banner);
    }

    if vm.is_editable {
        let status = if vm.in_tx { t("tableData.inManualTx") } else { t("tableData.shortTxDefault") };
        let mut controls = row()
            .key(TX_CONTROLS_KEY)
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .center_y()
            .px(12.0)
            .py(4.0)
            .child(text(status.to_owned()).font(k.ty.text_xsmall).color(k.c.text_muted));

        if vm.in_tx {
            controls = controls.child(action_button(
                TX_COMMIT_KEY,
                t("tableData.commitTx").to_owned(),
                ButtonVariant::Ghost,
                vm.tx_busy || vm.loading,
                PendingAction::CommitTx,
                &on_action,
                k,
            ));
            controls = controls.child(action_button(
                TX_ROLLBACK_KEY,
                t("tableData.rollbackTx").to_owned(),
                ButtonVariant::Ghost,
                vm.tx_busy || vm.loading,
                PendingAction::RollbackTx,
                &on_action,
                k,
            ));
            controls = controls.child(
                text(t("tableData.txManualHint")).key("table-tx-hint").font(k.ty.text_xsmall).color(k.c.text_accent),
            );
        } else {
            controls = controls.child(action_button(
                TX_BEGIN_KEY,
                t("tableData.beginTx").to_owned(),
                ButtonVariant::Ghost,
                vm.tx_busy || vm.loading,
                PendingAction::BeginTx,
                &on_action,
                k,
            ));
        }
        root = root.child(controls);
    }
    root
}
