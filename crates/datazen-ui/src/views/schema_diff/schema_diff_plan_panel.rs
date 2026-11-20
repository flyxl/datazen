//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffPlanPanel.tsx`
//! (T178).
//!
//! The reviewed plan's left-hand content: the options row (the
//! allow-destructive and include-indexes checkboxes plus the underlined
//! Regenerate link), the cross-dialect note, the type-suggestions notice,
//! the warnings box, the dialect/statements line, the requirements section,
//! the statement cards and the rollback status line.
//!
//! ## What maps where
//!
//! - The props become builder fields; `plan` is the [`SchemaDiffPlanVm`]
//!   projection from `commands/schema_diff` (T377), whose
//!   `rollbackCompletenessCounts` powers the status line.
//! - The three internal components live as submodules:
//!   [`requirements`] (`PlanRequirements`), [`type_suggestions`]
//!   (`TypeSuggestionsNotice`, with the derived-override-input contract)
//!   and [`rollback_status`] (`PlanRollbackStatus`).
//! - The Regenerate link keeps its PERSISTENT underline (React's
//!   `text-accent underline`) via a text span decoration, and disables
//!   with the kit's disabled treatment while `regenerating`.
//! - The warnings box (`border-warning/40 bg-surface-alt`): the warning
//!   title, then the bullet list (the `list-disc` bullets render as
//!   stacked rows — the same documented delta as the deploy panel's error
//!   list).
//! - Statement cards: `text-[11px]` mono SQL, `text-[10px]` uppercase
//!   risk chips through [`risk_color`] (additive/success,
//!   destructive/danger, rewrite/warning), keys `{i}-{summary}` verbatim.
//! - The empty plan line distinguishes warning-only plans
//!   (`emptyPlanSkipped`) from clean ones (`emptyPlanNoDiff`).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim:
//! `schema-diff-allow-destructive`, `schema-diff-include-indexes`,
//! `schema-diff-empty-plan` (and the submodules' testids). Derived keys:
//! the options row and its labels, the regenerate link, the warnings box,
//! the dialect line, each statement card `schema-diff-plan-statement-{i}`
//! (React keys by `{i}-{summary}` — kept in the card's key verbatim) and
//! the statements list.
//!
//! Deltas: pixel classes → points (×0.75); the hover underline on the
//! Regenerate link is persistent here (React underlines always, so no
//! change); the bullets lose their glyph (above).

mod requirements;
mod rollback_status;
mod type_suggestions;

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, Span, TextEdit, TextStyle};

use crate::commands::schema_diff::wire::{ColumnTypeOverrideVm, SchemaDiffPlanVm, TypeSuggestionVm};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

pub(crate) use type_suggestions::COMMON_MYSQL_TEXT_TYPES;

use requirements::plan_requirements;
use rollback_status::plan_rollback_status;
use type_suggestions::TypeSuggestionsNotice;

type ToggleFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The host's per-suggestion edit slot: `(table, column, initial)` → the
/// mutable [`TextEdit`] the input binds to (entry-or-insert). See the
/// type-suggestions submodule for the derived-value contract.
pub(crate) type OverrideEditFn<S> = Rc<
    dyn for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit,
>;

/// The plan panel builder, generic over the owning state `S`.
pub(crate) struct SchemaDiffPlanPanel<'a, S: 'static> {
    plan: &'a SchemaDiffPlanVm,
    allow_destructive: bool,
    include_indexes: bool,
    regenerating: bool,
    type_overrides: &'a [ColumnTypeOverrideVm],
    override_edits: Option<&'a std::collections::HashMap<String, TextEdit>>,
    on_override_edit: Option<OverrideEditFn<S>>,
    on_allow_destructive: Option<ToggleFn<S>>,
    on_include_indexes: Option<ToggleFn<S>>,
    on_regenerate: Option<ToggleFn<S>>,
    on_apply_type_overrides: Option<ToggleFn<S>>,
}

impl<'a, S: 'static> SchemaDiffPlanPanel<'a, S> {
    /// React's `plan` / `allowDestructive` / `includeIndexes` props.
    pub(crate) fn new(plan: &'a SchemaDiffPlanVm, allow_destructive: bool, include_indexes: bool) -> Self {
        Self {
            plan,
            allow_destructive,
            include_indexes,
            regenerating: false,
            type_overrides: &[],
            override_edits: None,
            on_override_edit: None,
            on_allow_destructive: None,
            on_include_indexes: None,
            on_regenerate: None,
            on_apply_type_overrides: None,
        }
    }

    /// React's `regenerating` (the Regenerate link's disabled state and
    /// the Apply button's).
    pub(crate) fn regenerating(mut self, regenerating: bool) -> Self {
        self.regenerating = regenerating;
        self
    }

    /// React's `typeOverrides`.
    pub(crate) fn type_overrides(mut self, overrides: &'a [ColumnTypeOverrideVm]) -> Self {
        self.type_overrides = overrides;
        self
    }

    /// The host-mirrored override edit slots (see the type-suggestions
    /// submodule).
    pub(crate) fn override_edits(
        mut self,
        edits: Option<&'a std::collections::HashMap<String, TextEdit>>,
    ) -> Self {
        self.override_edits = edits;
        self
    }

    /// React's `onTypeOverrideChange` landing (the per-suggestion edit
    /// slot accessor).
    pub(crate) fn on_override_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_override_edit = Some(Rc::new(f));
        self
    }

    /// React's `onAllowDestructiveChange` (the host flips its own flag —
    /// the new value is the flipped one).
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

    /// Paint the panel (React's `space-y-3` column).
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut panel = col()
            .key("schema-diff-plan")
            .w_full()
            .gap(9.0) // React space-y-3
            .font(type_sm());

        // --- The options row. ---
        panel = panel.child(self.options_row(dark));

        // --- The cross-dialect note. ---
        if !self.plan.same_dialect {
            panel = panel.child(
                text(t("schemaDiff.crossDialectNote").to_owned())
                    .key("schema-diff-cross-dialect-note")
                    .font(type_sm())
                    .color(k.c.warning.fg),
            );
        }

        // --- The type-suggestions notice. ---
        let suggestions: &[TypeSuggestionVm] = match &self.plan.type_suggestions {
            Some(suggestions) if !suggestions.is_empty() => suggestions,
            _ => &[],
        };
        if !suggestions.is_empty() {
            if let Some(notice) = self
                .suggestions_notice(suggestions)
                .build(dark)
            {
                panel = panel.child(notice);
            }
        }

        // --- The warnings box. ---
        if !self.plan.warnings.is_empty() {
            let mut warnings = col()
                .key("schema-diff-plan-warnings")
                .w_full()
                .gap(2.0)
                .padding(6.0) // React p-2
                .radius(3.0) // React rounded
                .border(0.75, k.alpha(k.c.warning.fg, 0.40)) // border-warning/40
                .fill(k.c.layer_01) // bg-surface-alt
                .font(type_xs())
                .child(
                    text(t("schemaDiff.warnings").to_owned())
                        .key("schema-diff-plan-warnings-title")
                        .font(type_xs_medium())
                        .color(k.c.warning.fg)
                        .mb(3.0), // React mb-1
                );
            for warning in &self.plan.warnings {
                warnings = warnings.child(
                    text(warning.clone())
                        .key(format!("schema-diff-plan-warning-{warning}"))
                        .color(k.c.text_muted),
                );
            }
            panel = panel.child(warnings);
        }

        // --- The dialect/statements line. ---
        panel = panel.child(
            text(format!(
                "{} → {} · {} {}",
                self.plan.source_dialect,
                self.plan.target_dialect,
                self.plan.statements.len(),
                t("schemaDiff.statements")
            ))
            .key("schema-diff-plan-dialects")
            .font(type_xs())
            .color(k.c.text_muted),
        );

        // --- The requirements section. ---
        if let Some(section) = plan_requirements(&self.plan.requirements, dark) {
            panel = panel.child(section);
        }

        // --- The statement cards (or the empty-plan line). ---
        let mut statements = col()
            .key("schema-diff-plan-statements")
            .w_full()
            .gap(6.0) // React space-y-2
            .max_h(48.0) // React max-h-64
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false }); // overflow-auto
        if self.plan.statements.is_empty() {
            statements = statements.child(
                col()
                    .key("schema-diff-empty-plan")
                    .w_full()
                    .pl(9.0)
                    .pr(9.0)
                    .pt(6.0)
                    .pb(6.0) // React px-3 py-2
                    .radius(3.0)
                    .border(0.75, k.c.border_base)
                    .fill(k.c.layer_01)
                    .child(
                        text(
                            if self.plan.warnings.is_empty() {
                                t("schemaDiff.emptyPlanNoDiff").to_owned()
                            } else {
                                t("schemaDiff.emptyPlanSkipped").to_owned()
                            },
                        )
                        .key("schema-diff-empty-plan-text")
                        .font(type_sm())
                        .color(k.c.text_muted),
                    ),
            );
        } else {
            for (index, stmt) in self.plan.statements.iter().enumerate() {
                statements = statements.child(statement_card(index, stmt, dark));
            }
        }
        panel = panel.child(statements);

        // --- The rollback status line. ---
        if let Some(status) = plan_rollback_status(self.plan, dark) {
            panel = panel.child(status);
        }
        panel
    }

    /// The options row: two labeled checkboxes and the Regenerate link.
    fn options_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut options = row()
            .key("schema-diff-plan-options")
            .w_full()
            .flex_wrap() // React flex-wrap
            .center_y()
            .gap(12.0) // React gap-4
            .font(type_sm());

        let mut allow = Checkbox::new("schema-diff-allow-destructive", self.allow_destructive);
        if let Some(on_allow) = &self.on_allow_destructive {
            let on_allow = Rc::clone(on_allow);
            allow = allow.on_toggle(move |s: &mut S| on_allow(s));
        }
        options = options.child(
            row()
                .key("schema-diff-allow-destructive-label")
                .gap(6.0) // React gap-2
                .center_y()
                .child(allow.build(k))
                .child(
                    text(t("schemaDiff.allowDestructive").to_owned())
                        .key("schema-diff-allow-destructive-text")
                        .font(type_sm())
                        .color(k.c.text_base),
                ),
        );

        let mut include = Checkbox::new("schema-diff-include-indexes", self.include_indexes);
        if let Some(on_include) = &self.on_include_indexes {
            let on_include = Rc::clone(on_include);
            include = include.on_toggle(move |s: &mut S| on_include(s));
        }
        options = options.child(
            row()
                .key("schema-diff-include-indexes-label")
                .gap(6.0)
                .center_y()
                .child(include.build(k))
                .child(
                    text(t("schemaDiff.includeIndexes").to_owned())
                        .key("schema-diff-include-indexes-text")
                        .font(type_sm())
                        .color(k.c.text_base),
                ),
        );

        // The Regenerate link: accent ink, persistent underline.
        let label = t("schemaDiff.regeneratePlan");
        let mut regenerate = row()
            .key("schema-diff-regenerate")
            .role("button")
            .disabled(self.regenerating)
            .child(
                text(label.to_owned())
                    .key("schema-diff-regenerate-text")
                    .font(type_sm())
                    .color(k.c.text_accent)
                    .spans(vec![Span::new(0, label.len()).underline(k.c.text_accent)]),
            );
        if !self.regenerating {
            if let Some(on_regenerate) = &self.on_regenerate {
                let on_regenerate = Rc::clone(on_regenerate);
                regenerate = regenerate.on_press(move |s: &mut S| on_regenerate(s));
            }
        }
        options.child(regenerate)
    }

    /// The type-suggestions notice wiring (React's `TypeSuggestionsNotice`
    /// props).
    fn suggestions_notice(&self, suggestions: &'a [TypeSuggestionVm]) -> TypeSuggestionsNotice<'a, S> {
        let mut notice = TypeSuggestionsNotice::new(suggestions, self.type_overrides)
            .regenerating(self.regenerating);
        if let Some(edits) = self.override_edits {
            notice = notice.override_edits(Some(edits));
        }
        if let Some(accessor) = &self.on_override_edit {
            let accessor = Rc::clone(accessor);
            // Re-wrap through the plain-fn bound the builder takes.
            notice = notice.on_override_edit(move |s, table, column, initial| {
                accessor(s, table, column, initial)
            });
        }
        if let Some(on_apply) = &self.on_apply_type_overrides {
            let on_apply = Rc::clone(on_apply);
            notice = notice.on_apply(move |s: &mut S| on_apply(s));
        }
        notice
    }
}

/// One statement card: the risk chip + summary header, then the mono SQL.
fn statement_card(
    index: usize,
    stmt: &crate::commands::schema_diff::wire::PlanStatementVm,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut card = col()
        .key(format!("{index}-{}", stmt.summary)) // React key verbatim
        .w_full()
        .gap(3.0)
        .padding(6.0) // React p-2
        .radius(3.0) // React rounded
        .border(0.75, k.c.border_base)
        .fill(k.c.layer_01) // bg-surface-alt
        .font(TextStyle::new(8.25, 400.0).mono()); // font-mono text-[11px]
    card = card.child(
        row()
            .key(format!("{index}-{}-header", stmt.summary))
            .gap(6.0) // React gap-2
            .center_y()
            .mb(3.0) // React mb-1
            .child(
                text(stmt.risk.wire_str().to_uppercase())
                    .key(format!("{index}-{}-risk", stmt.summary))
                    .font(TextStyle::new(7.5, 400.0)) // font-sans text-[10px] uppercase
                    .color(requirements::risk_color(stmt.risk, dark)),
            )
            .child(
                text(stmt.summary.clone())
                    .key(format!("{index}-{}-summary", stmt.summary))
                    .font(TextStyle::new(8.25, 400.0)) // font-sans on the header
                    .color(k.c.text_base),
            ),
    );
    card.child(
        text(stmt.sql.clone())
            .key(format!("{index}-{}-sql", stmt.summary))
            .font(TextStyle::new(8.25, 400.0).mono())
            .color(k.c.text_muted)
            .wrap(), // whitespace-pre-wrap
    )
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}

/// `text-xs font-medium` (9pt, weight 500).
fn type_xs_medium() -> TextStyle {
    TextStyle::new(9.0, 500.0)
}
