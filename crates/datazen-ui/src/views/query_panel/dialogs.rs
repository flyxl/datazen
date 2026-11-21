//! The dialogs QueryPanel hosts: the result-message dialog (success/error),
//! the dangerous-SQL and transaction-switch confirmations, and the strategy
//! ask. React routes each through its own hook/modal; here every choice is an
//! injected [`QueryPanelAction`].

use rune::{row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

use super::{
    MessageKind, QueryPanelAction, QueryPanelActionFn, QueryPanelVm, MESSAGE_DIALOG_KEY,
    MESSAGE_OK_KEY,
};

/// Build all open dialogs into `layer`.
#[allow(dead_code)] // dialog builder; remove when the workspace wires the panel host.
pub(crate) fn build<S: 'static>(
    vm: &QueryPanelVm,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    if vm.message.open {
        message_dialog::<S>(vm, on_action, dark, layer);
    }
    if vm.dangerous_confirm_open {
        confirm::<S>(
            "query-panel-dangerous-confirm",
            t("query.execute"),
            QueryPanelAction::ConfirmDangerous,
            QueryPanelAction::CancelDangerous,
            on_action,
            dark,
            layer,
        );
    }
    if vm.switch_tx_confirm_open {
        confirm::<S>(
            "query-panel-switch-tx-confirm",
            t("query.session.transactionSwitchConfirm"),
            QueryPanelAction::ConfirmSwitchTx,
            QueryPanelAction::CancelSwitchTx,
            on_action,
            dark,
            layer,
        );
    }
    if vm.strategy_ask_open {
        strategy_ask::<S>(on_action, dark, layer);
    }
}

fn message_dialog<S: 'static>(
    vm: &QueryPanelVm,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let title = match vm.message.kind {
        MessageKind::Error => t("common.error"),
        MessageKind::Success => t("common.success"),
    };
    let mut dialog = Dialog::new(MESSAGE_DIALOG_KEY, title)
        .open(true)
        .width(440.0)
        .child(
            row()
                .key("result-message-text")
                .w_full()
                .min_w(0.0)
                .child(
                    text(vm.message.text.clone())
                        .key("result-message-text-label")
                        .font(k.ty.text_small)
                        .color(k.c.text_base)
                        .wrap_anywhere(),
                ),
        );
    // React's error dialog also offers a copy action; success hides it. The
    // clipboard write and the 1500ms feedback revert are host concerns.
    if vm.message.kind == MessageKind::Error {
        let label = if vm.message_copied {
            t("common.copied")
        } else {
            t("common.copy")
        };
        let mut copy = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .build(k)
            .key("result-message-copy")
            .help(t("common.copy"));
        if let Some(callback) = on_action.clone() {
            let text = vm.message.text.clone();
            copy = copy.on_press(move |s: &mut S| {
                callback(s, QueryPanelAction::CopyMessage { text: text.clone() })
            });
        }
        dialog = dialog.footer(copy);
    }
    let callback = on_action.clone();
    let mut ok = Button::new(t("common.ok"))
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Regular)
        .build(k)
        .key(MESSAGE_OK_KEY);
    if let Some(callback) = callback {
        ok = ok.on_press(move |s: &mut S| callback(s, QueryPanelAction::CloseMessage));
    }
    dialog = dialog.footer(ok);
    if let Some(callback) = on_action.clone() {
        dialog = dialog.on_close(move |s: &mut S| callback(s, QueryPanelAction::CloseMessage));
    }
    dialog.build(k, layer);
}

fn confirm<S: 'static>(
    key: &str,
    label: &str,
    confirm_action: QueryPanelAction,
    cancel_action: QueryPanelAction,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut confirm = Button::new(label.to_owned())
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .build(k)
        .key(format!("{key}-confirm"));
    if let Some(callback) = on_action.clone() {
        confirm = confirm.on_press(move |s: &mut S| callback(s, confirm_action.clone()));
    }
    let mut cancel = Button::new(t("common.cancel"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key(format!("{key}-cancel"));
    let cancel_for_press = cancel_action.clone();
    if let Some(callback) = on_action.clone() {
        cancel = cancel.on_press(move |s: &mut S| callback(s, cancel_for_press.clone()));
    }
    let mut dialog = Dialog::new(key, label)
        .open(true)
        .width(440.0)
        .footer(confirm)
        .footer(cancel);
    if let Some(callback) = on_action.clone() {
        let dismiss = cancel_action.clone();
        dialog = dialog.on_close(move |s: &mut S| callback(s, dismiss.clone()));
    }
    dialog.build(k, layer);
}

fn strategy_ask<S: 'static>(
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut current = Button::new(t("query.executionStrategy.runCurrent"))
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .build(k)
        .key("query-panel-strategy-current");
    let mut entire = Button::new(t("query.executionStrategy.runEntire"))
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .build(k)
        .key("query-panel-strategy-entire");
    if let Some(callback) = on_action.clone() {
        current = current.on_press(move |s: &mut S| {
            callback(s, QueryPanelAction::ChooseStrategy { entire_script: false })
        });
    }
    if let Some(callback) = on_action.clone() {
        entire = entire.on_press(move |s: &mut S| {
            callback(s, QueryPanelAction::ChooseStrategy { entire_script: true })
        });
    }
    let mut dialog = Dialog::new("query-panel-strategy-ask", t("query.executionStrategy.askTitle"))
        .open(true)
        .width(448.0)
        .child(current)
        .child(entire);
    if let Some(callback) = on_action.clone() {
        dialog = dialog.on_close(move |s: &mut S| {
            callback(s, QueryPanelAction::ChooseStrategy { entire_script: false })
        });
    }
    dialog.build(k, layer);
}
