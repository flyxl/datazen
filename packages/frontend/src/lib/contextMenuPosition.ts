export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export const CONTEXT_MENU_PAD = 8;

function clamp(n: number, min: number, max: number): number {
  if (max < min) return min;
  return Math.min(max, Math.max(min, n));
}

/**
 * Place a root context menu at the cursor. Flip left/up when it would overflow
 * the viewport so the panel stays fully visible (not clipped by the window).
 */
export function positionRootMenu(
  cursor: Point,
  menu: Size,
  viewport: Viewport,
  pad = CONTEXT_MENU_PAD,
): { left: number; top: number } {
  let left = cursor.x;
  let top = cursor.y;

  if (left + menu.width > viewport.width - pad) {
    left = cursor.x - menu.width;
  }
  left = clamp(left, pad, viewport.width - pad - menu.width);

  if (top + menu.height > viewport.height - pad) {
    top = cursor.y - menu.height;
  }
  top = clamp(top, pad, viewport.height - pad - menu.height);

  return { left, top };
}

/**
 * Place a submenu next to its parent item. Prefer the right side; flip to the
 * left when the right edge would clip. Vertical align to the item, then clamp.
 */
export function positionSubmenu(
  item: Rect,
  submenu: Size,
  viewport: Viewport,
  pad = CONTEXT_MENU_PAD,
): { left: number; top: number; side: 'right' | 'left' } {
  const itemRight = item.left + item.width;
  const spaceRight = viewport.width - pad - itemRight;
  const spaceLeft = item.left - pad;
  const side: 'right' | 'left' =
    spaceRight >= submenu.width || spaceRight >= spaceLeft ? 'right' : 'left';

  let left = side === 'right' ? itemRight : item.left - submenu.width;
  left = clamp(left, pad, viewport.width - pad - submenu.width);

  let top = item.top;
  if (top + submenu.height > viewport.height - pad) {
    top = viewport.height - pad - submenu.height;
  }
  top = clamp(top, pad, viewport.height - pad - submenu.height);

  return { left, top, side };
}
