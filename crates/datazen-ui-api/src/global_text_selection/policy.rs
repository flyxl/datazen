use crate::DomElement;

use super::dom::{is_native_selectable, is_selectable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectAllTarget {
    NotElement,
    Element(DomElement),
}

pub fn should_prevent_select_all(meta: bool, ctrl: bool, key: &str, target: SelectAllTarget) -> bool {
    if !(meta || ctrl) || key.to_lowercase() != "a" {
        return false;
    }
    match target {
        SelectAllTarget::NotElement => true,
        SelectAllTarget::Element(element) => !is_native_selectable(&element),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RightDragSuppressor {
    right_down: bool,
}

impl RightDragSuppressor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_mouse_down(&mut self, button: u32) {
        if button == 2 {
            self.right_down = true;
        }
    }

    pub fn on_mouse_move(&self, buttons: u32) -> bool {
        self.right_down && (buttons & 2) != 0
    }

    pub fn on_mouse_up(&mut self, button: u32) {
        if button == 2 {
            self.right_down = false;
        }
    }

    pub fn on_blur(&mut self) {
        self.right_down = false;
    }

    pub fn is_right_down(&self) -> bool {
        self.right_down
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextMenuSuppressor {
    dev: bool,
}

impl ContextMenuSuppressor {
    pub fn new(dev: bool) -> Self {
        Self { dev }
    }

    pub fn should_prevent(&self) -> bool {
        !self.dev
    }

    pub fn is_dev(&self) -> bool {
        self.dev
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionSnapshot {
    pub collapsed: bool,
    pub range_count: usize,
    pub range_intersects_target: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragMouseDownInput {
    pub button: u32,
    pub detail: u32,
    pub selection: Option<SelectionSnapshot>,
    pub target: Option<DomElement>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PressStart {
    selectable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragSelectionGuard {
    press_start: Option<PressStart>,
}

impl DragSelectionGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_mouse_down(&mut self, input: DragMouseDownInput) -> bool {
        if input.button != 0 {
            return false;
        }
        let mut prevent_default = false;
        if input.detail == 1 {
            if let (Some(selection), Some(target)) = (input.selection.as_ref(), input.target.as_ref()) {
                if !selection.collapsed
                    && selection.range_count > 0
                    && selection.range_intersects_target
                {
                    prevent_default = true;
                }
            }
        }
        let selectable = is_selectable(input.target.as_ref());
        self.press_start = Some(PressStart { selectable });
        prevent_default
    }

    pub fn on_mouse_move(&self, buttons: u32) -> bool {
        match self.press_start {
            Some(press) if (buttons & 1) != 0 => !press.selectable,
            _ => false,
        }
    }

    pub fn on_mouse_up(&mut self) {
        self.press_start = None;
    }

    pub fn on_blur(&mut self) {
        self.press_start = None;
    }

    pub fn has_press(&self) -> bool {
        self.press_start.is_some()
    }
}
