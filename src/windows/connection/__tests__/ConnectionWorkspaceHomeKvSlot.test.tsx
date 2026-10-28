import { render, screen, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ComponentType } from 'react';
import type { ConnectionHomeSlotProps } from '@datazen/driver-sdk';
import { ConnectionWorkspaceHome } from '../ConnectionWorkspaceHome';
import type { KvConnectionHomeBinding } from '../useKvWorkspaceSlots';
import type { ConnectionContext, Panel } from '../../../stores/panelStore';

// Key-style mock: assertions target i18n keys and data attributes, never copy.
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  getDbIcon: () => ({ label: 'Fx', bg: 'bg-ink-900', iconBg: 'bg-ink-900', iconColor: '' }),
  getDbLabel: (type: string) => type,
  getDriverIconParents: () => ({}),
}));

vi.mock('../contentViewHelpers', () => ({
  getPanelIcon: () => null,
  getPanelLabel: (panel: Panel) => panel.type,
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: { getQueryHistoryPage: vi.fn().mockResolvedValue({ entries: [], total: 0 }) },
}));

const kvContext: ConnectionContext = {
  connectionId: 'cfg-1',
  dbSessionId: 'conn-1',
  connectionName: 'Redis local',
  databaseType: 'redis',
};

/** Fixture standing in for a driver's connection home (屏 A): props echoed as data. */
function FixtureConnectionHome({
  connectionId,
  dbSessionId,
  connectionName,
  databaseType,
  initialDatabase,
}: ConnectionHomeSlotProps) {
  return (
    <div
      data-testid="fixture-connection-home"
      data-connection-id={connectionId}
      data-db-session-id={dbSessionId}
      data-connection-name={connectionName}
      data-database-type={databaseType}
      data-initial-database={initialDatabase ?? ''}
    />
  );
}

const homeSlot: KvConnectionHomeBinding = {
  Component: FixtureConnectionHome as ComponentType<ConnectionHomeSlotProps>,
  props: {
    connectionId: 'cfg-1',
    dbSessionId: 'conn-1',
    connectionName: 'Redis local',
    databaseType: 'redis',
    initialDatabase: 'db3',
  },
};

function renderHome(connectionHomeSlot?: KvConnectionHomeBinding) {
  return render(
    <ConnectionWorkspaceHome
      hasConnections
      connectionContext={kvContext}
      recentPanels={[]}
      showNewQuery
      showNewTable={false}
      showErDiagram={false}
      showObjects={false}
      onNewConnection={vi.fn()}
      onNewQuery={vi.fn()}
      onCreateTable={vi.fn()}
      onOpenErDiagram={vi.fn()}
      onOpenObjects={vi.fn()}
      onOpenPanel={vi.fn()}
      connectionHomeSlot={connectionHomeSlot}
    />,
  );
}

afterEach(cleanup);

describe('ConnectionWorkspaceHome connection-home slot', () => {
  it('yields the connected landing screen to a driver-contributed connection home', () => {
    renderHome(homeSlot);

    const slot = screen.getByTestId('home-kv-connection-home');
    expect(slot.getAttribute('data-slot')).toBe('kv-connection-home');
    const fixture = screen.getByTestId('fixture-connection-home');
    expect(fixture.getAttribute('data-connection-id')).toBe('cfg-1');
    expect(fixture.getAttribute('data-db-session-id')).toBe('conn-1');
    expect(fixture.getAttribute('data-initial-database')).toBe('db3');
    // 屏 A takeover is wholesale: the host banner page (hero + quick actions) is gone,
    // while the landing-screen test id survives so outer wiring keeps working.
    expect(screen.queryByText('common.newQuery')).not.toBeInTheDocument();
    expect(screen.getByTestId('connection-workspace-home')).toBeInTheDocument();
  });

  it('keeps the host banner page when the driver contributes no connection home', () => {
    renderHome();

    expect(screen.queryByTestId('home-kv-connection-home')).not.toBeInTheDocument();
    expect(screen.getByText('common.newQuery')).toBeInTheDocument();
  });

  // [tester] The takeover is gated on a *live session*, not just on a binding
  // existing: `useKvWorkspaceSlots` only builds one from `connectionContext`, but a
  // stale/mismatched caller must never swap the driver home onto the connection
  // picker (State 3) or the empty state (State 1).
  it('[tester] does not hand over the landing screen without a connection context', () => {
    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={null}
        recentPanels={[]}
        showNewQuery
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        onNewConnection={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
        connectionHomeSlot={homeSlot}
      />,
    );

    expect(screen.queryByTestId('home-kv-connection-home')).not.toBeInTheDocument();
    expect(screen.queryByTestId('fixture-connection-home')).not.toBeInTheDocument();
    expect(screen.getByTestId('connection-workspace-home')).toBeInTheDocument();
  });
});
