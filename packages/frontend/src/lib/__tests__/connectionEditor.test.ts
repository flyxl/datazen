import { describe, expect, it, beforeEach } from 'vitest';
import {
  closeNewConnectionDialog,
  openNewConnectionDialog,
  useConnectionEditorStore,
} from '../connectionEditor';

describe('connectionEditor', () => {
  beforeEach(() => {
    useConnectionEditorStore.setState({
      open: false,
      editId: null,
      defaultGroup: null,
      openSeq: 0,
    });
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

  it('openNewConnectionDialog stores defaultGroup for create flows', () => {
    openNewConnectionDialog(undefined, 'prod');
    expect(useConnectionEditorStore.getState().defaultGroup).toBe('prod');
  });

  it('openNewConnectionDialog ignores defaultGroup when editing', () => {
    openNewConnectionDialog('cfg-1', 'prod');
    expect(useConnectionEditorStore.getState().editId).toBe('cfg-1');
    expect(useConnectionEditorStore.getState().defaultGroup).toBeNull();
  });

  it('closeNewConnectionDialog resets state', () => {
    useConnectionEditorStore.setState({ open: true, editId: 'cfg-1', defaultGroup: 'prod' });
    closeNewConnectionDialog();
    expect(useConnectionEditorStore.getState().open).toBe(false);
    expect(useConnectionEditorStore.getState().editId).toBeNull();
    expect(useConnectionEditorStore.getState().defaultGroup).toBeNull();
  });
});
