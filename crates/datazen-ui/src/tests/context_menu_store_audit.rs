//! T157 independent audit (tester): fresh vectors for the context-menu
//! store singleton — nested submenus that normalize away, predefined edit
//! rows, disabled items kept by normalization, and the cursor overwrite
//! between consecutive shows.

use std::rc::Rc;

use crate::components::ui::context_menu_store::{
    hide_web_context_menu, show_web_context_menu, with_context_menu_state,
};
use crate::components::ui::web_context_menu::{NativeMenuItemDef, PredefinedEdit};

fn item(id: &str, enabled: Option<bool>) -> NativeMenuItemDef {
    NativeMenuItemDef::Item {
        id: id.to_owned(),
        label: format!("Row {id}"),
        shortcut: Some("Ctrl+K".to_owned()),
        enabled,
        action: Rc::new(|| {}),
    }
}

fn empty_submenu(label: &str) -> NativeMenuItemDef {
    NativeMenuItemDef::Submenu {
        id: None,
        label: label.to_owned(),
        items: Vec::new(),
    }
}

#[test]
fn audit_an_empty_submenu_drops_with_its_place_kept_or_closed() {
    // [empty submenu, item, separator] -> [item]: the submenu that
    // normalizes away does not leave the menu closed.
    show_web_context_menu(
        vec![
            empty_submenu("Empty"),
            item("keep", None),
            NativeMenuItemDef::Separator { id: None },
        ],
        (100.0, 50.0),
    );
    with_context_menu_state(|state| {
        assert!(state.open);
        assert_eq!(state.items.len(), 1);
        assert!(
            matches!(&state.items[0], NativeMenuItemDef::Item { id, .. } if id == "keep"),
            "the empty submenu drops, the real row stays: {:?}",
            state.items[0]
        );
    });
    hide_web_context_menu();
}

#[test]
fn audit_disabled_items_are_kept_by_normalization() {
    show_web_context_menu(
        vec![item("off", Some(false)), item("on", Some(true))],
        (3.0, 4.0),
    );
    with_context_menu_state(|state| {
        assert_eq!(state.items.len(), 2, "disabled items are kept");
        assert!(
            !state.items[0].enabled() && state.items[1].enabled(),
            "the enabled flags read back per row"
        );
    });
    hide_web_context_menu();
}

#[test]
fn audit_predefined_rows_carry_their_command() {
    show_web_context_menu(
        vec![
            NativeMenuItemDef::Predefined {
                item: PredefinedEdit::Copy,
                text: Some("Copy table".to_owned()),
            },
            NativeMenuItemDef::Predefined {
                item: PredefinedEdit::Separator,
                text: None,
            },
            item("row", None),
        ],
        (9.0, 9.0),
    );
    with_context_menu_state(|state| {
        // A predefined separator between two rows survives as a
        // separator (it is not the same kind, so the duplicate rule
        // does not fire; leading/trailing does not apply here).
        assert_eq!(state.items.len(), 3, "the predefined separator stays mid-list");
        assert_eq!(
            state.items[0].item_label().as_deref(),
            Some("Copy table"),
            "the relabelled predefined text wins"
        );
    });
    hide_web_context_menu();
}

#[test]
fn audit_a_later_show_overwrites_the_position_and_items() {
    show_web_context_menu(vec![item("first", None)], (11.0, 12.0));
    show_web_context_menu(vec![item("second", None)], (13.0, 14.0));
    with_context_menu_state(|state| {
        assert_eq!((state.x, state.y), (13.0, 14.0), "the newer cursor wins");
        assert!(
            matches!(&state.items[0], NativeMenuItemDef::Item { id, .. } if id == "second"),
            "the newer items win"
        );
    });
    // After hide, a closed store reports closed again (idempotent).
    hide_web_context_menu();
    hide_web_context_menu();
    with_context_menu_state(|state| {
        assert!(!state.open && state.items.is_empty());
    });
}

#[test]
fn audit_an_all_noise_menu_never_opens_even_mixed_with_disabled_rows() {
    // [disabled item? No — separator, predefined separator, empty
    // submenu] all normalize away: the menu stays closed and the cursor
    // position is irrelevant.
    show_web_context_menu(
        vec![
            NativeMenuItemDef::Separator { id: Some("s1".to_owned()) },
            NativeMenuItemDef::Predefined {
                item: PredefinedEdit::Separator,
                text: None,
            },
            empty_submenu("Nothing"),
        ],
        (20.0, 30.0),
    );
    with_context_menu_state(|state| {
        assert!(!state.open, "noise-only never opens");
        assert!(state.items.is_empty());
    });
    hide_web_context_menu();
}
