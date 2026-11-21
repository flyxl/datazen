//! The unclosed-transaction confirm dialog (React `txUnclosedOpen` branch).

use rune::{text, El};
use rune_kit::ButtonVariant;

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

use super::{
    emit, footer_button, QueryTransactionHostPort, QueryTransactionModalAction as Action,
};

pub(crate) const UNCLOSED_KEY: &str = "query-tx-unclosed-modal";
pub(crate) const BODY_KEY: &str = "query-tx-unclosed-body";
pub(crate) const CANCEL_KEY: &str = "query-tx-unclosed-cancel";
pub(crate) const CONFIRM_KEY: &str = "query-tx-unclosed-confirm";

/// React: `Dialog` with `query.inTransaction` in the body, a ghost cancel and a
/// primary confirm. "Cancel" is also the close/Escape action.
pub(crate) fn build<S: 'static>(
    host: &QueryTransactionHostPort<S>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    Dialog::new(UNCLOSED_KEY, t("query.txUnclosedTitle"))
        .open(true)
        .fit()
        .description(t("query.txUnclosedBody"))
        .width(480.0)
        .body_max_height(240.0)
        .child(
            text(t("query.inTransaction"))
                .key(BODY_KEY)
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .footer(footer_button(
            CANCEL_KEY,
            "query.txUnclosedCancel",
            ButtonVariant::Ghost,
            false,
            host,
            Action::CancelUnclosed,
            k,
        ))
        .footer(footer_button(
            CONFIRM_KEY,
            "query.txUnclosedConfirm",
            ButtonVariant::Contrast,
            false,
            host,
            Action::ConfirmUnclosed,
            k,
        ))
        .on_close(emit(host, Action::CancelUnclosed))
        .build(k, layer);
}
