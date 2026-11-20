//! Port of `packages/frontend/src/components/connection/SaveTunnelDialog.tsx`.
//!
//! Names the current inline tunnel configuration and promotes it to a reusable
//! entity. React owns `name` / `submitting`, awaits an injected `onSubmit` and
//! shows a boxed `ErrorBanner` when the host reports a failure. Rune keeps the
//! values in [`SaveTunnelVm`] inside `S` and reports the two intents through
//! [`SaveTunnelAction`] to the injected host callback — no fake IPC or thread.
//!
//! Reopening always starts from a clean form (React's effect on `[open]`);
//! [`SaveTunnelVm::open`] is the explicit equivalent.
//!
//! ## Keys (React `data-testid` preserved)
//!
//! `save-tunnel-dialog` (panel), `save-tunnel-name`, `save-tunnel-error`,
//! `save-tunnel-cancel`, `save-tunnel-confirm`. The error banner keeps the
//! `alert` role and boxed danger surface from `ui::banner`.

use std::rc::Rc;

use rune::{col, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::shared::Label;

/// The dialog's observable state.
#[allow(dead_code)] // VM surface; consumer: the connection form host.
#[derive(Clone, Debug)]
pub(crate) struct SaveTunnelVm {
    pub open: bool,
    pub name: TextEdit,
    /// Local in-flight flag, set while the host persists.
    pub submitting: bool,
    /// Host's `busy` prop.
    pub busy: bool,
    /// Host's `error` prop (already formatted).
    pub error: String,
}

#[allow(dead_code)] // initial-state convenience; wired by the host view.
impl Default for SaveTunnelVm {
    fn default() -> Self {
        Self {
            open: false,
            name: TextEdit::new(String::new()),
            submitting: false,
            busy: false,
            error: String::new(),
        }
    }
}

#[allow(dead_code)] // open/reset contract; wired by the host view.
impl SaveTunnelVm {
    /// React's effect: every open starts from an empty name and idle state.
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.name.replace_text(String::new());
        self.submitting = false;
        self.error.clear();
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.submitting = false;
    }

    /// React's `pending = busy || submitting`.
    pub(crate) fn pending(&self) -> bool {
        self.busy || self.submitting
    }

    /// React's `trimmed`; the guard and the submitted value both use it.
    pub(crate) fn trimmed(&self) -> String {
        self.name.text.trim().to_owned()
    }
}

/// The two user intents. The host performs the save and feeds the answer back
/// via [`SaveTunnelVm::submitting`] / [`SaveTunnelVm::error`].
#[allow(dead_code)] // VM surface; consumer: the connection form host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveTunnelAction {
    Close,
    /// The trimmed tunnel name was submitted.
    Submit(String),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, SaveTunnelAction)>;

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: SaveTunnelAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

fn footer_button<S: 'static>(
    k: &Kit,
    key: &str,
    label: &str,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: SaveTunnelAction,
) -> El {
    let mut button = Button::new(label)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Regular)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(press(on_action, action));
    }
    button.build(k).key(key)
}

/// Builder for the ported dialog.
#[allow(dead_code)] // consumer: the connection form host (not landed).
pub(crate) struct SaveTunnelDialog<'a, S: 'static> {
    vm: &'a SaveTunnelVm,
    on_vm: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut SaveTunnelVm>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: the connection form host (not landed).
impl<'a, S: 'static> SaveTunnelDialog<'a, S> {
    pub(crate) fn new(vm: &'a SaveTunnelVm) -> Self {
        Self {
            vm,
            on_vm: None,
            on_action: None,
        }
    }

    pub(crate) fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut SaveTunnelVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, SaveTunnelAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let vm = self.vm;
        let k = kit(dark);
        let root = col()
            .key("save-tunnel-dialog-root")
            .w_full()
            .h_full();
        if !vm.open {
            return root;
        }

        let pending = vm.pending();
        let trimmed = vm.trimmed();
        let mut name = TextField::new("save-tunnel-name", &vm.name)
            .height(32.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(t("newConn.tunnelNamePlaceholder"));
        if let Some(accessor) = &self.on_vm {
            let accessor = accessor.clone();
            name = name.on_edit(rune::edit_access(move |s: &mut S| &mut accessor(s).name));
        } else {
            name = name.disabled(true);
        }

        let mut body = col()
            .key("save-tunnel-body")
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .child(
                Label::new("save-tunnel-name-label", t("newConn.tunnelName"))
                    .required(true)
                    .build(dark),
            )
            .child(name.build(dark));
        if let Some(banner) = Banner::new("save-tunnel-error", vm.error.clone())
            .variant(BannerVariant::Boxed)
            .build(k)
        {
            body = body.child(banner);
        }

        let confirm_label = if pending {
            t("newConn.tunnelSaving")
        } else {
            t("newConn.tunnelSaveConfirm")
        };
        let mut dialog = Dialog::new("save-tunnel-dialog", t("newConn.tunnelSaveTitle"))
            .description(t("newConn.tunnelSaveDesc"))
            .open(true)
            .fit()
            .child(body)
            .footer(footer_button(
                &k,
                "save-tunnel-cancel",
                t("common.cancel"),
                pending,
                &self.on_action,
                SaveTunnelAction::Close,
            ))
            .footer(footer_button(
                &k,
                "save-tunnel-confirm",
                confirm_label,
                trimmed.is_empty() || pending,
                &self.on_action,
                SaveTunnelAction::Submit(trimmed),
            ));
        if self.on_action.is_some() {
            let close = self.on_action.clone();
            dialog = dialog.on_close(move |s: &mut S| {
                if let Some(cb) = &close {
                    cb(s, SaveTunnelAction::Close);
                }
            });
        }
        dialog.build(&k, layer);
        root
    }
}

// Leftover L1 (removal stage): when the connection form host mounts this dialog,
// bind `on_vm` and translate `SaveTunnelAction::Submit` into the host's
// `saveAsTunnel` command, feeding `Ok(entity)` → close and `Err` → `error`; drop
// the per-item allows then. No IPC is performed here.
