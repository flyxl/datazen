mod dom;
mod policy;

#[cfg(test)]
mod tests;

pub use dom::{
    attr, is_body, is_interactive_control, is_native_selectable, is_selectable, is_selectable_hint,
    tag, INTERACTIVE_CONTROL_SELECTOR, NATIVE_SELECTABLE_SELECTOR, SELECTABLE_HINT_SELECTOR,
};
pub use policy::{
    should_prevent_select_all, ContextMenuSuppressor, DragMouseDownInput, DragSelectionGuard,
    RightDragSuppressor, SelectAllTarget, SelectionSnapshot,
};

pub const DOM_GAP_SURFACES: [&str; 6] = [
    "document.addEventListener",
    "document.removeEventListener",
    "window.addEventListener",
    "window.removeEventListener",
    "window.getSelection",
    "getComputedStyle",
];
