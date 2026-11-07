import { describe, expect, it } from 'vitest';
import { isComposingEvent, isEditableTarget } from '../gridEventGuards';

describe('isEditableTarget', () => {
  it('treats form controls and their descendants as editor-owned', () => {
    const wrapper = document.createElement('div');
    wrapper.innerHTML = '<input /><textarea></textarea><select></select>';
    for (const child of Array.from(wrapper.children)) {
      expect(isEditableTarget(child)).toBe(true);
      expect(isEditableTarget(child.firstChild ?? child)).toBe(true);
    }
  });

  it('treats a contentEditable host and an open cell editor as editor-owned', () => {
    const editable = document.createElement('div');
    editable.setAttribute('contenteditable', 'true');
    const span = document.createElement('span');
    editable.appendChild(span);

    const cell = document.createElement('div');
    cell.setAttribute('data-dt-editing', 'true');
    const padding = document.createElement('div');
    cell.appendChild(padding);

    expect(isEditableTarget(editable)).toBe(true);
    expect(isEditableTarget(span)).toBe(true);
    expect(isEditableTarget(cell)).toBe(true);
    expect(isEditableTarget(padding)).toBe(true);
  });

  it('leaves plain grid elements and non-elements alone', () => {
    const cell = document.createElement('div');
    cell.setAttribute('data-dt-row', '0');
    cell.setAttribute('data-dt-col', 'id');
    expect(isEditableTarget(cell)).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
    expect(isEditableTarget({} as EventTarget)).toBe(false);
  });
});

describe('isComposingEvent', () => {
  it('reads the flags straight off a native KeyboardEvent', () => {
    expect(isComposingEvent(new KeyboardEvent('keydown', { key: 'Escape', isComposing: true }))).toBe(
      true,
    );
    expect(isComposingEvent(new KeyboardEvent('keydown', { key: 'Escape' }))).toBe(false);
  });

  it('reads the legacy keyCode 229 signal (some IMEs never set isComposing)', () => {
    expect(isComposingEvent({ keyCode: 229 })).toBe(true);
    expect(isComposingEvent({ keyCode: 27 })).toBe(false);
  });

  it('unwraps the nativeEvent of a React synthetic event', () => {
    expect(isComposingEvent({ nativeEvent: { isComposing: true } })).toBe(true);
    expect(isComposingEvent({ nativeEvent: { keyCode: 229 } })).toBe(true);
    expect(isComposingEvent({ nativeEvent: { isComposing: false, keyCode: 27 } })).toBe(false);
  });

  it('returns false for an event with no flags at all', () => {
    expect(isComposingEvent({})).toBe(false);
  });
});
