//! T203: `windows/data-sync/DataSyncOptionsStep.tsx` ported to Rune.
//!
//! The wizard's second step: a titled card that mounts the
//! already-ported [`OptionsBar`] (T348) with the step's own callbacks.
//! The React source is a 28-line wrapper, and the port keeps exactly
//! that shape: the card chrome, the title block, the bar, and the
//! pass-through wiring (including the bar's conflict-select open slot,
//! which the wizard owns one level up).
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `data-sync-options-step` |
//! | title (internal) | `…-title` |
//! | hint (internal) | `…-hint` |
//!
//! ## Documented deviations from React
//!
//! * **`space-y-4`** becomes `gap(16.0)`; the card is `rounded-lg` with
//!   the kit `lg` radius, the `surface_weak` fill and 16pt padding
//!   (`p-4`).
//! * The bar itself keeps its own keys (`data-sync-*` family, T348).

// The production consumer is the Data Sync window (T268); until it wires
// this in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T268)

use std::rc::Rc;

use rune::{col, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::{OptionsBar, SyncOptions};

/// The step builder. `S` is the host state that owns the options.
pub(crate) struct DataSyncOptionsStep<S: 'static> {
    options: SyncOptions,
    policy_select_open: bool,
    on_change: Option<Rc<dyn Fn(&mut S, SyncOptions)>>,
    on_enable_delete: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_policy: Option<Rc<dyn Fn(&mut S)>>,
    on_close_policy: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> DataSyncOptionsStep<S> {
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

    /// React `onOptionsChange(next)`.
    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, SyncOptions) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onEnableDelete`.
    pub(crate) fn on_enable_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_enable_delete = Some(Rc::new(f));
        self
    }

    /// The conflict-policy select's open flag, from the host's slot (the
    /// bar needs it; the wizard owns it one level up).
    pub(crate) fn policy_select_open(mut self, open: bool) -> Self {
        self.policy_select_open = open;
        self
    }

    /// The host records the conflict select opening.
    pub(crate) fn on_toggle_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_policy = Some(Rc::new(f));
        self
    }

    /// The host closes the conflict select.
    pub(crate) fn on_close_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_policy = Some(Rc::new(f));
        self
    }

    /// The step on its own kit root.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(&k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The same step without its kit root (for a parent that roots the
    /// overlay once).
    pub(crate) fn build_inner(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut bar = OptionsBar::<S>::new(self.options);
        bar = bar.policy_select_open(self.policy_select_open);
        if let Some(on_change) = &self.on_change {
            let on_change = on_change.clone();
            bar = bar.on_change(move |s, next| on_change(s, next));
        }
        if let Some(on_enable_delete) = &self.on_enable_delete {
            let on_enable_delete = on_enable_delete.clone();
            bar = bar.on_enable_delete(move |s| on_enable_delete(s));
        }
        if let Some(on_toggle_policy) = &self.on_toggle_policy {
            let on_toggle_policy = on_toggle_policy.clone();
            bar = bar.on_toggle_policy(move |s| on_toggle_policy(s));
        }
        if let Some(on_close_policy) = &self.on_close_policy {
            let on_close_policy = on_close_policy.clone();
            bar = bar.on_close_policy(move |s| on_close_policy(s));
        }
        col()
            .key("data-sync-options-step")
            .w_full()
            .gap(16.0)
            .radius(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak)
            .padding(16.0)
            .child(
                col()
                    .key("data-sync-options-step-title-block")
                    .child(
                        text(t("sync.optionsTitle"))
                            .key("data-sync-options-step-title")
                            .font(k.ty.text_small)
                            .color(k.c.text_base),
                    )
                    .child(
                        text(t("sync.optionsHint"))
                            .key("data-sync-options-step-hint")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            )
            .child(bar.build_inner(k, layer))
    }
}
