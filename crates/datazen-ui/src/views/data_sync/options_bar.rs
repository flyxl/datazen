//! T348: `windows/data-sync/OptionsBar.tsx` ported to Rune.
//!
//! The sync options bar: the three operation checkboxes (insert / update
//! / delete), the conflict-policy select with its skip / force warnings,
//! and the hint. The delete checkbox is special in the React source and
//! stays special here: checking it never flips the option directly — it
//! asks the host to run the enable-delete confirmation (the Data Sync
//! dialogs' delete-confirm, T074); only the confirmation writes
//! `delete: true`. Unchecking is immediate (`delete: false`).
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key, verbatim; the rest are
//! internal:
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `data-sync-options-bar` |
//! | title span (internal) | `…-title` |
//! | insert checkbox (React test id) | `data-sync-option-insert` |
//! | update checkbox (React test id) | `data-sync-option-update` |
//! | delete checkbox (React test id) | `data-sync-option-delete` |
//! | conflict select trigger (React test id) | `data-sync-conflict-policy-trigger` |
//! | policy warning span (internal) | `…-policy-warning` |
//! | hint span (internal) | `…-hint` |
//!
//! ## Documented deviations from React
//!
//! * **`flex flex-wrap items-center gap-4`** becomes a wrapping row with
//!   `gap(16.0)`; `px-6 py-2` are 24/8pt; `border-b` rides on the root
//!   (the wizard mounts the bar under its own edge line, exactly as the
//!   React class list says).
//! * **`text-amber-600 dark:text-amber-400`** becomes the kit warning
//!   foreground — the crate's amber mapping (T073 / T148 precedent), with
//!   no new color literal.
//! * **The kit `Checkbox` reports its next state** (checked → toggle
//!   callback), which is what React's `e.target.checked` reads.

// The production consumers are the Data Sync options step (T203) and the
// compare step (T024's own bar slot); until they wire it in, the
// component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T203/T268)

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;
use crate::views::{SyncConflictPolicy, SyncOptions};

/// The builder. `S` is the host state that owns the options.
pub(crate) struct OptionsBar<S: 'static> {
    options: SyncOptions,
    policy_select_open: bool,
    on_change: Option<Rc<dyn Fn(&mut S, SyncOptions)>>,
    on_enable_delete: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_policy: Option<Rc<dyn Fn(&mut S)>>,
    on_close_policy: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> OptionsBar<S> {
    pub(crate) fn new(options: SyncOptions) -> Self {
        Self {
            options,
            policy_select_open: false,
            on_change: None,
            on_enable_delete: None,
            on_toggle_policy: None,
            on_close_policy: None,
        }
    }

    /// The conflict-policy select's open flag, from the host's slot.
    pub(crate) fn policy_select_open(mut self, open: bool) -> Self {
        self.policy_select_open = open;
        self
    }

    /// The host records the select opening.
    pub(crate) fn on_toggle_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_policy = Some(Rc::new(f));
        self
    }

    /// The host closes the select.
    pub(crate) fn on_close_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_policy = Some(Rc::new(f));
        self
    }

    /// React `onChange(next)`.
    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, SyncOptions) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onEnableDelete`: the confirmation the delete checkbox asks
    /// for instead of flipping the option itself.
    pub(crate) fn on_enable_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_enable_delete = Some(Rc::new(f));
        self
    }

    /// The bar on its own kit root.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(&k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The same bar without its kit root (for a parent that roots the
    /// overlay once).
    pub(crate) fn build_inner(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut bar = row()
            .key("data-sync-options-bar")
            .w_full()
            .flex_wrap()
            .items_center()
            .gap(16.0)
            .px(24.0)
            .py(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(t("sync.optionsTitle"))
                    .key("data-sync-options-bar-title")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted),
            )
            .child(self.option_checkbox(
                k,
                "data-sync-option-insert",
                t("sync.optionInsert"),
                self.options.insert,
                |options, checked| SyncOptions {
                    insert: checked,
                    ..options
                },
            ))
            .child(self.option_checkbox(
                k,
                "data-sync-option-update",
                t("sync.optionUpdate"),
                self.options.update,
                |options, checked| SyncOptions {
                    update: checked,
                    ..options
                },
            ))
            .child(self.delete_checkbox(k))
            .child(self.conflict_policy_row(k, layer));
        // The warning rides next to the select, exactly as React's
        // sibling span does.
        if let Some(warning) = policy_warning(&self.options) {
            bar = bar.child(
                text(warning)
                    .key("data-sync-options-bar-policy-warning")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }
        bar.child(
            text(t("sync.optionsHint"))
                .key("data-sync-options-bar-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        )
    }

    /// One plain operation checkbox: `label flex cursor-pointer items-center
    /// gap-2 text-sm`.
    fn option_checkbox(
        &self,
        k: &Kit,
        key: &str,
        label: &'static str,
        checked: bool,
        patch: fn(SyncOptions, bool) -> SyncOptions,
    ) -> El {
        let mut checkbox = Checkbox::new(key, checked);
        if let Some(on_change) = &self.on_change {
            let on_change = on_change.clone();
            let options = self.options.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                on_change(s, patch(options.clone(), !checked));
            });
        }
        row()
            .key(format!("{key}-label"))
            .items_center()
            .gap(8.0)
            .child(checkbox.build(k))
            .child(
                text(label)
                    .font(k.ty.text_small)
                    .color(k.c.text_base),
            )
    }

    /// The delete checkbox: checking asks for the confirmation; unchecking
    /// writes `delete: false` directly.
    fn delete_checkbox(&self, k: &Kit) -> El {
        let key = "data-sync-option-delete";
        let mut checkbox = Checkbox::new(key, self.options.delete);
        if self.options.delete {
            // Unchecking is the only direct write.
            if let Some(on_change) = &self.on_change {
                let on_change = on_change.clone();
                let options = self.options.clone();
                checkbox = checkbox.on_toggle(move |s: &mut S| {
                    on_change(
                        s,
                        SyncOptions {
                            delete: false,
                            ..options
                        },
                    );
                });
            }
        } else if let Some(on_enable_delete) = &self.on_enable_delete {
            // Checking asks the host to run the confirmation.
            let on_enable_delete = on_enable_delete.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| on_enable_delete(s));
        }
        row()
            .key(format!("{key}-label"))
            .items_center()
            .gap(8.0)
            .child(checkbox.build(k))
            .child(
                text(t("sync.optionDelete"))
                    .font(k.ty.text_small)
                    .color(k.c.text_base),
            )
    }

    /// The conflict-policy select and its label.
    fn conflict_policy_row(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let labels = [
            t("sync.conflictPolicyAbort"),
            t("sync.conflictPolicySkip"),
            t("sync.conflictPolicyForce"),
        ];
        let refs: Vec<&str> = labels.iter().map(|s| *s).collect();
        let current = match self.options.conflict_policy.unwrap_or_default() {
            SyncConflictPolicy::Abort => labels[0],
            SyncConflictPolicy::Skip => labels[1],
            SyncConflictPolicy::Force => labels[2],
        };
        let mut select = Select::new("data-sync-conflict-policy", &refs)
            .width(112.0)
            .open(self.policy_select_open)
            .current(Some(current));
        if let Some(on_toggle_policy) = &self.on_toggle_policy {
            let on_toggle_policy = on_toggle_policy.clone();
            select = select.on_toggle(move |s: &mut S| on_toggle_policy(s));
        }
        if let Some(on_close_policy) = &self.on_close_policy {
            let on_close_policy = on_close_policy.clone();
            select = select.on_close(move |s: &mut S| on_close_policy(s));
        }
        if let Some(on_change) = &self.on_change {
            let on_change = on_change.clone();
            let options = self.options.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                let policy = if value == t("sync.conflictPolicySkip") {
                    SyncConflictPolicy::Skip
                } else if value == t("sync.conflictPolicyForce") {
                    SyncConflictPolicy::Force
                } else {
                    SyncConflictPolicy::Abort
                };
                on_change(
                    s,
                    SyncOptions {
                        conflict_policy: Some(policy),
                        ..options
                    },
                );
            });
        }
        row()
            .key("data-sync-conflict-policy-label")
            .items_center()
            .gap(8.0)
            .child(
                text(t("sync.conflictPolicyLabel"))
                    .font(k.ty.text_small)
                    .color(k.c.text_base),
            )
            .child(select.build(k, layer))
    }
}

/// The policy warning text, `None` for abort or unset.
fn policy_warning(options: &SyncOptions) -> Option<&'static str> {
    match options.conflict_policy {
        Some(SyncConflictPolicy::Skip) => Some(t("sync.conflictPolicySkipWarning")),
        Some(SyncConflictPolicy::Force) => Some(t("sync.conflictPolicyForceWarning")),
        _ => None,
    }
}
