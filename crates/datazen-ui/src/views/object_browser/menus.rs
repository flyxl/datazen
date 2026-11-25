//! The two native context menus of `ObjectBrowser`.
//!
//! React builds them with `buildObjectBrowserListMenuItems` (refresh / open /
//! copy-name / copy-ddl) and `buildObjectBrowserEditorMenuItems` (OS edit items,
//! a separator, then execute / format / comment). The OS edit items are a host
//! concern; the three view-owned entries and the exact i18n labels are ported
//! here. Every entry emits an [`ObjectBrowserAction`] through the same host
//! port as the rest of the view — no fake clipboard or command call.

use rune::{el, El, PointerPress};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::menu::{context_menu, Item};

use super::{
    emit, BrowserObject, ObjectBrowserAction, ObjectBrowserActionFn, ObjectBrowserVm,
    EDITOR_MENU_KEY, LIST_MENU_KEY,
};

fn trigger(key: &str) -> El {
    el().key(format!("{key}-trigger")).size(0.0, 0.0).a11y_hidden()
}

/// React's routine-list menu, anchored at the secondary press point.
#[allow(dead_code)] // menu builder; remove when the panel host wires the view.
pub fn build_list_menu<S: 'static>(
    object: &BrowserObject,
    press: PointerPress,
    on_action: &Option<ObjectBrowserActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let refresh = emit(
        on_action,
        ObjectBrowserAction::Refresh { kind: object.kind },
    );
    let open = emit(
        on_action,
        ObjectBrowserAction::OpenObject {
            object: object.clone(),
        },
    );
    let copy_name = emit(
        on_action,
        ObjectBrowserAction::CopyName {
            name: object.name.clone(),
        },
    );
    let copy_ddl = emit(
        on_action,
        ObjectBrowserAction::CopyDdl {
            object: object.clone(),
        },
    );
    let close = emit(on_action, ObjectBrowserAction::CloseMenu);
    let menu = context_menu::<S>(LIST_MENU_KEY, trigger(LIST_MENU_KEY), Some(press))
        .min_width(160.0)
        .item(Item::new(t("objects.refresh")).on_select(refresh))
        .item(Item::new(t("objects.open")).on_select(open))
        .item(Item::new(t("common.copyName")).on_select(copy_name))
        .item(Item::new(t("common.copyDdl")).on_select(copy_ddl))
        .on_close(close);
    let _ = menu.build(k, layer);
}

/// React's DDL-editor menu: a separator, then execute / format / comment.
#[allow(dead_code)] // menu builder; remove when the panel host wires the view.
pub fn build_editor_menu<S: 'static>(
    vm: &ObjectBrowserVm,
    press: PointerPress,
    on_action: &Option<ObjectBrowserActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let sql = vm.ddl.text.clone();
    let schema = vm.selected.as_ref().and_then(|object| object.schema.clone());
    let execute = {
        let callback = on_action.clone();
        move |s: &mut S| {
            if let Some(callback) = &callback {
                callback(
                    s,
                    ObjectBrowserAction::Execute {
                        sql: sql.clone(),
                        schema: schema.clone(),
                    },
                );
            }
        }
    };
    let menu = context_menu::<S>(EDITOR_MENU_KEY, trigger(EDITOR_MENU_KEY), Some(press))
        .min_width(160.0)
        .separator()
        .item(Item::new(t("query.execute")).on_select(execute))
        .item(Item::new(t("query.format")).on_select(emit(
            on_action,
            ObjectBrowserAction::FormatDdl,
        )))
        .item(Item::new(t("query.comment")).on_select(emit(
            on_action,
            ObjectBrowserAction::ToggleComment,
        )))
        .on_close(emit(on_action, ObjectBrowserAction::CloseMenu));
    let _ = menu.build(k, layer);
}
