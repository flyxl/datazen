import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import { ConnectionNavigatorTree } from '../ConnectionNavigatorTree';
import type { ConnectionConfig } from '../../../types';

const mockMoveConnectionToGroup = vi.fn();
const mockToggleConnectionPinned = vi.fn();
const mockFetchConnections = vi.fn();

const connectionsState = {
  connections: [] as ConnectionConfig[],
  groups: ['Group A', 'Group B'],
  fetchConnections: mockFetchConnections,
  moveConnectionToGroup: mockMoveConnectionToGroup,
  toggleConnectionPinned: mockToggleConnectionPinned,
};

vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: typeof connectionsState) => unknown) =>
        selector ? selector(connectionsState) : connectionsState,
      {
        getState: () => connectionsState,
        setState: (partial: Partial<typeof connectionsState>) => Object.assign(connectionsState, partial),
      },
    ),
  };
});

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({
    count,
    estimateSize,
    getScrollElement,
  }: {
    count: number;
    estimateSize?: (i: number) => number;
    getScrollElement?: () => unknown;
  }) => {
    getScrollElement?.();
    const sizeOf = (i: number) => estimateSize?.(i) ?? 28;
    let offset = 0;
    const items = Array.from({ length: count }, (_, index) => {
      const size = sizeOf(index);
      const start = offset;
      offset += size;
      return { index, key: index, start, size, end: start + size };
    });
    return {
      getTotalSize: () => offset || count * 28,
      getVirtualItems: () => items,
    };
  },
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: (selector?: (s: { connections: Record<string, unknown> }) => unknown) =>
    selector ? selector({ connections: {} }) : { connections: {} },
}));

vi.mock('../../../stores/panelStore', () => ({
  usePanelStore: (selector?: (s: { activePanelId: null }) => unknown) =>
    selector ? selector({ activePanelId: null }) : { activePanelId: null },
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (selector?: (s: Record<string, unknown>) => unknown) =>
    selector ? selector({}) : {},
  useConnectionSchemaField: () => [],
}));

describe('ConnectionNavigatorTree Group Drag & Drop', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const baseProps = {
    selectedConnectionId: null,
    onSelectConnection: vi.fn(),
    onNewConnection: vi.fn(),
    onExportConnections: vi.fn(),
    onImportConnections: vi.fn(),
    onRefresh: vi.fn(),
    onSelectTable: vi.fn(),
  };

  it('moves connection to target group on drop', async () => {
    connectionsState.connections = [
      { id: 'conn-1', name: 'Conn 1', databaseType: 'postgresql', group: 'Group A' } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() => expect(container.querySelector('[data-conn-name="Conn 1"]')).not.toBeNull());

    const connEl = container.querySelector('[data-conn-name="Conn 1"]')!;
    const groupBEl = container.querySelector('[data-group-name="Group B"]')!;
    expect(groupBEl).not.toBeNull();

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(connEl, { dataTransfer: dt });

    fireEvent.dragOver(groupBEl, { dataTransfer: dt });
    expect(dt.dropEffect).toBe('move');

    fireEvent.drop(groupBEl, { dataTransfer: dt });
    expect(mockMoveConnectionToGroup).toHaveBeenCalledWith('conn-1', 'Group B');
  });

  it('rejects dragover and drop onto recent section', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        driver: 'postgres',
        host: 'localhost',
        port: 5432,
        database: 'db1',
        user: 'postgres',
        lastConnectedAt: new Date().toISOString(),
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() => expect(container.querySelector('[data-section="recent"]')).not.toBeNull());

    const connEl = container.querySelector('[data-conn-name="Conn 1"]')!;
    const recentHeader = container.querySelector('[data-section="recent"]')!;

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(connEl, { dataTransfer: dt });

    fireEvent.dragOver(recentHeader, { dataTransfer: dt });
    // Recent section must disallow drop
    expect(dt.dropEffect).toBe('none');

    fireEvent.drop(recentHeader, { dataTransfer: dt });
    expect(mockMoveConnectionToGroup).not.toHaveBeenCalled();
    expect(mockToggleConnectionPinned).not.toHaveBeenCalled();
  });
});
