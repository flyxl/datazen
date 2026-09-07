import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import { ConnectionNavigatorTree } from '../ConnectionNavigatorTree';
import { SCHEMA_OBJECT_MIME } from '../schema-tree/schemaTreeDrag';
import type { ConnectionConfig } from '../../../types';

const mockMoveConnectionToGroup = vi.fn();
const mockToggleConnectionPinned = vi.fn();
const mockFetchConnections = vi.fn();
const mockSaveConnection = vi.fn().mockResolvedValue(undefined);
const mockReorderConnections = vi.fn().mockResolvedValue(undefined);

const connectionsState = {
  connections: [] as ConnectionConfig[],
  groups: ['Group A', 'Group B'],
  fetchConnections: mockFetchConnections,
  moveConnectionToGroup: mockMoveConnectionToGroup,
  toggleConnectionPinned: mockToggleConnectionPinned,
};

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    saveConnection: (...args: unknown[]) => mockSaveConnection(...args),
    reorderConnections: (...args: unknown[]) => mockReorderConnections(...args),
  },
}));

vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: typeof connectionsState) => unknown) =>
        selector ? selector(connectionsState) : connectionsState,
      {
        getState: () => connectionsState,
        setState: (partial: Partial<typeof connectionsState>) =>
          Object.assign(connectionsState, partial),
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
  useActiveConnectionStore: (
    selector?: (s: { connections: Record<string, unknown> }) => unknown,
  ) => (selector ? selector({ connections: {} }) : { connections: {} }),
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
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-conn-name="Conn 1"]')).not.toBeNull(),
    );

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

  it('does not intercept table drops onto group headers', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-group-name="Group A"]')).not.toBeNull(),
    );

    const groupAEl = container.querySelector('[data-group-name="Group A"]')!;
    expect(groupAEl).not.toBeNull();

    // Simulate a table drag (no dragStart on connection — so dragConnId.current is null).
    // Table drags set SCHEMA_OBJECT_MIME + text/plain containing the table name.
    const dt = {
      setData: vi.fn(),
      getData: vi.fn((type: string) => {
        if (type === 'text/plain') return 'er_orders';
        if (type === SCHEMA_OBJECT_MIME) return JSON.stringify({ version: 1, kind: 'table' });
        return '';
      }),
      effectAllowed: 'copy',
      dropEffect: '',
      types: [SCHEMA_OBJECT_MIME, 'text/plain'],
    };
    fireEvent.drop(groupAEl, { dataTransfer: dt });

    // group handler must NOT consume the drop — it should have bailed out
    // because dragConnId.current was null (table drag, not connection reorder)
    expect(mockMoveConnectionToGroup).not.toHaveBeenCalled();
  });

  it('does not intercept table drops onto section headers', async () => {
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

    const recentHeader = container.querySelector('[data-section="recent"]')!;

    // Simulate a table drag (no dragStart on connection — so dragConnId.current is null).
    const dt = {
      setData: vi.fn(),
      getData: vi.fn((type: string) => {
        if (type === 'text/plain') return 'er_orders';
        if (type === SCHEMA_OBJECT_MIME) return JSON.stringify({ version: 1, kind: 'table' });
        return '';
      }),
      effectAllowed: 'copy',
      dropEffect: '',
      types: [SCHEMA_OBJECT_MIME, 'text/plain'],
    };
    fireEvent.drop(recentHeader, { dataTransfer: dt });

    // Section handler must NOT consume the drop
    expect(mockMoveConnectionToGroup).not.toHaveBeenCalled();
  });

  it('moves connection to empty group placeholder on drop', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-conn-name="Conn 1"]')).not.toBeNull(),
    );

    const connEl = container.querySelector('[data-conn-name="Conn 1"]')!;
    const emptyGroupBEl = container.querySelector('[data-empty-group="Group B"]')!;
    expect(emptyGroupBEl).not.toBeNull();

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(connEl, { dataTransfer: dt });

    fireEvent.dragOver(emptyGroupBEl, { dataTransfer: dt });
    expect(dt.dropEffect).toBe('move');

    fireEvent.drop(emptyGroupBEl, { dataTransfer: dt });
    expect(mockMoveConnectionToGroup).toHaveBeenCalledWith('conn-1', 'Group B');
  });

  it('moves connection to target group and reorders when dropped on a connection in another group', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
      {
        id: 'conn-2',
        name: 'Conn 2',
        databaseType: 'postgresql',
        group: 'Group B',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-conn-name="Conn 2"]')).not.toBeNull(),
    );

    const conn1El = container.querySelector('[data-conn-name="Conn 1"]')!;
    const conn2El = container.querySelector('[data-conn-name="Conn 2"]')!;

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(conn1El, { dataTransfer: dt });

    // Hover over conn-2 with clientY below midpoint (position: after)
    fireEvent.dragOver(conn2El, { dataTransfer: dt, clientY: 100 });
    fireEvent.drop(conn2El, { dataTransfer: dt });

    await waitFor(() => {
      expect(mockSaveConnection).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'conn-1', group: 'Group B' }),
      );
      expect(mockReorderConnections).toHaveBeenCalledWith(['conn-2', 'conn-1']);
      expect(mockFetchConnections).toHaveBeenCalled();
    });
  });

  it('reorders connections within same group without changing group attribute', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
      {
        id: 'conn-2',
        name: 'Conn 2',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-conn-name="Conn 2"]')).not.toBeNull(),
    );

    const conn1El = container.querySelector('[data-conn-name="Conn 1"]')!;
    const conn2El = container.querySelector('[data-conn-name="Conn 2"]')!;

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(conn1El, { dataTransfer: dt });

    fireEvent.dragOver(conn2El, { dataTransfer: dt, clientY: 100 });
    fireEvent.drop(conn2El, { dataTransfer: dt });

    await waitFor(() => {
      expect(mockSaveConnection).not.toHaveBeenCalled();
      expect(mockReorderConnections).toHaveBeenCalledWith(['conn-2', 'conn-1']);
      expect(mockFetchConnections).toHaveBeenCalled();
    });
  });

  it('preserves dropTarget when dragleave bubbles from child elements', async () => {
    connectionsState.connections = [
      {
        id: 'conn-1',
        name: 'Conn 1',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
      {
        id: 'conn-2',
        name: 'Conn 2',
        databaseType: 'postgresql',
        group: 'Group A',
      } as ConnectionConfig,
    ];
    const { container } = render(<ConnectionNavigatorTree {...baseProps} />);

    await waitFor(() =>
      expect(container.querySelector('[data-conn-name="Conn 2"]')).not.toBeNull(),
    );

    const conn1El = container.querySelector('[data-conn-name="Conn 1"]')!;
    let conn2El = container.querySelector('[data-conn-name="Conn 2"]')!;

    const dt = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(conn1El, { dataTransfer: dt });
    fireEvent.dragOver(conn2El, { dataTransfer: dt, clientY: 100 });

    // After dragOver re-render, re-query conn2El and its child span
    conn2El = container.querySelector('[data-conn-name="Conn 2"]')!;
    const childSpan = conn2El.querySelector('span')!;

    // In jsdom DragEvent doesn't carry relatedTarget by default, so attach it explicitly
    const leaveEvent = fireEvent.dragLeave(conn2El);
    // Re-dragOver to restore target, then simulate dragLeave with relatedTarget
    fireEvent.dragOver(conn2El, { dataTransfer: dt, clientY: 100 });
    const customLeave = new MouseEvent('dragleave', { bubbles: true });
    Object.defineProperty(customLeave, 'relatedTarget', { value: childSpan });
    fireEvent(conn2El, customLeave);

    // Drop should still succeed because dropTarget was not cleared
    fireEvent.drop(conn2El, { dataTransfer: dt });

    await waitFor(() => {
      expect(mockReorderConnections).toHaveBeenCalledWith(['conn-2', 'conn-1']);
    });
  });
});
