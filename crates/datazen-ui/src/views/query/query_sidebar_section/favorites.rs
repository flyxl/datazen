//! The favorites sidebar of `QuerySidebarSection`: a file-first list with a
//! rescan button, the sync-folder hint, empty state, per-row open/delete and a
//! native context menu. Every command goes through the shared host port.

use std::rc::Rc;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::icons::{ICON_REFRESH, ICON_TRASH};
use crate::ui::kit::kit;

use super::{
    bottom_border, icon_button, FavoriteVm, QuerySidebarAction, QuerySidebarActionFn,
    SidebarSectionVm, FAVORITES_PANEL_KEY, FAVORITES_REFRESH_KEY, FAVORITES_ROOT_KEY,
};

/// Build the favorites aside into `layer`.
#[allow(dead_code)] // panel builder; remove when the query host wires the sidebar.
pub(crate) fn build<S: 'static>(
    vm: &SidebarSectionVm,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let refresh = icon_button::<S>(
        FAVORITES_REFRESH_KEY,
        t("query.favoritesRefresh"),
        &ICON_REFRESH,
        on_action.clone().map(|callback| {
            Rc::new(move |s: &mut S| callback(s, QuerySidebarAction::RefreshFavorites))
                as Rc<dyn Fn(&mut S)>
        }),
        dark,
    );
    let header = row()
        .key("query-favorites-header")
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .justify_between()
        .px(12.0)
        .py(8.0)
        .child(
            text(t("query.favoritesTitle"))
                .key("query-favorites-title")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
        .child(refresh);

    let mut aside = col()
        .key(FAVORITES_PANEL_KEY)
        .w(256.0)
        .shrink()
        .min_h(0.0)
        .fill(k.c.layer_02)
        .child(header)
        .child(bottom_border(dark));

    if let Some(root) = vm.favorites_root.as_deref() {
        aside = aside.child(
            row()
                .key(FAVORITES_ROOT_KEY)
                .w_full()
                .min_w(0.0)
                .gap(4.0)
                .px(12.0)
                .py(6.0)
                .child(
                    text(t("query.favoritesRoot"))
                        .key("query-favorites-root-label")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(
                    text(root.to_owned())
                        .key("query-favorites-root-path")
                        .font(k.ty.mono)
                        .color(k.c.text_muted)
                        .wrap_anywhere()
                        .min_w(0.0),
                ),
        );
    }

    if vm.favorites.is_empty() {
        aside = aside.child(
            row()
                .key("query-no-favorites")
                .w_full()
                .px(12.0)
                .py(16.0)
                .child(
                    text(t("query.noFavorites"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
    }
    for (at, favorite) in vm.favorites.iter().enumerate() {
        aside = aside.child(favorite_row(favorite, at, on_action, dark));
    }
    layer.push(aside);
}

fn favorite_row<S: 'static>(
    favorite: &FavoriteVm,
    at: usize,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut open = col()
        .key(format!("favorite-row-{at}"))
        .role("button")
        .focusable(true)
        .w_full()
        .min_w(0.0)
        .grow(1.0)
        .gap(2.0)
        .px(12.0)
        .py(8.0)
        .child(
            text(favorite.title.clone())
                .key(format!("favorite-title-{at}"))
                .font(k.ty.text_small)
                .color(k.c.text_base)
                .truncate()
                .w_full()
                .min_w(0.0),
        )
        .child(
            text(favorite.sql.clone())
                .key(format!("favorite-sql-{at}"))
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .truncate()
                .w_full()
                .min_w(0.0),
        );
    if let Some(callback) = on_action.clone() {
        let fav = favorite.clone();
        open = open.on_press(move |s: &mut S| {
            callback(s, QuerySidebarAction::OpenFavorite(fav.clone()));
        });
    }

    let delete = icon_button::<S>(
        &format!("favorite-delete-{at}"),
        t("common.delete"),
        &ICON_TRASH,
        on_action.clone().map(|callback| {
            let id = favorite.id.clone();
            Rc::new(move |s: &mut S| callback(s, QuerySidebarAction::DeleteFavorite(id.clone())))
                as Rc<dyn Fn(&mut S)>
        }),
        dark,
    );

    let mut row_el = row()
        .key(format!("favorite-item-{at}"))
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .center_y()
        .px(4.0)
        .hover(k.c.surface_raised)
        .child(open)
        .child(delete);
    if let Some(callback) = on_action.clone() {
        let fav = favorite.clone();
        row_el = row_el.on_secondary_press(move |s, press, _tx| {
            callback(
                s,
                QuerySidebarAction::FavoriteContextMenu {
                    favorite: fav.clone(),
                    press,
                },
            );
        });
    }
    row_el
}
