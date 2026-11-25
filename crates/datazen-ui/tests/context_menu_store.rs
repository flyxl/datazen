//! T157: the web context-menu store (`stores/contextMenuStore.ts`) —
//! the singleton the bridge writes to and the host renders from. The
//! vectors mirror the React store test: a separators-only menu never
//! opens, a real menu opens normalized at the given position, and hide
//! closes it.

use std::rc::Rc;

use datazen_ui::components::ui::context_menu_store::{
    hide_web_context_menu, show_web_context_menu, with_context_menu_state,
};
use datazen_ui::components::ui::web_context_menu::NativeMenuItemDef;

fn sep() -> NativeMenuItemDef {
    NativeMenuItemDef::Separator { id: None }
}

fn item(id: &str, label: &str) -> NativeMenuItemDef {
    NativeMenuItemDef::Item {
        id: id.to_owned(),
        label: label.to_owned(),
        shortcut: None,
        enabled: None,
        action: Rc::new(|| {}),
    }
}

#[test]
fn a_menu_that_normalizes_to_empty_is_ignored() {
    show_web_context_menu(vec![sep()], (10.0, 20.0));
    with_context_menu_state(|state| {
        assert!(!state.open, "separators-only stays closed");
        assert!(state.items.is_empty());
    });
    hide_web_context_menu();
}

#[test]
fn a_real_menu_opens_normalized_at_the_position() {
    show_web_context_menu(
        vec![sep(), item("a", "A"), sep()],
        (12.0, 34.0),
    );
    with_context_menu_state(|state| {
        assert!(state.open, "the menu is up");
        assert_eq!((state.x, state.y), (12.0, 34.0), "the cursor position lands verbatim");
        assert_eq!(state.items.len(), 1, "the leading and trailing separators drop");
        assert!(
            matches!(&state.items[0], NativeMenuItemDef::Item { id, label, .. }
                if id == "a" && label == "A"),
            "the surviving item is the plain one: {:?}",
            state.items[0]
        );
    });
    hide_web_context_menu();
}

#[test]
fn hide_closes_the_menu_and_empties_the_items() {
    show_web_context_menu(vec![item("a", "A")], (1.0, 1.0));
    hide_web_context_menu();
    with_context_menu_state(|state| {
        assert!(!state.open);
        assert!(state.items.is_empty());
    });
}

#[test]
fn reopening_replaces_the_items_and_resets_the_submenu() {
    // Open with one item, then a second menu at another position: the
    // store holds the second menu, not a mix.
    show_web_context_menu(
        vec![
            item("a", "A"),
            NativeMenuItemDef::Submenu {
                id: None,
                label: "More".to_owned(),
                items: vec![item("b", "B")],
            },
        ],
        (5.0, 6.0),
    );
    show_web_context_menu(vec![item("c", "C")], (7.0, 8.0));
    with_context_menu_state(|state| {
        assert_eq!(state.items.len(), 1);
        assert!(
            matches!(&state.items[0], NativeMenuItemDef::Item { id, .. } if id == "c"),
            "the reopen replaces the items, not appends"
        );
        assert_eq!((state.x, state.y), (7.0, 8.0));
        assert!(state.submenu.is_none(), "the submenu closes on reopen");
    });
    hide_web_context_menu();
}
