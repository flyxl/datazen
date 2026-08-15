import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  installDragSelectionGuard,
  installRightDragSelectionSuppressor,
} from '../globalTextSelection';

function mockSelection(nonCollapsed: boolean) {
  const removeAllRanges = vi.fn();
  Object.defineProperty(window, 'getSelection', {
    configurable: true,
    value: () => ({ isCollapsed: !nonCollapsed, removeAllRanges }),
  });
  return removeAllRanges;
}

function fire(type: string, init: { button?: number; buttons?: number }) {
  document.dispatchEvent(
    new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      button: init.button ?? 0,
      buttons: init.buttons ?? 0,
    }),
  );
}

describe('installRightDragSelectionSuppressor', () => {
  let stop: () => void;

  beforeEach(() => {
    stop = installRightDragSelectionSuppressor();
  });

  afterEach(() => {
    stop();
    vi.restoreAllMocks();
  });

  it('clears the selection when the right button is held and dragged', () => {
    const removeAllRanges = mockSelection(true);
    fire('mousedown', { button: 2, buttons: 2 });
    fire('mousemove', { buttons: 2 });
    expect(removeAllRanges).toHaveBeenCalledTimes(1);
  });

  it('keeps clearing while dragging with the right button held', () => {
    const removeAllRanges = mockSelection(true);
    fire('mousedown', { button: 2, buttons: 2 });
    fire('mousemove', { buttons: 2 });
    fire('mousemove', { buttons: 2 });
    expect(removeAllRanges).toHaveBeenCalledTimes(2);
  });

  it('does not clear a left-button selection on a plain right click', () => {
    const removeAllRanges = mockSelection(true);
    fire('mousedown', { button: 2, buttons: 2 });
    fire('mouseup', { button: 2, buttons: 0 });
    expect(removeAllRanges).not.toHaveBeenCalled();
  });

  it('ignores left-button drags', () => {
    const removeAllRanges = mockSelection(true);
    fire('mousedown', { button: 0, buttons: 1 });
    fire('mousemove', { buttons: 1 });
    expect(removeAllRanges).not.toHaveBeenCalled();
  });

  it('stops suppressing after the cleanup function runs', () => {
    const removeAllRanges = mockSelection(true);
    stop();
    fire('mousedown', { button: 2, buttons: 2 });
    fire('mousemove', { buttons: 2 });
    expect(removeAllRanges).not.toHaveBeenCalled();
  });
});

describe('installDragSelectionGuard', () => {
  let stop: () => void;
  let el: HTMLElement;

  beforeEach(() => {
    stop = installDragSelectionGuard();
    el = document.createElement('div');
    document.body.appendChild(el);
  });

  afterEach(() => {
    stop();
    el.remove();
    vi.restoreAllMocks();
  });

  function pressAndDrag(userSelect: string) {
    el.style.userSelect = userSelect;
    el.dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }),
    );
    fire('mousemove', { buttons: 1 });
  }

  it('clears the selection when the press started on non-selectable content', () => {
    const removeAllRanges = mockSelection(true);
    pressAndDrag('none');
    expect(removeAllRanges).toHaveBeenCalledTimes(1);
  });

  it('keeps the selection when the press started on selectable text', () => {
    const removeAllRanges = mockSelection(true);
    pressAndDrag('text');
    expect(removeAllRanges).not.toHaveBeenCalled();
  });

  it('does not clear on a plain click without movement', () => {
    const removeAllRanges = mockSelection(true);
    el.style.userSelect = 'none';
    el.dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }),
    );
    fire('mouseup', { button: 0, buttons: 0 });
    expect(removeAllRanges).not.toHaveBeenCalled();
  });

  it('stops guarding after the cleanup function runs', () => {
    const removeAllRanges = mockSelection(true);
    stop();
    pressAndDrag('none');
    expect(removeAllRanges).not.toHaveBeenCalled();
  });

  function mockSelectionIntersecting(intersects: boolean) {
    Object.defineProperty(window, 'getSelection', {
      configurable: true,
      value: () => ({
        isCollapsed: false,
        rangeCount: 1,
        getRangeAt: () => ({
          intersectsNode: () => intersects,
          containsNode: () => false,
        }),
        removeAllRanges: vi.fn(),
      }),
    });
  }

  function pressWithDetail(detail: number): boolean {
    const ev = new MouseEvent('mousedown', {
      bubbles: true,
      cancelable: true,
      button: 0,
      detail,
    });
    el.dispatchEvent(ev);
    return ev.defaultPrevented;
  }

  it('prevents a single press inside an existing selection (keeps highlight stable)', () => {
    mockSelectionIntersecting(true);
    expect(pressWithDetail(1)).toBe(true);
  });

  it('lets the second press of a double-click proceed (word selection works)', () => {
    mockSelectionIntersecting(true);
    expect(pressWithDetail(2)).toBe(false);
  });

  it('does not prevent a press outside the existing selection', () => {
    mockSelectionIntersecting(false);
    expect(pressWithDetail(1)).toBe(false);
  });
});
