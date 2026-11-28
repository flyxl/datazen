//! The favorite-name dialog (React `FavoriteNameDialog`).

use rune::{col, text, El, Tx};
use rune_kit::ButtonVariant;

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::{
    can_save_favorite, emit, footer_button, FavoriteNameDialogVm, QueryTransactionHostPort,
    QueryTransactionModalAction as Action,
};

pub const FAVORITE_KEY: &str = "query-favorite-dialog";
pub const TITLE_INPUT_KEY: &str = "query-favorite-title-input";
pub const SQL_KEY: &str = "query-favorite-sql";
pub const CANCEL_KEY: &str = "query-favorite-cancel";
pub const SAVE_KEY: &str = "query-favorite-save";

/// React: title input (`data-testid="query-favorite-title-input"`) with
/// Enter-to-save, a read-only SQL block, ghost cancel and a Save button that is
/// disabled while the title is blank.
pub fn build<S: 'static>(
    favorite: &FavoriteNameDialogVm,
    host: &QueryTransactionHostPort<S>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    build_live_favorite(favorite, host, 0, false, dark, layer);
}

pub(crate) fn build_live_favorite<S: 'static>(
    favorite: &FavoriteNameDialogVm,
    host: &QueryTransactionHostPort<S>,
    epoch: u64,
    save_failed: bool,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut field = TextField::new(TITLE_INPUT_KEY, &favorite.favorite_name)
        .height(32.0)
        .placeholder(t("query.favoriteTitlePlaceholder"));
    if let Some(accessor) = host.favorite_name_edit.clone() {
        let edit_accessor = accessor.clone();
        let submit_action = emit(host, Action::FavoriteSave);
        field = field
            .on_edit(rune::edit_access(move |s: &mut S| edit_accessor(s)))
            .on_submit(move |s: &mut S, tx: &mut Tx| {
                if !tx.key_repeat() && can_save_favorite(accessor(s).text.as_str()) {
                    submit_action(s);
                }
            });
    }
    let disabled = !can_save_favorite(favorite.favorite_name.text.as_str());

    let body = col()
        .key("query-favorite-dialog-body")
        .w_full()
        .min_w(0.0)
        .gap(10.0)
        .child(
            text(t("query.favoriteTitle"))
                .key("query-favorite-title-label")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .child(field.build(dark))
        .children(save_failed.then(|| text(t("common.operationFailed"))
            .key("query-favorite-save-error").color(k.c.danger.fg).wrap()))
        .child(
            text("SQL")
                .key("query-favorite-sql-label")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .child(
            text(favorite.favorite_dialog_sql.clone())
                .key(SQL_KEY)
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .max_h(120.0)
                .clip_soft(0.0)
                .wrap_anywhere(),
        );

    Dialog::new(FAVORITE_KEY, t("common.addToFavorites"))
        .open(true)
        .open_epoch(epoch)
        .fit()
        .width(448.0)
        .body_max_height(280.0)
        .child(body)
        .footer(footer_button(
            CANCEL_KEY,
            "common.cancel",
            ButtonVariant::Ghost,
            false,
            host,
            Action::FavoriteCancel,
            k,
        ))
        .footer(footer_button(
            SAVE_KEY,
            "common.save",
            ButtonVariant::Contrast,
            disabled,
            host,
            Action::FavoriteSave,
            k,
        ))
        .on_close(emit(host, Action::FavoriteCancel))
        .build(k, layer);
}
