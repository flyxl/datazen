//! Context-menu placement geometry, ported from
//! `packages/frontend/src/lib/contextMenuPosition.ts`.
//!
//! Pure arithmetic: place a root context menu at the cursor, place a submenu
//! next to its parent item, and flip or clamp so the panel stays fully inside
//! the viewport. No Rune tree, no state — the WebContextMenu port (tasklist
//! T098) feeds measured sizes and the window extent in, and lays the returned
//! coordinates out.
//!
//! React's numbers are JavaScript doubles; Rune lays out in `f32`, so this
//! port narrows every coordinate to `f32`. The tests only use values that are
//! exact in both widths, so every branch pins bit-for-bit.

/// A cursor point, in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A measured panel size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// The window the menu must stay inside of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

/// A menu-item box (left/top origin, like a DOM rect).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// The gap kept between a placed menu and the viewport edge.
pub const CONTEXT_MENU_PAD: f32 = 8.0;

/// `Math.min(max, Math.max(min, n))`, with React's inverted-range branch:
/// when `max < min` the clamp answers `min` instead of tightening.
///
/// Not `f32::clamp`: the std method panics on `min > max`, and this layer's
/// whole point is staying calm when the menu is bigger than the window.
/// (NaN never reaches layout coordinates; were it to, Rust's `f32::max`
/// drops the NaN where JavaScript's `Math.max` propagates it — an accepted
/// divergence that no caller can reach.)
fn clamp(n: f32, min: f32, max: f32) -> f32 {
    if max < min {
        return min;
    }
    n.max(min).min(max)
}

/// Where a root menu ended up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootMenuPosition {
    pub left: f32,
    pub top: f32,
}

/// Which side a submenu opened on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmenuSide {
    Right,
    Left,
}

/// Where a submenu ended up, and the side it opened on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubmenuPosition {
    pub left: f32,
    pub top: f32,
    pub side: SubmenuSide,
}

/// Place a root context menu at the cursor. Flip left/up when it would
/// overflow the viewport so the panel stays fully visible (not clipped by
/// the window). React's `pad` parameter defaults to
/// [`CONTEXT_MENU_PAD`]; this is the default-argument form.
pub fn position_root_menu(cursor: Point, menu: Size, viewport: Viewport) -> RootMenuPosition {
    position_root_menu_with_pad(cursor, menu, viewport, CONTEXT_MENU_PAD)
}

/// [`position_root_menu`] with an explicit `pad` — the React 4th argument.
pub fn position_root_menu_with_pad(
    cursor: Point,
    menu: Size,
    viewport: Viewport,
    pad: f32,
) -> RootMenuPosition {
    let mut left = cursor.x;
    let mut top = cursor.y;

    if left + menu.width > viewport.width - pad {
        left = cursor.x - menu.width;
    }
    left = clamp(left, pad, viewport.width - pad - menu.width);

    if top + menu.height > viewport.height - pad {
        top = cursor.y - menu.height;
    }
    top = clamp(top, pad, viewport.height - pad - menu.height);

    RootMenuPosition { left, top }
}

/// Place a submenu next to its parent item. Prefer the right side; flip to the
/// left when the right edge would clip (right survives a tie). Vertical align
/// to the item, then clamp. React's `pad` parameter defaults to
/// [`CONTEXT_MENU_PAD`]; this is the default-argument form.
pub fn position_submenu(item: Rect, submenu: Size, viewport: Viewport) -> SubmenuPosition {
    position_submenu_with_pad(item, submenu, viewport, CONTEXT_MENU_PAD)
}

/// [`position_submenu`] with an explicit `pad` — the React 4th argument.
pub fn position_submenu_with_pad(
    item: Rect,
    submenu: Size,
    viewport: Viewport,
    pad: f32,
) -> SubmenuPosition {
    let item_right = item.left + item.width;
    let space_right = viewport.width - pad - item_right;
    let space_left = item.left - pad;
    // `>=` twice: an exact fit stays right, and so does a right side that is
    // merely no narrower than the left one.
    let side = if space_right >= submenu.width || space_right >= space_left {
        SubmenuSide::Right
    } else {
        SubmenuSide::Left
    };

    let mut left = if side == SubmenuSide::Right {
        item_right
    } else {
        item.left - submenu.width
    };
    left = clamp(left, pad, viewport.width - pad - submenu.width);

    let mut top = item.top;
    if top + submenu.height > viewport.height - pad {
        top = viewport.height - pad - submenu.height;
    }
    top = clamp(top, pad, viewport.height - pad - submenu.height);

    SubmenuPosition { left, top, side }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENU: Size = Size { width: 180.0, height: 240.0 };
    const VIEWPORT: Viewport = Viewport { width: 800.0, height: 600.0 };

    #[test]
    fn the_pad_constant_is_the_react_default() {
        assert_eq!(CONTEXT_MENU_PAD, 8.0);
    }

    #[test]
    fn places_at_the_cursor_when_there_is_room() {
        let pos = position_root_menu(Point { x: 40.0, y: 50.0 }, MENU, VIEWPORT);
        assert_eq!((pos.left, pos.top), (40.0, 50.0));
    }

    #[test]
    fn flips_left_when_the_right_edge_would_clip() {
        let pos = position_root_menu(Point { x: 780.0, y: 40.0 }, MENU, VIEWPORT);
        assert_eq!(pos.left, 780.0 - 180.0);
        assert!(pos.left + MENU.width <= 800.0 - 8.0);
    }

    #[test]
    fn flips_up_when_the_bottom_edge_would_clip() {
        let pos = position_root_menu(Point { x: 40.0, y: 580.0 }, MENU, VIEWPORT);
        assert_eq!(pos.top, 580.0 - 240.0);
        assert!(pos.top + MENU.height <= 600.0 - 8.0);
    }

    #[test]
    fn clamps_to_padding_when_flipping_would_leave_the_viewport() {
        // The inverted-range branch: the menu is bigger than the window, so
        // both clamps answer `min` (the pad) rather than tightening.
        let pos = position_root_menu(
            Point { x: 10.0, y: 10.0 },
            Size { width: 900.0, height: 700.0 },
            VIEWPORT,
        );
        assert_eq!(pos.left, 8.0);
        assert_eq!(pos.top, 8.0);
    }

    #[test]
    fn an_exact_fit_on_the_pad_edge_does_not_flip() {
        // 612 + 180 == 800 - 8 and 352 + 240 == 600 - 8: the overflow guard is
        // strictly `>`, so an exact fit stays at the cursor.
        let pos = position_root_menu(Point { x: 612.0, y: 352.0 }, MENU, VIEWPORT);
        assert_eq!((pos.left, pos.top), (612.0, 352.0));
    }

    #[test]
    fn a_custom_pad_flips_and_clamps_like_the_react_fourth_argument() {
        let pos = position_root_menu_with_pad(Point { x: 780.0, y: 40.0 }, MENU, VIEWPORT, 20.0);
        // 780 + 180 > 800 - 20 flips to 600, and 600 is exactly the clamp max.
        assert_eq!(pos.left, 600.0);
        assert_eq!(pos.top, 40.0);
    }

    #[test]
    fn opens_to_the_right_when_there_is_room() {
        let pos = position_submenu(
            Rect { left: 100.0, top: 80.0, width: 180.0, height: 28.0 },
            Size { width: 160.0, height: 200.0 },
            VIEWPORT,
        );
        assert_eq!(pos.side, SubmenuSide::Right);
        assert_eq!(pos.left, 280.0);
        assert_eq!(pos.top, 80.0);
    }

    #[test]
    fn flips_to_the_left_when_the_right_side_would_clip() {
        let pos = position_submenu(
            Rect { left: 640.0, top: 80.0, width: 180.0, height: 28.0 },
            Size { width: 160.0, height: 200.0 },
            VIEWPORT,
        );
        assert_eq!(pos.side, SubmenuSide::Left);
        assert_eq!(pos.left, 640.0 - 160.0);
        assert!(pos.left >= 8.0);
        assert!(pos.left + 160.0 <= 800.0 - 8.0);
    }

    #[test]
    fn shifts_up_when_the_submenu_would_clip_the_bottom() {
        let pos = position_submenu(
            Rect { left: 100.0, top: 500.0, width: 180.0, height: 28.0 },
            Size { width: 160.0, height: 200.0 },
            VIEWPORT,
        );
        // 500 + 200 > 592 shifts to 592 - 200; the clamp keeps it there.
        assert_eq!(pos.top, 392.0);
        assert!(pos.top + 200.0 <= 600.0 - 8.0);
        assert!(pos.top >= 8.0);
    }

    #[test]
    fn an_exact_bottom_fit_stays_aligned_to_the_item() {
        // 392 + 200 == 600 - 8: strictly `>`, no shift, and the clamp agrees.
        let pos = position_submenu(
            Rect { left: 100.0, top: 392.0, width: 180.0, height: 28.0 },
            Size { width: 160.0, height: 200.0 },
            VIEWPORT,
        );
        assert_eq!(pos.top, 392.0);
    }

    #[test]
    fn keeps_a_tall_submenu_inside_a_short_window() {
        let pos = position_submenu(
            Rect { left: 100.0, top: 80.0, width: 180.0, height: 28.0 },
            Size { width: 160.0, height: 700.0 },
            Viewport { width: 400.0, height: 300.0 },
        );
        assert_eq!(pos.top, 8.0);
        // Left side: space right (112) cannot fit 160 but does beat space left
        // (92), so it opens right and the clamp pulls it to 232.
        assert_eq!(pos.side, SubmenuSide::Right);
        assert_eq!(pos.left, 232.0);
    }

    #[test]
    fn a_tied_right_side_stays_right_even_when_neither_side_fits() {
        // space right == space left == 388, both under the 400-wide submenu:
        // the tie branch keeps the right side, then the clamp fits what it can.
        let pos = position_submenu(
            Rect { left: 396.0, top: 80.0, width: 8.0, height: 28.0 },
            Size { width: 400.0, height: 200.0 },
            VIEWPORT,
        );
        assert_eq!(pos.side, SubmenuSide::Right);
        assert_eq!(pos.left, 392.0);
    }

    #[test]
    fn an_exact_width_fit_opens_right_and_lands_on_the_pad_edge() {
        let pos = position_submenu(
            Rect { left: 100.0, top: 80.0, width: 180.0, height: 28.0 },
            Size { width: 512.0, height: 200.0 },
            VIEWPORT,
        );
        assert_eq!(pos.side, SubmenuSide::Right);
        assert_eq!(pos.left, 280.0);
        assert_eq!(pos.left + 512.0, 800.0 - 8.0);
    }
}
