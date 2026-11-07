/**
 * The 48px content toolbar band is skipped for KV panels.
 *
 * Why the suppression lives here and not in the driver: a KV driver already has
 * every SQL affordance the band carries switched off (`showNewQuery` and friends
 * are all `false` for a key-value type), and the one slot that used to fill the
 * band — the driver's `contextBar` — is gone. What would remain is a reserved
 * 48px row with nothing in it above every Redis panel. The band is a host
 * surface, so the host decides whether to draw it; the driver only decides what
 * to put in it, and now puts that nothing.
 *
 * These cases pin the *decision*, in both directions. A one-sided test would
 * still pass if the guard were inverted to `isKvPanel`, which would delete the
 * toolbar from every SQL panel — the far more expensive mistake of the two.
 *
 * `ContentToolbar` itself is stubbed to a marker: this is about whether the host
 * mounts the band at all, not about what is inside it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ConnectionSchemaState } from '../../../stores/schemaStoreState';

const workspace = vi.hoisted(() => ({ isKvPanel: true }));

vi.mock('../ContentToolbar', () => ({
  ContentToolbar: () => <div data-testid="stub-content-toolbar" />,
}));

vi.mock('../useConnectionWorkspaceMeta', () => ({
  useConnectionWorkspaceMeta: () => ({
    sidebarConnCtx: null,
    initialDatabase: 'db0',
    databaseType: 'redis',
    dbSessionId: 'sess-1',
    connectionName: 'local',
    hasSavedConnections: false,
    showStructureEditor: false,
    exportScope: undefined,
    batchExportSupported: false,
    showNewQuery: false,
    showNewTable: false,
    showErDiagramToolbar: false,
    showObjectsToolbar: false,
    connectingEntry: null,
    connectingName: '',
    connectingDbType: undefined,
    recentPanels: [],
    statusDatabase: 'db0',
    isKvPanel: workspace.isKvPanel,
    connectionId: 'cfg-1',
  }),
}));

vi.mock('../useKvWorkspaceSlots', () => ({
  useKvWorkspaceSlots: () => ({}),
}));

vi.mock('../useKvSlotActions', () => ({
  useKvSlotActions: () => ({ request: () => {}, dialog: null }),
}));

vi.mock('../usePanelHandlers', () => ({
  usePanelHandlers: () => ({
    handleRefresh: () => {},
    handleNewQuery: () => {},
    handleCreateTable: () => {},
    handleOpenErDiagram: () => {},
    handleOpenObjects: () => {},
    handleOpenPrivileges: () => {},
    handleSelectTable: () => {},
    handleClosePanel: () => {},
    handlePanelTabContextMenu: () => {},
    handleSetSubTab: () => {},
    handleExitStructureEditing: () => {},
    handleEditTableStructure: () => {},
    handleOpenQueryHistory: () => {},
  }),
}));

// `vi.mock` replaces the whole module, so the pane-identity helpers and the
// frozen exec default `panelStore` re-exports must exist here too; both come
// from pure leaf modules, so take the real implementations.
vi.mock('../../../stores/panelStore', async () => {
  const pane = await vi.importActual<typeof import('../../../stores/paneKeys')>(
    '../../../stores/paneKeys',
  );
  const { EMPTY_QUERY_EXEC } = await vi.importActual<
    typeof import('../../../stores/queryExecActions')
  >('../../../stores/queryExecActions');
  const state = {
    panels: [{ id: 'panel-1', type: 'redis-db', title: 'db0' }],
    activePanelId: 'panel-1' as string | null,
    setActivePanel: () => {},
    updatePanel: () => {},
  };
  const usePanelStore = (selector: (s: typeof state) => unknown) => selector(state);
  usePanelStore.getState = () => state;
  return { ...pane, EMPTY_QUERY_EXEC, usePanelStore, nextPanelId: (prefix: string) => `panel-${prefix}` };
});

vi.mock('../../../stores/settingsStore', () => {
  const state = { settings: { safeMode: false } };
  const useSettingsStore = (selector: (s: typeof state) => unknown) => selector(state);
  useSettingsStore.getState = () => state;
  return { useSettingsStore };
});

vi.mock('../../../stores/connectionStore', () => {
  const state = { connections: [] };
  const useConnectionStore = (selector: (s: typeof state) => unknown) => selector(state);
  useConnectionStore.getState = () => state;
  return { useConnectionStore };
});

// Wholesale replacement of the real module, so it must expose every value export
// it has — `useConnectionSchemaField` was missing, and `ContentView` calls it
// three times, so every render in this file threw before asserting anything.
vi.mock('../../../stores/schemaStore', async () => {
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  const state = {
    schemas: new Map<string, ConnectionSchemaState>(),
    activeDbSessionId: null as string | null,
    loadForConnection: async () => {},
    loadTables: async () => {},
    setCurrentDatabase: () => {},
  };
  return schemaStoreMockModule(state);
});

vi.mock('../../../stores/tableDataStore', () => {
  const state = { byPanel: new Map(), removePanel: () => {} };
  const useTableDataStore = (selector: (s: typeof state) => unknown) => selector(state);
  useTableDataStore.getState = () => state;
  return { useTableDataStore };
});

vi.mock('../../../lib/kvSlotState', () => ({ pruneKvSlotStates: () => {} }));

import { ContentView } from '../ContentView';

beforeEach(() => {
  workspace.isKvPanel = true;
});

afterEach(() => {
  cleanup();
});

describe('ContentView — the 48px toolbar band', () => {
  it('is not rendered for a KV panel', () => {
    // Enter condition: a panel is open, and only the KV flag differs below.
    workspace.isKvPanel = true;
    render(<ContentView />);
    expect(screen.queryByTestId('stub-content-toolbar')).toBeNull();
  });

  it('is still rendered for a relational panel', () => {
    // The guard must not be inverted: every SQL panel depends on this band.
    workspace.isKvPanel = false;
    render(<ContentView />);
    expect(screen.getByTestId('stub-content-toolbar')).toBeTruthy();
  });
});
