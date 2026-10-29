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
import { useTableForeignKeyNavigation } from '@datazen/frontend/src/windows/connection/useTableForeignKeyNavigation';
import { usePanelStore } from '@datazen/frontend/src/stores/panelStore';
import { useTableDataStore } from '@datazen/frontend/src/stores/tableDataStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '@datazen/frontend/src/stores/fkNavigationStore';
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../desktop/tauriBackendTransport';
import type { TablePanel } from '@datazen/frontend/src/stores/panelTypes';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

const schemaReads = vi.hoisted(() => ({ count: 0 }));

vi.mock('@datazen/frontend/src/lib/schemaCache', () => ({
  // The reverse catalog (DB-20) subscribes at module load; a no-op keeps
  // this audit focused on the jump's own reads.
  subscribeSchemaInvalidation: () => () => undefined,
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

vi.mock('@datazen/frontend/src/lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

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
      <button
        type="button"
        data-testid="jump"
        onClick={() => navigation.foreignKeys.onJump?.(0, 'customer_id')}
      >
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
  // The viewport is only read from an element that is still in the document —
  // a detached one belongs to a surface that has already been replaced.
  document.body.appendChild(element);
  // jsdom has no layout: a real surface reports the height it occupies, which
  // is how the hook tells the grid on screen from an inactive pane's.
  Object.defineProperty(element, 'clientHeight', { value: 375, configurable: true });
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
  // Panels stay mounted behind the front tab, so the snapshot is consumed on
  // activation: the tab under test has to be the active one.
  usePanelStore.getState().setActivePanel(SOURCE_PANEL);
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
    // Read the snapshot without consuming it: the mount that follows is the one
    // allowed to take it.
    const snapshot = useFkNavigationStore.getState().viewports.get(SOURCE_PANEL);
    expect(snapshot?.scrollTop).toBe(140);
    expect(snapshot?.anchorRow).toBe(5);

    // The source tab is entered again: the panel is mounted from scratch and is
    // the tab in front, which is the only state that may place an offset (a
    // hidden tab that placed one would consume the position "back" restores).
    first.unmount();
    scrollEl.scrollTop = 0;
    usePanelStore.getState().setActivePanel(SOURCE_PANEL);
    render(<Harness panelId={SOURCE_PANEL} scrollEl={scrollEl} />);

    await waitFor(() => expect(scrollEl.scrollTop).toBe(140));
  });

  it('keeps the offset the press started from when the engine rewinds the grid', async () => {
    const scrollEl = makeScrollElement();
    scrollEl.scrollTop = 140;
    // Focusing a button inside the virtual window makes the engine scroll the
    // surface to its top while the press is dispatched. The snapshot has to be
    // the offset the user saw before that happened.
    const view = render(<Harness panelId={SOURCE_PANEL} scrollEl={scrollEl} />);
    await waitFor(() => expect(schemaReads.count).toBeGreaterThan(0));
    const jump = view.container.querySelector('[data-testid="jump"]') as HTMLElement;
    // The press starts on the grid, which is what the capture listener sees.
    fireEvent.mouseDown(scrollEl);
    // ...and the engine rewinds the surface before the click is dispatched.
    scrollEl.scrollTop = 0;
    fireEvent.click(jump);

    await waitFor(() => expect(usePanelStore.getState().panels).toHaveLength(2));
    const snapshot = useFkNavigationStore.getState().takeViewport(SOURCE_PANEL);
    expect(snapshot?.scrollTop).toBe(140);
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
