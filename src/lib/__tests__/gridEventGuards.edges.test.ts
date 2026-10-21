/**
 * DB-01 §4.5 / §3.10 — the extracted event guards, exercised the way the two real callers
 * pass events: a native `KeyboardEvent` (DataTable's imperative listener) and a React
 * synthetic event (the hook's `onKeyDown`).
 *
 * The main guard suite covers the happy shapes; this file pins the contract-critical ones:
 * the composition signal must arrive through *either* shape, and a non-composing event must
 * keep flowing (the guard must not turn into a blanket "ignore every key").
 */
import { describe, expect, it } from 'vitest';
import { isComposingEvent, isEditableTarget } from '../gridEventGuards';

describe('isComposingEvent across native and synthetic shapes', () => {
  it('accepts a native KeyboardEvent whose isComposing flag is true', () => {
    const native = new KeyboardEvent('keydown', { key: 'Escape', isComposing: true });
    expect(native.isComposing).toBe(true);
    expect(isComposingEvent(native)).toBe(true);
  });

  it('accepts a React synthetic event whose nativeEvent is composing', () => {
    // React does not forward `isComposing` onto the synthetic object, so the guard must read
    // through `nativeEvent` — this is the shape `useGridSelection.onKeyDown` actually gets.
    const synthetic = {
      isComposing: undefined,
      nativeEvent: new KeyboardEvent('keydown', { key: 'Escape', isComposing: true }),
    };
    expect(isComposingEvent(synthetic)).toBe(true);
  });

  it('trusts the native flag even if a wrapper reports false at the top level', () => {
    expect(
      isComposingEvent({
        isComposing: false,
        nativeEvent: { isComposing: true },
      }),
    ).toBe(true);
  });

  it('reports the legacy keyCode 229 through either shape', () => {
    expect(isComposingEvent({ keyCode: 229 })).toBe(true);
    expect(isComposingEvent({ nativeEvent: { keyCode: 229 } })).toBe(true);
  });

  it('lets non-composing events through — a plain Escape/Enter is not swallowed', () => {
    expect(isComposingEvent(new KeyboardEvent('keydown', { key: 'Escape' }))).toBe(false);
    expect(isComposingEvent({ isComposing: false, nativeEvent: { isComposing: false } })).toBe(
      false,
    );
    expect(isComposingEvent({ keyCode: 27 })).toBe(false);
    expect(isComposingEvent({ nativeEvent: { isComposing: false, keyCode: 13 } })).toBe(false);
  });
});

describe('isEditableTarget beyond the common controls', () => {
  it('treats a text node inside an editable host as editor-owned via its element parent', () => {
    const editor = document.createElement('div');
    editor.setAttribute('contenteditable', 'true');
    const text = document.createTextNode('typing');
    editor.appendChild(text);
    // A bare text node is not an Element; the guard must not throw on it.
    expect(isEditableTarget(text)).toBe(false);
    expect(isEditableTarget(text.parentElement)).toBe(true);
  });

  it('treats option/select descendants and the grid root as distinct', () => {
    const select = document.createElement('select');
    const option = document.createElement('option');
    select.appendChild(option);
    expect(isEditableTarget(option)).toBe(true);

    const grid = document.createElement('div');
    grid.setAttribute('data-dt-surface', '');
    expect(isEditableTarget(grid)).toBe(false);
  });

  it('returns false for non-Element event targets (window/document/plain object)', () => {
    expect(isEditableTarget(window)).toBe(false);
    expect(isEditableTarget(document)).toBe(false);
    expect(isEditableTarget({} as EventTarget)).toBe(false);
  });
});
