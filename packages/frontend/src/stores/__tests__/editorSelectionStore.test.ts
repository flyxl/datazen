import { beforeEach, describe, expect, it } from 'vitest';
import { getEditorSelection, useEditorSelectionStore } from '../editorSelectionStore';

describe('editorSelectionStore', () => {
  beforeEach(() => {
    useEditorSelectionStore.setState({ byPanel: {} });
  });

  it('keeps selections isolated by panel and overwrites only that panel', () => {
    const store = useEditorSelectionStore.getState();
    store.setSelection('panel-a', 'WHERE id=1');
    store.setSelection('panel-b', 'SELECT 2');

    expect(getEditorSelection('panel-a')).toBe('WHERE id=1');
    expect(getEditorSelection('panel-b')).toBe('SELECT 2');
    expect(getEditorSelection('missing')).toBe('');
  });

  it('clears a panel when the selection collapses and on unmount', () => {
    const store = useEditorSelectionStore.getState();
    store.setSelection('panel-a', 'WHERE id=1');
    store.setSelection('panel-a', '');

    expect(getEditorSelection('panel-a')).toBe('');

    store.setSelection('panel-a', 'SELECT 1');
    store.clearSelection('panel-a');
    expect(getEditorSelection('panel-a')).toBe('');
  });
});
