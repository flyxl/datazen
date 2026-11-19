//! Rune ports of `packages/frontend/src/components/ui/**`.
//!
//! | React source | Rune module |
//! |---|---|
//! | `WebContextMenu.tsx` (T096) | `web_context_menu` |
//! | `stores/contextMenuStore.ts` (T157) | `context_menu_store` |

pub(crate) mod context_menu_store;
pub(crate) mod web_context_menu;

pub(crate) use web_context_menu::{
    is_separator, normalize_native_menu_items, position_root_menu, position_submenu,
    CONTEXT_MENU_PAD, NativeMenuItemDef, OpenSubmenu, PredefinedEdit, SubmenuSide,
    WebContextMenuHost, WebContextMenuState,
};
