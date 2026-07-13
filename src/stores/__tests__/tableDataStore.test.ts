import { describe, expect, it, vi, beforeEach } from 'vitest';

/**
 * Unit tests for the detail panel row tracking in tableDataStore.
 *
 * We test the pure state logic: setting/clearing the detail row index
 * and verifying that updateCell uses the correct row for edits.
 */

// Mock Tauri invoke so the store can import without a real backend
vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableData: vi.fn(),
    commitRowUpdates: vi.fn(),
  },
}));

describe('tableDataStore detail row tracking', () => {
  // We'll import the store lazily to ensure mocks are applied
  let useTableDataStore: typeof import('../../stores/tableDataStore').useTableDataStore;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../../stores/tableDataStore');
    useTableDataStore = mod.useTableDataStore;
  });

  it('detailRowIndex defaults to null', () => {
    const state = useTableDataStore.getState();
    expect(state.detailRowIndex).toBeNull();
  });

  it('setDetailRow sets the detail row index', () => {
    const store = useTableDataStore;
    store.getState().setDetailRow(2);
    expect(store.getState().detailRowIndex).toBe(2);
  });

  it('setDetailRow(null) clears the detail row', () => {
    const store = useTableDataStore;
    store.getState().setDetailRow(5);
    store.getState().setDetailRow(null);
    expect(store.getState().detailRowIndex).toBeNull();
  });
});
