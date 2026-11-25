//! The QueryPanel shell and its keyed regions: the editor column, the optional
//! results host, the sidebars and the session-disconnect banner. Each region is
//! an honest keyed slot; the query-panel host fills it with the already-ported
//! child views. Execute/cancel are rendered here because the panel owns their
//! routing decision.

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_PLAY, ICON_X};
use crate::ui::kit::kit;

use super::{
    CancelAvailability, QueryPanelAction, QueryPanelActionFn, QueryPanelVm, EDITOR_KEY, EXECUTE_KEY,
    NEW_SESSION_KEY, RESULTS_HOST_KEY, ROOT_KEY, SESSION_BANNER_KEY, SIDEBAR_KEY, STOP_KEY,
};

/// Build the panel root and its layout regions into `layer`.
#[allow(dead_code)] // region builder; remove when the workspace wires the panel host.
pub fn build<S: 'static>(
    vm: &QueryPanelVm,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    // The running flag and execution sequence are observable on the root's
    // help text, mirroring React's `data-query-running` / `data-execution-seq`.
    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_h(0.0)
        .min_w(0.0)
        .gap(0.0)
        .help(format!("seq={};running={}", vm.execution_seq, vm.running));

    if vm.session_disconnected() {
        root = root.child(session_banner::<S>(on_action, dark));
    }

    let mut editor_column = col()
        .key("query-panel-editor-column")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(action_bar::<S>(vm, on_action, dark))
        .child(
            row()
                .key(EDITOR_KEY)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0),
        );
    if !vm.results_idle() {
        editor_column = editor_column.child(
            col()
                .key(RESULTS_HOST_KEY)
                .w_full()
                .min_h(0.0)
                .min_w(0.0)
                .shrink(),
        );
    }

    let body = row()
        .key("query-panel-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(editor_column)
        .child(
            row()
                .key(SIDEBAR_KEY)
                .h_full()
                .min_w(0.0)
                .fill(k.c.layer_02),
        );
    layer.push(root.child(body));
}

fn action_bar<S: 'static>(
    vm: &QueryPanelVm,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let pane = vm.focused_pane_id.clone();
    let control = match vm.cancel_availability() {
        CancelAvailability::Execute => {
            let mut button = Button::new(t("query.execute"))
                .icon(&ICON_PLAY)
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Small)
                .build(k)
                .key(EXECUTE_KEY)
                .label(t("query.execute"));
            if let Some(callback) = on_action.clone() {
                let pane = pane.clone();
                button = button.on_press(move |s: &mut S| {
                    callback(
                        s,
                        QueryPanelAction::Execute {
                            pane: pane.clone(),
                        },
                    )
                });
            }
            button
        }
        availability => {
            let disabled = availability == CancelAvailability::StopDisabled;
            let label = if vm.cancel_capability.is_none() {
                t("query.cancelUnknown")
            } else if vm.cancel_capability == Some(false) {
                t("query.cancelUnavailable")
            } else {
                t("query.stop")
            };
            let mut button = Button::new(label.clone())
                .icon(&ICON_X)
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Small)
                .disabled(disabled)
                .build(k)
                .key(STOP_KEY)
                .label(label);
            if !disabled {
                if let Some(callback) = on_action.clone() {
                    let pane = pane.clone();
                    button = button.on_press(move |s: &mut S| {
                        callback(
                            s,
                            QueryPanelAction::Cancel {
                                pane: pane.clone(),
                            },
                        )
                    });
                }
            }
            button
        }
    };
    row()
        .key("query-panel-action-bar")
        .w_full()
        .min_w(0.0)
        .h(36.0)
        .center_y()
        .justify_end()
        .px(12.0)
        .child(control)
}

fn session_banner<S: 'static>(
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut button = Button::new(t("query.session.openNew"))
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .build(k)
        .key(NEW_SESSION_KEY);
    let callback = on_action.clone();
    if let Some(callback) = callback {
        button = button.on_press(move |s: &mut S| callback(s, QueryPanelAction::OpenNewSession));
    }
    row()
        .key(SESSION_BANNER_KEY)
        .role("status")
        .w_full()
        .gap(8.0)
        .center_y()
        .px(12.0)
        .py(8.0)
        .child(
            text(t("query.session.disconnected"))
                .key("query-panel-session-text")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .child(button)
}
