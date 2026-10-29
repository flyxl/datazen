import { describe, expect, it } from 'vitest';
import { positionRootMenu, positionSubmenu } from '../contextMenuPosition';

const menu = { width: 180, height: 240 };
const viewport = { width: 800, height: 600 };

describe('positionRootMenu', () => {
  it('places at the cursor when there is room', () => {
    expect(positionRootMenu({ x: 40, y: 50 }, menu, viewport)).toEqual({ left: 40, top: 50 });
  });

  it('flips left when the right edge would clip', () => {
    const pos = positionRootMenu({ x: 780, y: 40 }, menu, viewport);
    expect(pos.left).toBe(780 - 180);
    expect(pos.left + menu.width).toBeLessThanOrEqual(800 - 8);
  });

  it('flips up when the bottom edge would clip', () => {
    const pos = positionRootMenu({ x: 40, y: 580 }, menu, viewport);
    expect(pos.top).toBe(580 - 240);
    expect(pos.top + menu.height).toBeLessThanOrEqual(600 - 8);
  });

  it('clamps to padding when flipping would leave the viewport', () => {
    const pos = positionRootMenu({ x: 10, y: 10 }, { width: 900, height: 700 }, viewport);
    expect(pos.left).toBe(8);
    expect(pos.top).toBe(8);
  });
});

describe('positionSubmenu', () => {
  const item = { left: 100, top: 80, width: 180, height: 28 };
  const sub = { width: 160, height: 200 };

  it('opens to the right when there is room', () => {
    const pos = positionSubmenu(item, sub, viewport);
    expect(pos.side).toBe('right');
    expect(pos.left).toBe(280);
    expect(pos.top).toBe(80);
  });

  it('flips to the left when the right side would clip', () => {
    const nearRight = { left: 640, top: 80, width: 180, height: 28 };
    const pos = positionSubmenu(nearRight, sub, viewport);
    expect(pos.side).toBe('left');
    expect(pos.left).toBe(640 - 160);
    expect(pos.left).toBeGreaterThanOrEqual(8);
    expect(pos.left + sub.width).toBeLessThanOrEqual(800 - 8);
  });

  it('shifts up when the submenu would clip the bottom', () => {
    const nearBottom = { left: 100, top: 500, width: 180, height: 28 };
    const pos = positionSubmenu(nearBottom, sub, viewport);
    expect(pos.top + sub.height).toBeLessThanOrEqual(600 - 8);
    expect(pos.top).toBeGreaterThanOrEqual(8);
  });

  it('keeps a tall submenu inside a short window', () => {
    const pos = positionSubmenu(item, { width: 160, height: 700 }, { width: 400, height: 300 });
    expect(pos.top).toBe(8);
  });
});
