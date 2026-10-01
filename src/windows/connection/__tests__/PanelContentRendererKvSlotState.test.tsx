/**
 * [tester] F-2 wiring check: the host hands a KV panel's connection view the very
 * same `KvSlotState` object it hands the in-panel slots, so the workbench's
 * `selectKey` / `setDirty` publishes reach the context bar / status bar / sidebar.
 *
 * `useKvWorkspaceSlots.test.tsx` already proves `panelState === contextBar.props.state`
 * (one object, three slots). This file proves the other half of the chain:
 * `PanelContentRenderer` forwards the relay it is given, untouched, to the view
 * returned by `getConnectionView()`.
 */
import { render, screen, cleanup } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ComponentType } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';
import { PanelContentRenderer } from '../PanelContentRenderer';
import { createKvSlotState } from '../../../lib/kvSlotState';
import type { DatabaseTypeMeta } from '../../../lib/databaseMeta';
import type { Panel } from '../../../stores/panelStore';

const { registry, receivedState } = vi.hoisted(() => ({
  registry: {} as Record<string, unknown>,
  receivedState: { current: undefined as unknown },
}));

vi.mock('../../../hooks/useI18n', () => ({
  // Key-style mock: nothing here asserts visible copy.
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  DB_REGISTRY: registry,
  getDbLabel: (type: string) => type,
}));

/** The connection view is the only thing the KV branch renders. */
vi.mock('../../../lib/connectionViews', () => ({
  getConnectionView: (): ComponentType<Record<string, unknown>> =>
    function MockKvView(props) {
      const state = props.kvSlotState as KvSlotState | undefined;
      receivedState.current = state;
      return (
        <div
          data-testid="mock-kv-view"
          data-session={String(props.dbSessionId)}
          data-has-state={state ? 'yes' : 'no'}
          data-selected-key={state?.getSelectedKey() ?? ''}
          data-dirty={String(state?.getDirty() ?? false)}
        />
      );
    },
}));
vi.mock('../../../lib/connectionViews/types', () => ({}));

// The SQL siblings are imported by the same file but never rendered on the KV path;
// each factory must stay self-contained (`vi.mock` is hoisted above `const stub`).
vi.mock('../TableView', () => ({ TableView: () => null }));
vi.mock('../StructureView', () => ({ StructureView: () => null }));
vi.mock('../IndexesView', () => ({ IndexesView: () => null }));
vi.mock('../ForeignKeysView', () => ({ ForeignKeysView: () => null }));
vi.mock('../DDLView', () => ({ DDLView: () => null }));
vi.mock('../QueryPanel', () => ({ QueryPanel: () => null }));
vi.mock('../TableStructureEditor', () => ({ TableStructureEditor: () => null }));
vi.mock('../ErDiagramView', () => ({ ErDiagramView: () => null }));
vi.mock('../ObjectBrowser', () => ({ ObjectBrowser: () => null }));
vi.mock('../DatabaseObjectView', () => ({ DatabaseObjectView: () => null }));
vi.mock('../PrivilegeView', () => ({ PrivilegeView: () => null }));
vi.mock('../ProcessListView', () => ({ ProcessListView: () => null }));
vi.mock('../ServerStatusView', () => ({ ServerStatusView: () => null }));

vi.mock('../../../stores/schemaStore', async () => {
  // See ConnectionPage.test.tsx: the per-session selectors are part of the real
  // module's surface, so the mock reproduces them via the shared double.
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  const perSession = schemaStoreMockModule({ schemas: new Map(), activeDbSessionId: null });
  return {
    useSchemaStore: Object.assign((sel: (s: Record<string, unknown>) => unknown) => sel({}), {
      getState: () => ({}),
    }),
    useConnectionSchemaField: perSession.useConnectionSchemaField,
    useConnectionColumnMaps: perSession.useConnectionColumnMaps,
  };
});
vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: { connections: unknown[] }) => unknown) => sel({ connections: [] }),
}));

const KV_PANEL = {
  id: 'panel-kv-1',
  type: 'redis-db',
  connectionId: 'cfg-1',
  dbSessionId: 'sess-1',
  connectionName: 'KV Local',
  databaseType: 'kvredis',
  dbName: 'db7',
} as unknown as Panel;

const KV_META: DatabaseTypeMeta = {
  label: 'Fixture KV',
  shortLabel: 'FK',
  iconBg: 'bg-red-600',
  iconColor: 'text-red-400',
  defaultPort: 0,
  defaultHost: '127.0.0.1',
  defaultUser: '',
  quoteChar: '`',
  connectionMode: 'server',
  supportsSSH: false,
  supportsSSL: false,
  supportsBackup: false,
  supportsTables: false,
  isKeyValue: true,
  supportsSQL: false,
  category: 'kv',
  connectionView: 'keyvalue',
  connectionForm: 'standard',
  // Required by DatabaseTypeMeta; this renderer only reads `isKeyValue` /
  // `connectionView`, and a non-'index' value keeps any index-addressed branch
  // (e.g. `resolveKvDatabaseIndex`) on the same path the absent field took.
  databaseFieldType: 'name',
};

function renderRenderer(kvSlotState?: KvSlotState) {
  return render(
    <PanelContentRenderer
      activePanel={KV_PANEL}
      currentDatabase="db7"
      lastTableSchema={null}
      onSetSubTab={vi.fn()}
      onExitStructureEditing={vi.fn()}
      onEditTableStructure={vi.fn()}
      onSelectTable={vi.fn()}
      onOpenErDiagram={vi.fn()}
      onClosePanel={vi.fn()}
      onRefresh={vi.fn()}
      resolveTableSchema={() => null}
      onUpdatePanelData={vi.fn()}
      kvSlotState={kvSlotState}
    />,
  );
}

beforeEach(() => {
  registry.kvredis = KV_META;
});

afterEach(() => {
  cleanup();
});

describe('[tester] PanelContentRenderer KV state relay', () => {
  it('forwards the host atom to the driver connection view, identity intact', () => {
    const atom = createKvSlotState();
    atom.selectKey('app:cache:session:1');
    atom.setDirty(true);
    receivedState.current = undefined;

    renderRenderer(atom);

    expect(receivedState.current).toBe(atom);
    const view = screen.getByTestId('mock-kv-view');
    // Identity, not structural equality: a re-created atom would silently fork the
    // publish (workbench) and read (slots) sides of the relay.
    expect(view.getAttribute('data-has-state')).toBe('yes');
    expect(view.getAttribute('data-selected-key')).toBe('app:cache:session:1');
    expect(view.getAttribute('data-dirty')).toBe('true');
    expect(view.getAttribute('data-session')).toBe('sess-1');
  });

  it('renders the KV view without a relay while the driver declares no capability', () => {
    // Wave-2 pre-state: a KV panel exists, but nothing claimed a KV slot, so the
    // host hands over `undefined` and the driver view keeps working as today.
    renderRenderer(undefined);

    const view = screen.getByTestId('mock-kv-view');
    expect(view.getAttribute('data-has-state')).toBe('no');
  });
});
