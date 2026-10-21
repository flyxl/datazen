/**
 * Independent verification of the return path (Wave 5 · track C, item 4).
 *
 * "Back" has to do two things: activate the tab the jump came **from**, and
 * put that tab back where the user left it. The position is restored when the
 * tab is entered again — the panel's content is unmounted while another tab
 * is active (`ContentView` renders one panel at a time), so the scroll offset
 * has to be restored explicitly.
 *
 * The contract (07 §3.5, as corrected): the snapshot belongs to the **panel**,
 * not to a stack entry — `recordViewport(panelId)` / `takeViewport(panelId)` —
 * and the restored offset is `scrollTopForAnchorRow(anchorRow, rowHeight)`.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { useEffect } from 'react';
import { useTableForeignKeyNavigation } from '../useTableForeignKeyNavigation';
import { usePanelStore } from '../../../stores/panelStore';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '../../../stores/fkNavigationStore';
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../../../platform/tauriBackendTransport';
import type { TablePanel } from '../../../stores/panelTypes';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

const schemaReads = vi.hoisted(() => ({ count: 0 }));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: async () => {
    schemaReads.count += 1;
    return {
      tableName: 'orders',
      columns: [],
      primaryKeys: [],
      indexes: [],
      foreignKeys: [
        {
          name: 'orders_customer_fk',
          columns: ['customer_id'],
          referencedTable: 'customers',
          referencedColumns: ['id'],
        },
      ],
    };
  },
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

const SOURCE_PANEL = 'panel-source';

function sourcePanel(): TablePanel {
  return {
    id: SOURCE_PANEL,
    type: 'table',
    tableName: 'orders',
    database: 'app',
    tableSchema: null,
    subTab: 'data',
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    connectionName: 'Local',
    databaseType: 'postgresql',
  };
}

function Harness({ panelId, scrollEl }: { panelId: string; scrollEl: HTMLDivElement }) {
  const navigation = useTableForeignKeyNavigation({
    panelId,
    dbSessionId: 'session-1',
    database: 'app',
    tableName: 'orders',
    schema: null,
    connectionId: 'conn-1',
    databaseType: 'postgresql',
    rows: [{ id: 1, customer_id: 7 }],
    displayedColumns: [{ name: 'id' }, { name: 'customer_id' }],
    rowHeight: 28,
    activeCell: null,
  });
  useEffect(() => {
    navigation.onScrollElementChange(scrollEl);
  }, [navigation.onScrollElementChange, scrollEl]);
  return (
    <div>
      <button type="button" data-testid="jump" onClick={() => navigation.onFkJump(0, 'customer_id')}>
        jump
      </button>
      <button type="button" data-testid="back" onClick={navigation.onBack}>
        back
      </button>
      <button type="button" data-testid="forward" onClick={navigation.onForward}>
        forward
      </button>
    </div>
  );
}

/** A scroll container jsdom can actually move: jsdom has no layout. */
function makeScrollElement(): HTMLDivElement {
  const element = document.createElement('div');
  Object.defineProperty(element, 'scrollTop', {
    value: 0,
    writable: true,
    configurable: true,
  });
  return element;
}

beforeEach(() => {
  vi.clearAllMocks();
  schemaReads.count = 0;
  resetFkNavigationEntryIds();
  usePanelStore.setState({ panels: [], activePanelId: null });
  useTableDataStore.getState().reset();
  useFkNavigationStore.getState().reset();
  usePanelStore.getState().addPanel(sourcePanel());
  setBackendClient(
    DESKTOP_BACKEND_ID,
    createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport()),
  );
  mockInvoke.mockResolvedValue({
    target: {
      database: 'app',
      schema: null,
      table: 'customers',
      columns: ['id'],
      dataTypes: ['integer'],
      primaryKeys: ['id'],
      unique: true,
      candidateLimit: 50,
    },
    exists: true,
    candidates: [],
    truncated: false,
  });
});

afterEach(cleanup);

describe('audit: the return restores the position of the source tab', () => {
  it('restores the scroll offset of the tab the jump came from', async () => {
    const scrollEl = makeScrollElement();
    scrollEl.scrollTop = 140;

    const first = render(<Harness panelId={SOURCE_PANEL} scrollEl={scrollEl} />);
    await waitFor(() => expect(schemaReads.count).toBeGreaterThan(0));
    fireEvent.click(first.container.querySelector('[data-testid="jump"]') as HTMLElement);

    // The jump happened: the target tab is open and the source position was
    // snapshotted.
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const snapshot = useFkNavigationStore.getState().takeViewport(SOURCE_PANEL);
    expect(snapshot?.scrollTop).toBe(140);
    expect(snapshot?.anchorRow).toBe(5);
    // Put it back, so the restore has something to do.
    useFkNavigationStore
      .getState()
      .recordViewport(SOURCE_PANEL, snapshot as { scrollTop: number; anchorRow: number; anchorColumn: string | null });

    // The source tab is entered again: the panel is mounted from scratch.
    first.unmount();
    scrollEl.scrollTop = 0;
    render(<Harness panelId={SOURCE_PANEL} scrollEl={scrollEl} />);

    await waitFor(() => expect(scrollEl.scrollTop).toBe(140));
  });

  it('activates the source tab on back and the target tab on forward', async () => {
    const scrollEl = makeScrollElement();
    const view = render(<Harness panelId={SOURCE_PANEL} scrollEl={scrollEl} />);
    await waitFor(() => expect(schemaReads.count).toBeGreaterThan(0));
    fireEvent.click(view.container.querySelector('[data-testid="jump"]') as HTMLElement);
    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const targetId = usePanelStore.getState().panels[1].id;

    fireEvent.click(view.container.querySelector('[data-testid="back"]') as HTMLElement);
    expect(usePanelStore.getState().activePanelId).toBe(SOURCE_PANEL);
    expect(useFkNavigationStore.getState().back).toHaveLength(0);
    expect(useFkNavigationStore.getState().forward).toHaveLength(1);

    fireEvent.click(view.container.querySelector('[data-testid="forward"]') as HTMLElement);
    expect(usePanelStore.getState().activePanelId).toBe(targetId);
    expect(useFkNavigationStore.getState().back).toHaveLength(1);
    expect(useFkNavigationStore.getState().forward).toHaveLength(0);
  });
});
