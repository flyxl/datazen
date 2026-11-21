use crate::DomElement;

use super::*;

fn el(attrs: Vec<(&str, &str)>) -> DomElement {
    DomElement::leaf(attrs)
}

fn child(attrs: Vec<(&str, &str)>, parent: DomElement) -> DomElement {
    DomElement::child_of(attrs, parent)
}

#[test]
fn dom_gap_surfaces_are_pinned() {
    assert_eq!(DOM_GAP_SURFACES.len(), 6);
    assert!(DOM_GAP_SURFACES.contains(&"document.addEventListener"));
    assert!(DOM_GAP_SURFACES.contains(&"window.getSelection"));
    assert!(DOM_GAP_SURFACES.contains(&"getComputedStyle"));
}

#[test]
fn selector_constants_keep_their_lists() {
    assert!(NATIVE_SELECTABLE_SELECTOR.contains("textarea"));
    assert!(NATIVE_SELECTABLE_SELECTOR.contains(".cm-editor"));
    assert!(INTERACTIVE_CONTROL_SELECTOR.contains("[role=\"switch\"]"));
    assert!(SELECTABLE_HINT_SELECTOR.contains("[data-testid*=\"error\"]"));
}

#[test]
fn select_all_requires_a_modifier_and_the_a_key() {
    assert!(!should_prevent_select_all(false, false, "a", SelectAllTarget::NotElement));
    assert!(!should_prevent_select_all(false, false, "b", SelectAllTarget::NotElement));
    assert!(!should_prevent_select_all(true, false, "b", SelectAllTarget::Element(el(vec![("tag", "div")]))));
}

#[test]
fn select_all_is_blocked_outside_native_controls() {
    assert!(should_prevent_select_all(true, false, "A", SelectAllTarget::NotElement));
    assert!(should_prevent_select_all(false, true, "a", SelectAllTarget::NotElement));
    assert!(should_prevent_select_all(true, false, "a", SelectAllTarget::Element(el(vec![("tag", "div")]))));
    assert!(should_prevent_select_all(
        true,
        false,
        "a",
        SelectAllTarget::Element(child(vec![("tag", "span")], el(vec![("tag", "div")])))
    ));
}

#[test]
fn select_all_is_allowed_inside_native_controls() {
    for attrs in [
        vec![("tag", "input")],
        vec![("tag", "textarea")],
        vec![("tag", "select")],
        vec![("class", "cm-editor")],
        vec![("contenteditable", "true")],
        vec![("contenteditable", "plaintext-only")],
    ] {
        assert!(!should_prevent_select_all(true, false, "a", SelectAllTarget::Element(el(attrs))));
    }
    let editor = el(vec![("tag", "div"), ("class", "cm-editor")]);
    let inner = child(vec![("tag", "span")], editor);
    assert!(!should_prevent_select_all(true, false, "a", SelectAllTarget::Element(inner)));
}

#[test]
fn interactive_control_and_selectable_hint_match_through_ancestors() {
    let button = el(vec![("tag", "button")]);
    assert!(is_interactive_control(&button));
    let child_of_button = child(vec![("tag", "span")], button);
    assert!(is_interactive_control(&child_of_button));

    for attrs in [
        vec![("class", "selectable")],
        vec![("tag", "pre")],
        vec![("role", "alert")],
        vec![("data-error", "")],
        vec![("data-testid", "field-error")],
        vec![("aria-invalid", "true")],
    ] {
        assert!(is_selectable_hint(&el(attrs)));
    }
    let hint = el(vec![("class", "selectable")]);
    assert!(is_selectable_hint(&child(vec![("tag", "span")], hint)));
}

#[test]
fn selectable_walk_honors_computed_style_and_body_boundary() {
    assert!(!is_selectable(None));
    let text = el(vec![("tag", "div"), ("userSelect", "text")]);
    assert!(is_selectable(Some(&text)));
    let ancestor = el(vec![("tag", "div"), ("userSelect", "text")]);
    let leaf = child(vec![("tag", "span")], ancestor);
    assert!(is_selectable(Some(&leaf)));
    assert!(!is_selectable(Some(&el(vec![("tag", "div"), ("userSelect", "none")]))));
    assert!(!is_selectable(Some(&el(vec![("tag", "button"), ("userSelect", "text")]))));

    let body = el(vec![("tag", "body"), ("userSelect", "none")]);
    let outside = child(vec![("tag", "div")], body);
    let beyond = child(vec![("tag", "span")], outside);
    assert!(!is_selectable(Some(&beyond)));
}

#[test]
fn right_drag_suppressor_tracks_right_button_only() {
    let mut suppressor = RightDragSuppressor::new();
    assert!(!suppressor.on_mouse_move(2));
    suppressor.on_mouse_down(0);
    assert!(!suppressor.on_mouse_move(2));
    suppressor.on_mouse_down(2);
    assert!(suppressor.is_right_down());
    assert!(suppressor.on_mouse_move(2));
    assert!(suppressor.on_mouse_move(3));
    assert!(!suppressor.on_mouse_move(1));
    suppressor.on_mouse_up(0);
    assert!(suppressor.on_mouse_move(2));
    suppressor.on_mouse_up(2);
    assert!(!suppressor.on_mouse_move(2));
    suppressor.on_mouse_down(2);
    suppressor.on_blur();
    assert!(!suppressor.on_mouse_move(2));
}

#[test]
fn context_menu_suppressor_only_acts_in_release_builds() {
    let dev = ContextMenuSuppressor::new(true);
    assert!(dev.is_dev());
    assert!(!dev.should_prevent());
    let release = ContextMenuSuppressor::new(false);
    assert!(release.should_prevent());
}

#[test]
fn drag_guard_ignores_non_left_buttons_and_second_clicks() {
    let mut guard = DragSelectionGuard::new();
    let input = DragMouseDownInput {
        button: 2,
        detail: 1,
        selection: Some(SelectionSnapshot {
            collapsed: false,
            range_count: 1,
            range_intersects_target: true,
        }),
        target: Some(el(vec![("tag", "div")])),
    };
    assert!(!guard.on_mouse_down(input));
    assert!(!guard.has_press());
    assert!(!guard.on_mouse_move(1));

    let input = DragMouseDownInput {
        button: 0,
        detail: 2,
        selection: Some(SelectionSnapshot {
            collapsed: false,
            range_count: 1,
            range_intersects_target: true,
        }),
        target: Some(el(vec![("tag", "div")])),
    };
    assert!(!guard.on_mouse_down(input));
    assert!(guard.has_press());
}

#[test]
fn drag_guard_preserves_a_selection_intersecting_the_pressed_target() {
    let mut guard = DragSelectionGuard::new();
    let input = DragMouseDownInput {
        button: 0,
        detail: 1,
        selection: Some(SelectionSnapshot {
            collapsed: false,
            range_count: 1,
            range_intersects_target: true,
        }),
        target: Some(el(vec![("tag", "div"), ("class", "selectable")])),
    };
    assert!(guard.on_mouse_down(input));

    let mut guard = DragSelectionGuard::new();
    let collapsed = DragMouseDownInput {
        button: 0,
        detail: 1,
        selection: Some(SelectionSnapshot {
            collapsed: true,
            range_count: 1,
            range_intersects_target: true,
        }),
        target: Some(el(vec![("tag", "div")])),
    };
    assert!(!guard.on_mouse_down(collapsed));

    let mut guard = DragSelectionGuard::new();
    let no_range = DragMouseDownInput {
        button: 0,
        detail: 1,
        selection: Some(SelectionSnapshot {
            collapsed: false,
            range_count: 0,
            range_intersects_target: true,
        }),
        target: Some(el(vec![("tag", "div")])),
    };
    assert!(!guard.on_mouse_down(no_range));
}

#[test]
fn drag_guard_clears_only_stray_drags() {
    let mut guard = DragSelectionGuard::new();
    guard.on_mouse_down(DragMouseDownInput {
        button: 0,
        detail: 1,
        selection: None,
        target: Some(el(vec![("tag", "div")])),
    });
    assert!(guard.on_mouse_move(1));
    assert!(!guard.on_mouse_move(2));
    guard.on_mouse_up();
    assert!(!guard.on_mouse_move(1));

    let mut guard = DragSelectionGuard::new();
    guard.on_mouse_down(DragMouseDownInput {
        button: 0,
        detail: 1,
        selection: None,
        target: Some(el(vec![("tag", "pre")])),
    });
    assert!(!guard.on_mouse_move(1));
    guard.on_blur();
    assert!(!guard.on_mouse_move(1));
}
