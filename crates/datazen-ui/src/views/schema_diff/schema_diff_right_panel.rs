//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffRightPanel.tsx`
//! (T204).
//!
//! The wizard's right-hand review panel: the Plan / Deploy tab strip (the
//! deploy step's review) above a scrolling body that shows the plan panel
//! or the deploy panel for the current tab — or the "generate a plan first"
//! empty state when no plan exists yet. The wizard's step mode hides the
//! tab strip (`hideTabs`) and the deploy step hides the deploy button
//! (`hideDeployButton`).
//!
//! ## What maps where
//!
//! - The props become builder fields and are threaded into the two panel
//!   builders verbatim: [`SchemaDiffPlanPanel`] (T178) and
//!   [`SchemaDiffDeployPanel`] (T025) — including the plan panel's
//!   type-override edit contract (the host-mirrored slots and the
//!   entry-or-insert accessor).
//! - React's `Tabs` with `getTabClassName` styling is the `ui/tabs.rs`
//!   atom (`Tabs::new(key, items, active)`), whose key convention gives
//!   the tabs `schema-diff-tabs-tab-plan` / `-deploy` (derived; React's
//!   per-item testids `schema-diff-plan-tab` / `-deploy-tab` are the
//!   atom's keys here).
//! - `onTabChange(tab)` stays typed: [`SchemaDiffRightPanelTab`] in,
//!   [`SchemaDiffRightPanel::on_tab_change`] out.
//! - The empty state (`min-h-[8rem]` = 96pt) centres
//!   `schemaDiff.generatePlan`, exactly React's two branches.
//! - `flex-[1.4]` → `grow(1.4)`; `bg-surface` → `surface_base`;
//!   `border-edge` → `border_base` (the workspace-navigator convention).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: the root
//! `schema-diff-right-panel` and the two tab bodies `schema-diff-plan-panel`
//! / `schema-diff-deploy-panel`. Derived: the tab strip
//! `schema-diff-tabs` (the atom's `-tablist` / `-tab-{id}` children) and
//! the empty states `schema-diff-right-panel-empty`.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextEdit, TextStyle};

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, SchemaDiffDeployResultVm, SchemaDiffPlanVm,
};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::tabs::{TabItem, Tabs};
use crate::views::schema_diff::schema_diff_deploy_panel::SchemaDiffDeployPanel;
use crate::views::schema_diff::schema_diff_plan_panel::SchemaDiffPlanPanel;

use crate::views::schema_diff::schema_diff_plan_panel::OverrideEditFn;

type ToggleFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type TabFn<S> = Rc<dyn Fn(&mut S, SchemaDiffRightPanelTab) + 'static>;
type ConfirmProjection<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

/// React `SchemaDiffRightPanelTab`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffRightPanelTab {
    Plan,
    Deploy,
}

/// The right review panel builder, generic over the owning state `S`.
pub(crate) struct SchemaDiffRightPanel<'a, S: 'static> {
    active_tab: SchemaDiffRightPanelTab,
    plan: Option<&'a SchemaDiffPlanVm>,
    allow_destructive: bool,
    include_indexes: bool,
    regenerating: bool,
    type_overrides: &'a [ColumnTypeOverrideVm],
    override_edits: Option<&'a HashMap<String, TextEdit>>,
    on_override_edit: Option<OverrideEditFn<S>>,
    target_label: &'a str,
    use_transaction: bool,
    require_rollback: bool,
    confirm_text: &'a TextEdit,
    deploying: bool,
    hide_deploy_button: bool,
    deploy_result: Option<&'a SchemaDiffDeployResultVm>,
    hide_tabs: bool,
    on_tab_change: Option<TabFn<S>>,
    on_allow_destructive: Option<ToggleFn<S>>,
    on_include_indexes: Option<ToggleFn<S>>,
    on_regenerate: Option<ToggleFn<S>>,
    on_apply_type_overrides: Option<ToggleFn<S>>,
    on_use_transaction: Option<ToggleFn<S>>,
    on_require_rollback: Option<ToggleFn<S>>,
    on_deploy: Option<ToggleFn<S>>,
    confirm_projection: Option<ConfirmProjection<S>>,
}

impl<'a, S: 'static> SchemaDiffRightPanel<'a, S> {
    /// React's `activeTab` / `plan` / `allowDestructive` / `includeIndexes`
    /// / `targetLabel` / `useTransaction` / `requireRollback` /
    /// `confirmText` / `deploying` / `deployResult` props.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        active_tab: SchemaDiffRightPanelTab,
        plan: Option<&'a SchemaDiffPlanVm>,
        allow_destructive: bool,
        include_indexes: bool,
        target_label: &'a str,
        use_transaction: bool,
        require_rollback: bool,
        confirm_text: &'a TextEdit,
        deploying: bool,
        deploy_result: Option<&'a SchemaDiffDeployResultVm>,
    ) -> Self {
        Self {
            active_tab,
            plan,
            allow_destructive,
            include_indexes,
            regenerating: false,
            type_overrides: &[],
            override_edits: None,
            on_override_edit: None,
            target_label,
            use_transaction,
            require_rollback,
            confirm_text,
            deploying,
            hide_deploy_button: false,
            deploy_result,
            hide_tabs: false,
            on_tab_change: None,
            on_allow_destructive: None,
            on_include_indexes: None,
            on_regenerate: None,
            on_apply_type_overrides: None,
            on_use_transaction: None,
            on_require_rollback: None,
            on_deploy: None,
            confirm_projection: None,
        }
    }

    /// React's `hideTabs` (default false).
    pub(crate) fn hide_tabs(mut self, hide: bool) -> Self {
        self.hide_tabs = hide;
        self
    }

    /// React's `hideDeployButton` (default false).
    pub(crate) fn hide_deploy_button(mut self, hide: bool) -> Self {
        self.hide_deploy_button = hide;
        self
    }

    /// React's `regenerating`.
    pub(crate) fn regenerating(mut self, regenerating: bool) -> Self {
        self.regenerating = regenerating;
        self
    }

    /// React's `typeOverrides`.
    pub(crate) fn type_overrides(mut self, overrides: &'a [ColumnTypeOverrideVm]) -> Self {
        self.type_overrides = overrides;
        self
    }

    /// The host-mirrored override edit slots (the plan panel's contract).
    pub(crate) fn override_edits(
        mut self,
        edits: Option<&'a HashMap<String, TextEdit>>,
    ) -> Self {
        self.override_edits = edits;
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

    /// React's `onTabChange`.
    pub(crate) fn on_tab_change(
        mut self,
        f: impl Fn(&mut S, SchemaDiffRightPanelTab) + 'static,
    ) -> Self {
        self.on_tab_change = Some(Rc::new(f));
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

    /// React's `onRegenerate`.
    pub(crate) fn on_regenerate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_regenerate = Some(Rc::new(f));
        self
    }

    /// React's `onApplyTypeOverrides`.
    pub(crate) fn on_apply_type_overrides(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_type_overrides = Some(Rc::new(f));
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

    /// The confirm input's edit projection into `S` (React's
    /// `onConfirmTextChange`), the deploy panel's convention.
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

    /// Paint the panel.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut panel = col()
            .key("schema-diff-right-panel")
            .min_w(0.0)
            .min_h(0.0)
            .grow(1.4) // React flex-[1.4]
            .fill(k.c.surface_base); // bg-surface

        // --- The tab strip (React's bordered header row). ---
        let mut header = row()
            .key("schema-diff-right-panel-header")
            .w_full()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        if !self.hide_tabs {
            let mut tabs = Tabs::<S>::new(
                "schema-diff-tabs",
                vec![
                    TabItem::new("plan", t("schemaDiff.stepPlan")),
                    TabItem::new("deploy", t("schemaDiff.stepReview")),
                ],
                match self.active_tab {
                    SchemaDiffRightPanelTab::Plan => "plan",
                    SchemaDiffRightPanelTab::Deploy => "deploy",
                },
            )
            .aria_label(t("common.schemaDiff"));
            if let Some(on_tab_change) = &self.on_tab_change {
                let on_tab_change = Rc::clone(on_tab_change);
                tabs = tabs.on_change(move |s: &mut S, id| {
                    let tab = if id == "deploy" {
                        SchemaDiffRightPanelTab::Deploy
                    } else {
                        SchemaDiffRightPanelTab::Plan
                    };
                    on_tab_change(s, tab);
                });
            }
            header = header.child(tabs.build(dark));
        }
        panel = panel.child(header);

        // --- The scrolling body. ---
        let mut body = col()
            .key("schema-diff-right-panel-body")
            .min_h(0.0)
            .grow(1.0) // React flex-1
            .w_full()
            .padding(12.0) // React p-4
            .scroll(ScrollSpec { stick_bottom: false });

        match self.active_tab {
            SchemaDiffRightPanelTab::Plan => {
                let mut wrapper = col().key("schema-diff-plan-panel").w_full();
                if let Some(plan) = self.plan {
                    wrapper = wrapper.child(self.plan_panel(plan, dark));
                } else {
                    wrapper = wrapper.child(empty_state(dark));
                }
                body = body.child(wrapper);
            }
            SchemaDiffRightPanelTab::Deploy => {
                let mut wrapper = col().key("schema-diff-deploy-panel").w_full();
                if let Some(plan) = self.plan {
                    wrapper = wrapper.child(self.deploy_panel(plan, dark));
                } else {
                    wrapper = wrapper.child(empty_state(dark));
                }
                body = body.child(wrapper);
            }
        }
        panel.child(body)
    }

    /// The plan tab's panel (the T178 builder with the threaded props).
    fn plan_panel(&self, plan: &SchemaDiffPlanVm, dark: bool) -> El {
        let mut panel = SchemaDiffPlanPanel::<S>::new(
            plan,
            self.allow_destructive,
            self.include_indexes,
        )
        .regenerating(self.regenerating)
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
        panel.build(dark)
    }

    /// The deploy tab's panel (the T025 builder with the threaded props).
    fn deploy_panel(&self, plan: &SchemaDiffPlanVm, dark: bool) -> El {
        let mut panel = SchemaDiffDeployPanel::<S>::new(
            "schema-diff-deploy-panel-root",
            plan,
            self.target_label,
            self.use_transaction,
            self.require_rollback,
            self.confirm_text,
            self.deploying,
            self.deploy_result,
        )
        .hide_deploy_button(self.hide_deploy_button);
        if let Some(projection) = &self.confirm_projection {
            let projection = Rc::clone(projection);
            panel = panel.on_confirm_edit(move |s: &mut S| projection(s));
        }
        if let Some(f) = &self.on_use_transaction {
            let f = Rc::clone(f);
            panel = panel.on_use_transaction(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_require_rollback {
            let f = Rc::clone(f);
            panel = panel.on_require_rollback(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_deploy {
            let f = Rc::clone(f);
            panel = panel.on_deploy(move |s: &mut S| f(s));
        }
        panel.build(dark)
    }
}

/// React's empty state: the centred "generate a plan first" hint
/// (`h-full min-h-[8rem]`).
fn empty_state(dark: bool) -> El {
    let k = kit(dark);
    col()
        .key("schema-diff-right-panel-empty")
        .w_full()
        .min_h(96.0) // React min-h-[8rem]
        .grow(1.0)
        .center()
        .child(
            text(t("schemaDiff.generatePlan").to_owned())
                .key("schema-diff-right-panel-empty-text")
                .font(type_sm())
                .color(k.c.text_muted),
        )
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}
