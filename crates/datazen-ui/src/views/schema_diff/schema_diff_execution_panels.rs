//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffExecutionPanels.tsx`
//! (T075).
//!
//! The wizard's per-step execution surfaces, one branch per step:
//!
//! - **compare** — the loading splash while the first diffs arrive, or the
//!   three-pane row: the table list rail (T269), the column resizer, and
//!   the detail panel (the selected table's header with the copy-summary
//!   button above the schema diff panel, or the select-a-table hint).
//! - **plan** — the generating splash while the first plan arrives, the
//!   host's plan-actions row (a slot), then the right review panel (T204)
//!   pinned to the plan tab.
//! - **deploy** — the right review panel pinned to the deploy tab, with
//!   the deploy button hidden (the step's footer owns the action) and
//!   `deploying`/`deployResult` threaded from the run.
//!
//! ## What maps where
//!
//! - Every prop becomes a builder field and threads into the composed
//!   builders: [`SchemaDiffTableListPanel`] (T269), [`SchemaDiffPanel`]
//!   (the components/schema port) and [`SchemaDiffRightPanel`] (T204,
//!   twice — plan tab and deploy tab, both `hide_tabs`, the deploy one
//!   `hide_deploy_button`).
//! - `planActions: ReactNode` is a slot: [`Self::plan_actions`] takes the
//!   host's built [`El`].
//! - `clipboardFeedback` (the `'summary' | 'sql' | 'config' | null` union)
//!   becomes [`SchemaDiffClipboardFeedback`]; only `Summary` is read here
//!   (the copy button's `common.copied` swap), the other arms exist for
//!   the wizard's other copy sites.
//! - `tableListResizeRef` (the drag-to-resize handle's ref) becomes the
//!   resizer element with its hover wash; the drag itself is the
//!   `useColumnResize` host concern (T183/T307 wire it — the handle carries
//!   no interaction of its own here, exactly like the React div whose only
//!   behaviour is the attached listener).
//! - The copy button: React `variant="secondary" size="sm"` with the lucide
//!   Copy glyph → the kit button's default variant + Small size +
//!   [`ICON_COPY`] (the copyable-button convention).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `schema-diff-detail-panel`
//! (the compare detail pane). Derived: the compare row, the resizer
//! (`schema-diff-table-list-resizer`), the compare loading splash, the
//! selected table header + name + copy button, and the generating splash.
//!
//! Deltas: pixel classes → points (×0.75); `bg-surface-alt/30` → the
//! 30%-alpha wash of the surface-alt token; `cursor-col-resize` is a host
//! cursor concern.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextEdit, TextStyle};

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, SchemaDiffDeployResultVm, SchemaDiffPlanVm,
};
use crate::components::schema::schema_diff_panel::{SchemaDiffPanel, TableSchemaDiffVm};
use crate::i18n::t;
use crate::icons::ICON_COPY;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::schema_diff::schema_diff_plan_panel::OverrideEditFn;
use crate::views::schema_diff::schema_diff_right_panel::{
    SchemaDiffRightPanel, SchemaDiffRightPanelTab,
};
use crate::views::schema_diff::schema_diff_table_list_panel::SchemaDiffTableListPanel;
use rune_kit::{Button, ButtonVariant, ControlSize};

type ToggleFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type NameFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;

/// React `ExecutionStep`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffExecutionStep {
    Compare,
    Plan,
    Deploy,
}

/// React `ClipboardFeedback` (`'summary' | 'sql' | 'config' | null`); the
/// `null` arm is `Option::None` at the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffClipboardFeedback {
    Summary,
    Sql,
    Config,
}

/// The execution panels builder, generic over the owning state `S`.
pub(crate) struct SchemaDiffExecutionPanels<'a, S: 'static> {
    step: SchemaDiffExecutionStep,
    loading: bool,
    diffs: &'a [TableSchemaDiffVm],
    selected_diff: Option<&'a TableSchemaDiffVm>,
    selected_tables: &'a [String],
    selected_table: Option<&'a str>,
    table_has_diff: Option<&'a HashMap<String, bool>>,
    table_list_width: f32,
    clipboard_feedback: Option<SchemaDiffClipboardFeedback>,
    plan: Option<&'a SchemaDiffPlanVm>,
    plan_actions: Option<El>,
    allow_destructive: bool,
    include_indexes: bool,
    type_overrides: &'a [ColumnTypeOverrideVm],
    override_edits: Option<&'a HashMap<String, TextEdit>>,
    target_label: &'a str,
    use_transaction: bool,
    require_rollback: bool,
    confirm_text: &'a TextEdit,
    deploy_result: Option<&'a SchemaDiffDeployResultVm>,
    on_select_table: Option<NameFn<S>>,
    on_copy_summary: Option<ToggleFn<S>>,
    on_regenerate: Option<ToggleFn<S>>,
    on_allow_destructive: Option<ToggleFn<S>>,
    on_include_indexes: Option<ToggleFn<S>>,
    on_apply_type_overrides: Option<ToggleFn<S>>,
    on_override_edit: Option<OverrideEditFn<S>>,
    on_use_transaction: Option<ToggleFn<S>>,
    on_require_rollback: Option<ToggleFn<S>>,
    on_deploy: Option<ToggleFn<S>>,
    confirm_projection: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
}

impl<'a, S: 'static> SchemaDiffExecutionPanels<'a, S> {
    /// React's core props: the step, the loading flag, the diffs, the
    /// selected table state and the plan-side inputs.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        step: SchemaDiffExecutionStep,
        loading: bool,
        diffs: &'a [TableSchemaDiffVm],
        selected_diff: Option<&'a TableSchemaDiffVm>,
        selected_tables: &'a [String],
        selected_table: Option<&'a str>,
        table_list_width: f32,
        plan: Option<&'a SchemaDiffPlanVm>,
    ) -> Self {
        Self {
            step,
            loading,
            diffs,
            selected_diff,
            selected_tables,
            selected_table,
            table_has_diff: None,
            table_list_width,
            clipboard_feedback: None,
            plan,
            plan_actions: None,
            allow_destructive: false,
            include_indexes: false,
            type_overrides: &[],
            override_edits: None,
            target_label: "",
            use_transaction: false,
            require_rollback: false,
            confirm_text: empty_edit(),
            deploy_result: None,
            on_select_table: None,
            on_copy_summary: None,
            on_regenerate: None,
            on_allow_destructive: None,
            on_include_indexes: None,
            on_apply_type_overrides: None,
            on_override_edit: None,
            on_use_transaction: None,
            on_require_rollback: None,
            on_deploy: None,
            confirm_projection: None,
        }
    }

    /// React's `tableHasDiff` (the rail's badges; `undefined` renders none,
/// and the compare step passes it only when diffs exist).
    pub(crate) fn table_has_diff(mut self, map: Option<&'a HashMap<String, bool>>) -> Self {
        self.table_has_diff = map;
        self
    }

    /// React's `clipboardFeedback`.
    pub(crate) fn clipboard_feedback(
        mut self,
        feedback: Option<SchemaDiffClipboardFeedback>,
    ) -> Self {
        self.clipboard_feedback = feedback;
        self
    }

    /// React's `planActions` slot (a host-built element).
    pub(crate) fn plan_actions(mut self, actions: Option<El>) -> Self {
        self.plan_actions = actions;
        self
    }

    /// React's `allowDestructive` / `includeIndexes` / `targetLabel` /
    /// `useTransaction` / `requireRollback` / `confirmText` /
    /// `deployResult` (the plan and deploy tabs' threading).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn review_inputs(
        mut self,
        allow_destructive: bool,
        include_indexes: bool,
        target_label: &'a str,
        use_transaction: bool,
        require_rollback: bool,
        confirm_text: &'a TextEdit,
        deploy_result: Option<&'a SchemaDiffDeployResultVm>,
    ) -> Self {
        self.allow_destructive = allow_destructive;
        self.include_indexes = include_indexes;
        self.target_label = target_label;
        self.use_transaction = use_transaction;
        self.require_rollback = require_rollback;
        self.confirm_text = confirm_text;
        self.deploy_result = deploy_result;
        self
    }

    /// React's `typeOverrides` and the host-mirrored edit slots (the plan
    /// panel's contract).
    pub(crate) fn type_overrides(
        mut self,
        overrides: &'a [ColumnTypeOverrideVm],
        edits: Option<&'a HashMap<String, TextEdit>>,
    ) -> Self {
        self.type_overrides = overrides;
        self.override_edits = edits;
        self
    }

    /// React's `onSelectTable`.
    pub(crate) fn on_select_table(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select_table = Some(Rc::new(f));
        self
    }

    /// React's `onCopySummary`.
    pub(crate) fn on_copy_summary(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_summary = Some(Rc::new(f));
        self
    }

    /// React's `onRegenerate`.
    pub(crate) fn on_regenerate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_regenerate = Some(Rc::new(f));
        self
    }

    /// React's `onAllowDestructiveChange`.
    pub(crate) fn on_allow_destructive(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_allow_destructive = Some(Rc::new(f));
        self
    }

    /// React's `onIncludeIndexesChange`.
    pub(crate) fn on_include_indexes(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_include_indexes = Some(Rc::new(f));
        self
    }

    /// React's `onApplyTypeOverrides`.
    pub(crate) fn on_apply_type_overrides(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_type_overrides = Some(Rc::new(f));
        self
    }

    /// React's `onTypeOverrideChange` landing (the plan panel's contract).
    pub(crate) fn on_override_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_override_edit = Some(Rc::new(f));
        self
    }

    /// React's `onUseTransactionChange`.
    pub(crate) fn on_use_transaction(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_use_transaction = Some(Rc::new(f));
        self
    }

    /// React's `onRequireRollbackChange`.
    pub(crate) fn on_require_rollback(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_require_rollback = Some(Rc::new(f));
        self
    }

    /// The confirm input's edit projection (React's
    /// `onConfirmTextChange`).
    pub(crate) fn on_confirm_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.confirm_projection = Some(Rc::new(f));
        self
    }

    /// React's `onDeploy`.
    pub(crate) fn on_deploy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_deploy = Some(Rc::new(f));
        self
    }

    /// Paint the active step's surface.
    pub(crate) fn build(self, dark: bool) -> El {
        match self.step {
            SchemaDiffExecutionStep::Compare => self.compare_step(dark),
            SchemaDiffExecutionStep::Plan => self.plan_step(dark),
            SchemaDiffExecutionStep::Deploy => self.deploy_step(dark),
        }
    }

    // ---- compare -------------------------------------------------------------

    fn compare_step(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut step = col()
            .key("schema-diff-compare-step")
            .w_full()
            .min_w(0.0)
            .min_h(0.0)
            .grow(1.0); // overflow-hidden -> the min-size rule + the scroll children
        // The first-load splash: a spinner while no diff has arrived.
        if self.loading && self.diffs.is_empty() {
            return step.child(
                row()
                    .key("schema-diff-compare-loading")
                    .grow(1.0)
                    .center()
                    .gap(6.0) // React gap-2
                    .child(Spinner::new("schema-diff-compare-spinner").size(SpinnerSize::Lg).build(dark))
                    .child(
                        text(t("schemaDiff.compare").to_owned())
                            .key("schema-diff-compare-loading-text")
                            .font(type_sm())
                            .color(k.c.text_muted),
                    ),
            );
        }

        // The three-pane row.
        let mut panes = row()
            .key("schema-diff-compare-row")
            .min_h(0.0)
            .grow(1.0)
            .w_full()
            .radius(4.5) // React rounded-lg
            .border(0.75, k.c.border_base)
            .clip_rounded(4.5); // overflow-hidden: children clipped to the corner

        // 1) the table list rail (flex-none, pinned width) — the diff
        //    badges only exist once diffs arrived.
        let mut rail = SchemaDiffTableListPanel::<S>::new(
            self.selected_tables,
            self.selected_table,
        )
        .width(self.table_list_width);
        let has_diff_map = if self.diffs.is_empty() { None } else { self.table_has_diff };
        rail = rail.table_has_diff(has_diff_map);
        if let Some(on_select) = &self.on_select_table {
            let on_select = Rc::clone(on_select);
            rail = rail.on_select(move |s: &mut S, table| on_select(s, table));
        }
        panes = panes.child(rail.build(dark));

        // 2) the resizer handle (the drag wiring is the host's).
        panes = panes.child(
            col()
                .key("schema-diff-table-list-resizer")
                .w(0.75) // React w-1
                .h_full()
                .hover(k.alpha(k.c.bg_accent, 0.30)), // hover:bg-accent/30
        );

        // 3) the detail panel.
        let mut detail = col()
            .key("schema-diff-detail-panel")
            .min_h(0.0)
            .min_w(0.0)
            .grow(1.0)
            .w_full()
            .fill(k.alpha(k.c.layer_01, 0.30)) // bg-surface-alt/30
            .scroll(ScrollSpec { stick_bottom: false });
        if let Some(diff) = self.selected_diff {
            let mut header = row()
                .key("schema-diff-detail-header")
                .w_full()
                .center_y()
                .justify_between()
                .gap(6.0) // React gap-2
                .mb(9.0); // React mb-3
            header = header.child(
                text(diff.table.clone())
                    .key("schema-diff-detail-table")
                    .font(type_sm_medium().mono())
                    .color(k.c.text_base),
            );
            if !self.diffs.is_empty() {
                let copied = self.clipboard_feedback
                    == Some(SchemaDiffClipboardFeedback::Summary);
                let label = if copied {
                    t("common.copied").to_owned()
                } else {
                    t("schemaDiff.copySummary").to_owned()
                };
                let mut copy = Button::new(label)
                    .variant(ButtonVariant::Neutral) // React variant="secondary"
                    .control_size(ControlSize::Small) // React size="sm"
                    .icon(&ICON_COPY);
                if let Some(on_copy) = &self.on_copy_summary {
                    let on_copy = Rc::clone(on_copy);
                    copy = copy.on_press(move |s: &mut S| on_copy(s));
                }
                header = header.child(copy.build(&k).key("schema-diff-copy-summary"));
            }
            detail = detail
                .child(
                    col()
                        .key("schema-diff-detail-body")
                        .w_full()
                        .padding(12.0) // React p-4
                        .child(header)
                        .child(SchemaDiffPanel::new((*diff).clone()).build(dark)),
                );
        } else {
            detail = detail.child(
                col()
                    .key("schema-diff-detail-hint")
                    .w_full()
                    .grow(1.0)
                    .padding(12.0) // React p-4
                    .center()
                    .child(
                        text(t("schemaDiff.selectTableHint").to_owned())
                            .key("schema-diff-detail-hint-text")
                            .font(type_sm())
                            .color(k.c.text_muted),
                    ),
            );
        }
        panes = panes.child(detail);
        step.child(panes)
    }

    // ---- plan ------------------------------------------------------------------

    fn plan_step(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut step = col()
            .key("schema-diff-plan-step")
            .w_full()
            .min_w(0.0)
            .min_h(0.0)
            .grow(1.0)
            .gap(9.0); // React gap-3 (overflow-hidden -> the min-size rule)
        // The generating splash while the first plan arrives.
        if self.loading && self.plan.is_none() {
            step = step.child(
                row()
                    .key("schema-diff-plan-generating")
                    .gap(6.0)
                    .center_y()
                    .child(Spinner::new("schema-diff-plan-spinner").size(SpinnerSize::Lg).build(dark))
                    .child(
                        text(t("schemaDiff.generating").to_owned())
                            .key("schema-diff-plan-generating-text")
                            .font(type_sm())
                            .color(k.c.text_muted),
                    ),
            );
        }
        // The host's plan-actions slot.
        if let Some(actions) = self.plan_actions.clone() {
            step = step.child(actions);
        }
        // The review panel pinned to the plan tab: no tab strip, deploy
        // inert (React's `deploying={false}` / `onDeploy={() => {}}` /
        // `deployResult={null}`).
        step.child(
            self.review_panel(SchemaDiffRightPanelTab::Plan, false, None, dark)
                .hide_tabs(true)
                .build(dark),
        )
    }

    // ---- deploy -----------------------------------------------------------------

    fn deploy_step(&self, dark: bool) -> El {
        // The review panel pinned to the deploy tab: no tab strip, the
        // deploy button hidden (the step's footer owns the action).
        self.review_panel(SchemaDiffRightPanelTab::Deploy, self.loading, self.deploy_result, dark)
            .hide_tabs(true)
            .hide_deploy_button(true)
            .build(dark)
    }

    /// The shared review-panel builder (the T204 threading).
    fn review_panel(
        &self,
        tab: SchemaDiffRightPanelTab,
        deploying: bool,
        deploy_result: Option<&'a SchemaDiffDeployResultVm>,
        _dark: bool,
    ) -> SchemaDiffRightPanel<'a, S> {
        let mut panel = SchemaDiffRightPanel::<S>::new(
            tab,
            self.plan,
            self.allow_destructive,
            self.include_indexes,
            self.target_label,
            self.use_transaction,
            self.require_rollback,
            self.confirm_text,
            deploying,
            deploy_result,
        )
        .regenerating(self.loading && self.plan.is_some())
        .type_overrides(self.type_overrides);
        if let Some(edits) = self.override_edits {
            panel = panel.override_edits(Some(edits));
        }
        if let Some(accessor) = &self.on_override_edit {
            let accessor = Rc::clone(accessor);
            panel = panel.on_override_edit(move |s, table, column, initial| {
                accessor(s, table, column, initial)
            });
        }
        if let Some(f) = &self.on_allow_destructive {
            let f = Rc::clone(f);
            panel = panel.on_allow_destructive(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_include_indexes {
            let f = Rc::clone(f);
            panel = panel.on_include_indexes(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_regenerate {
            let f = Rc::clone(f);
            panel = panel.on_regenerate(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_apply_type_overrides {
            let f = Rc::clone(f);
            panel = panel.on_apply_type_overrides(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_use_transaction {
            let f = Rc::clone(f);
            panel = panel.on_use_transaction(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_require_rollback {
            let f = Rc::clone(f);
            panel = panel.on_require_rollback(move |s: &mut S| f(s));
        }
        if let Some(projection) = &self.confirm_projection {
            let projection = Rc::clone(projection);
            panel = panel.on_confirm_edit(move |s: &mut S| projection(s));
        }
        if let Some(f) = &self.on_deploy {
            let f = Rc::clone(f);
            panel = panel.on_deploy(move |s: &mut S| f(s));
        }
        panel
    }
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-sm font-medium` (10.5pt, weight 500).
fn type_sm_medium() -> TextStyle {
    TextStyle::new(10.5, 500.0)
}

/// The shared empty edit for an unwired confirm input (the wizard always
/// owns one; this is the builder's neutral default).
fn empty_edit() -> &'static TextEdit {
    static EMPTY: std::sync::OnceLock<TextEdit> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| TextEdit::new(String::new()))
}
