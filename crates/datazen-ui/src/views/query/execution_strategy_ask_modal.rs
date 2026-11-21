//! Port of `windows/connection/query/ExecutionStrategyAskModal.tsx`.
//!
//! When a script holds several statements React asks which range to run: an
//! optional "current statement" card and an "entire script" card, each showing
//! a short SQL preview, plus a cancel button. Both cards are `role="button"`
//! with Enter/Space activation in React. This port keeps the same two choices
//! and routes `ExecuteCurrent` / `ExecuteEntire` / `Cancel` through an injected
//! host port — no fake IPC, script execution, timer or thread.
//!
//! ## Keys
//!
//! The dialog panel is `execution-strategy-ask-modal`; the description, the two
//! option cards and their label/preview rows, and the cancel button carry Rune
//! observation keys named after the React elements. The dialog base adds its
//! own `-title` / `-body` / `-footer` keys.
//!
//! ## Local stand-ins (removal stage)
//!
//! `StatementTargetVm` mirrors the few `StatementTargetInfo` fields the modal
//! reads; preview truncation and the two interpolated labels are pure ports of
//! their React helpers. No production host mounts this modal yet, so every
//! reusable item carries a per-item `#[allow(dead_code)]` with the removal stage
//! "drop when the SQL execution host wires the strategy ask".

use std::rc::Rc;

use rune::{col, row, text, El, Key, KeyEvent, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

pub(crate) const DIALOG_KEY: &str = "execution-strategy-ask-modal";
pub(crate) const DESC_KEY: &str = "execution-strategy-ask-desc";
pub(crate) const CURRENT_KEY: &str = "execution-strategy-run-current";
pub(crate) const ENTIRE_KEY: &str = "execution-strategy-run-entire";
pub(crate) const CANCEL_KEY: &str = "execution-strategy-cancel";

/// React `StatementTargetInfo`, reduced to what the modal reads.
#[allow(dead_code)] // VM mirror; remove when the execution host owns the target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatementTargetVm {
    pub(crate) sql: String,
    pub(crate) from_line: u32,
    pub(crate) to_line: u32,
}

/// React `ExecutionStrategyAskModalProps` as a cloneable VM.
#[allow(dead_code)] // VM mirror; remove when the execution host owns the ask.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExecutionStrategyAskVm {
    pub(crate) open: bool,
    pub(crate) current_statement: Option<StatementTargetVm>,
    pub(crate) statement_count: u32,
    pub(crate) entire_script: String,
}

/// React's `sql.length > 120 ? sql.slice(0, 120) + '…' : sql`. Rune counts
/// scalar chars rather than UTF-16 units; the preview is display-only.
#[allow(dead_code)] // pure helper; remove when the React helper is ported.
pub(crate) fn preview(sql: &str) -> String {
    if sql.chars().count() > 120 {
        let head: String = sql.chars().take(120).collect();
        format!("{head}…")
    } else {
        sql.to_owned()
    }
}

/// React `t('query.executionStrategy.runCurrent', { from, to })`.
#[allow(dead_code)] // pure helper; remove with the host.
pub(crate) fn current_label(vm: &ExecutionStrategyAskVm) -> String {
    match &vm.current_statement {
        Some(current) => t("query.executionStrategy.runCurrent")
            .replace("{from}", &current.from_line.to_string())
            .replace("{to}", &current.to_line.to_string()),
        None => String::new(),
    }
}

/// React `t('query.executionStrategy.runEntire', { count })`.
#[allow(dead_code)] // pure helper; remove with the host.
pub(crate) fn entire_label(vm: &ExecutionStrategyAskVm) -> String {
    t("query.executionStrategy.runEntire").replace("{count}", &vm.statement_count.to_string())
}

/// The three React callbacks, forwarded to the host.
#[allow(dead_code)] // host port; remove when the execution host wires the modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionStrategyAction {
    ExecuteCurrent,
    ExecuteEntire,
    Cancel,
}

#[allow(dead_code)] // host port; remove when the execution host wires the modal.
pub(crate) type ExecutionStrategyActionFn<S> = Rc<dyn Fn(&mut S, ExecutionStrategyAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ExecutionStrategyActionFn<S>>,
    action: ExecutionStrategyAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action);
        }
    }
}

/// A keyed, focusable option card with React's click and Enter/Space handlers.
fn option_card<S: 'static>(
    key: &str,
    label: String,
    sql_preview: String,
    action: ExecutionStrategyAction,
    on_action: &Option<ExecutionStrategyActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut card = col()
        .key(key.to_owned())
        .role("button")
        .focusable(true)
        .w_full()
        .min_w(0.0)
        .gap(6.0)
        .padding(12.0)
        .radius(8.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .hover(k.c.surface_raised)
        .child(
            text(label)
                .key(format!("{key}-label"))
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
        .child(
            text(sql_preview)
                .key(format!("{key}-preview"))
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .max_h(80.0)
                .clip_soft(0.0)
                .wrap_anywhere(),
        );
    if let Some(on_action) = on_action.clone() {
        let press = on_action.clone();
        card = card.on_press(move |s: &mut S| press(s, action));
        let keyed = on_action;
        card = card.on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            let space = matches!(&event.key, Key::Text(value) if value.as_str() == " ");
            if event.key == Key::Enter || space {
                keyed(s, action);
                true
            } else {
                false
            }
        });
    }
    card
}

/// Render the modal into `layer`. A closed React `Dialog` mounts nothing.
#[allow(dead_code)] // view entry; remove when the execution host wires it.
pub(crate) fn build<S: 'static>(
    vm: &ExecutionStrategyAskVm,
    on_action: &Option<ExecutionStrategyActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    if !vm.open {
        return;
    }
    let k = kit(dark);

    let mut body = col()
        .key("execution-strategy-ask-body-inner")
        .w_full()
        .min_w(0.0)
        .gap(16.0)
        .py(8.0)
        .child(
            text(t("query.executionStrategy.askDesc"))
                .key(DESC_KEY)
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
    if let Some(current) = &vm.current_statement {
        body = body.child(option_card::<S>(
            CURRENT_KEY,
            current_label(vm),
            preview(&current.sql),
            ExecutionStrategyAction::ExecuteCurrent,
            on_action,
            dark,
        ));
    }
    body = body.child(option_card::<S>(
        ENTIRE_KEY,
        entire_label(vm),
        preview(&vm.entire_script),
        ExecutionStrategyAction::ExecuteEntire,
        on_action,
        dark,
    ));

    let cancel = Button::new(t("common.cancel"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key(CANCEL_KEY)
        .on_press(emit(on_action, ExecutionStrategyAction::Cancel));
    body = body.child(
        row()
            .key("execution-strategy-cancel-row")
            .w_full()
            .min_w(0.0)
            .justify_end()
            .child(cancel),
    );

    Dialog::new(
        DIALOG_KEY,
        t("query.executionStrategy.askTitle"),
    )
    .open(true)
    .width(448.0)
    .body_max_height(360.0)
    .body_pad((16.0, 16.0))
    .child(body)
    .on_close(emit(on_action, ExecutionStrategyAction::Cancel))
    .build(k, layer);
}
