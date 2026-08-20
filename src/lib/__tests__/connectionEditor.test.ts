import { describe, expect, it, beforeEach } from 'vitest';
import {
  closeNewConnectionDialog,
  openNewConnectionDialog,
  useConnectionEditorStore,
} from '../connectionEditor';

describe('connectionEditor', () => {
  beforeEach(() => {
    useConnectionEditorStore.setState({ open: false, editId: null, openSeq: 0 });
  });

  it('openNewConnectionDialog opens with optional editId', () => {
    openNewConnectionDialog('cfg-1');
    expect(useConnectionEditorStore.getState().open).toBe(true);
    expect(useConnectionEditorStore.getState().editId).toBe('cfg-1');
  });

  it('openNewConnectionDialog without editId clears editId', () => {
    useConnectionEditorStore.setState({ open: true, editId: 'old' });
    openNewConnectionDialog();
    expect(useConnectionEditorStore.getState().open).toBe(true);
    expect(useConnectionEditorStore.getState().editId).toBeNull();
  });

  it('openNewConnectionDialog increments openSeq on each open', () => {
    openNewConnectionDialog();
    expect(useConnectionEditorStore.getState().openSeq).toBe(1);
    closeNewConnectionDialog();
    openNewConnectionDialog('cfg-1');
    expect(useConnectionEditorStore.getState().openSeq).toBe(2);
    openNewConnectionDialog();
    expect(useConnectionEditorStore.getState().openSeq).toBe(3);
  });

  it('closeNewConnectionDialog resets state', () => {
    useConnectionEditorStore.setState({ open: true, editId: 'cfg-1' });
    closeNewConnectionDialog();
    expect(useConnectionEditorStore.getState().open).toBe(false);
    expect(useConnectionEditorStore.getState().editId).toBeNull();
  });
});
