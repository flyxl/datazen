//! The aborted-transaction rollback prompt (React `txAbortedOpen` branch).

use rune::{text, El};
use rune_kit::ButtonVariant;

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

use super::{
    emit, footer_button, QueryTransactionHostPort, QueryTransactionModalAction as Action,
    QueryTransactionModalsVm,
};

pub(crate) const ABORTED_KEY: &str = "query-tx-aborted-modal";
pub(crate) const DETAIL_KEY: &str = "query-tx-aborted-detail";
pub(crate) const SKIP_KEY: &str = "query-tx-aborted-skip";
pub(crate) const ROLLBACK_KEY: &str = "query-tx-aborted-rollback";

/// React: a `<pre>` with the abort detail (danger-coloured, capped height), a
/// ghost Skip and a danger Roll back — both disabled while `txBusy`.
pub(crate) fn build<S: 'static>(
    vm: &QueryTransactionModalsVm,
    host: &QueryTransactionHostPort<S>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut dialog = Dialog::new(ABORTED_KEY, t("query.txAbortedTitle"))
        .open(true)
        .fit()
        .description(t("query.txAbortedBody"))
        .width(520.0)
        .body_max_height(280.0);
    if let Some(detail) = vm.tx_aborted_detail.as_deref() {
        if !detail.is_empty() {
            dialog = dialog.child(
                text(detail.to_owned())
                    .key(DETAIL_KEY)
                    .font(k.ty.mono)
                    .color(k.c.danger.fg)
                    .max_h(160.0)
                    .clip_soft(0.0)
                    .wrap_anywhere(),
            );
        }
    }
    dialog
        .footer(footer_button(
            SKIP_KEY,
            "query.txAbortedSkip",
            ButtonVariant::Ghost,
            vm.tx_busy,
            host,
            Action::AbortedSkip,
            k,
        ))
        .footer(footer_button(
            ROLLBACK_KEY,
            "query.txAbortedRollback",
            ButtonVariant::Danger,
            vm.tx_busy,
            host,
            Action::AbortedRollback,
            k,
        ))
        .on_close(emit(host, Action::AbortedSkip))
        .build(k, layer);
}
