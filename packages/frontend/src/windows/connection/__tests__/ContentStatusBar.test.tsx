import { render, screen, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { ContentStatusBar } from '../ContentStatusBar';
import type { KvStatusBarBinding } from '../useKvWorkspaceSlots';
import type { KvStatusBarProps } from '@datazen/driver-sdk';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const usePlatformMock = vi.fn(() => 'macos' as 'macos' | 'windows' | 'linux' | 'unknown');

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => usePlatformMock(),
}));

beforeEach(() => {
  usePlatformMock.mockReturnValue('macos');
});

afterEach(() => {
  cleanup();
});

describe('ContentStatusBar', () => {
  it('exposes status semantics and the active database context', () => {
    render(
      <ContentStatusBar
        databaseType="redis"
        connectionName="Redis local"
        currentDatabase="db5"
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('connWin.connected');
    expect(status).toHaveTextContent('Redis local · db5');
    expect(status.querySelector('.bg-success')).not.toBeNull();
  });

  it('shows Mac modifier labels on macOS', () => {
    usePlatformMock.mockReturnValue('macos');
    render(<ContentStatusBar currentDatabase={null} tableName="" columnCount={0} totalRows={0} />);

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('⌘N');
    expect(status).toHaveTextContent('⌘W');
  });

  it('shows Ctrl modifier labels on Windows and Linux', () => {
    usePlatformMock.mockReturnValue('windows');
    render(<ContentStatusBar currentDatabase={null} tableName="" columnCount={0} totalRows={0} />);

    let status = screen.getByRole('status');
    expect(status).toHaveTextContent('Ctrl+N');
    expect(status).toHaveTextContent('Ctrl+W');

    cleanup();
    usePlatformMock.mockReturnValue('linux');
    render(<ContentStatusBar currentDatabase={null} tableName="" columnCount={0} totalRows={0} />);

    status = screen.getByRole('status');
    expect(status).toHaveTextContent('Ctrl+N');
    expect(status).toHaveTextContent('Ctrl+W');
  });

  it('[tester] shows Ctrl modifier labels when platform is unknown', () => {
    usePlatformMock.mockReturnValue('unknown');
    render(<ContentStatusBar currentDatabase={null} tableName="" columnCount={0} totalRows={0} />);

    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('Ctrl+N');
    expect(status).toHaveTextContent('Ctrl+W');
  });

  it('hands the driver status cluster the frozen KV props and keeps the surrounding footer', () => {
    const state = {
      subscribe: () => () => {},
      getSelectedKey: () => null,
      selectKey: vi.fn(),
      getDirty: () => false,
      setDirty: vi.fn(),
      // W3-A widened relay members: this case only pins the frozen KV props, so
      // the rest of the atom stays at its documented neutral values.
      getLoadedCount: () => 0,
      setLoadedCount: vi.fn(),
      getScanCursor: () => '0',
      setScanCursor: vi.fn(),
      isScanning: () => false,
      setScanning: vi.fn(),
      getScanBudgetUsed: () => 0,
      getScanBudgetTotal: () => 0,
      setScanBudget: vi.fn(),
      getSelectionCount: () => 0,
      setSelectionCount: vi.fn(),
      getLastWriteCommand: () => null,
      getLastWriteDurationMs: () => null,
      recordWrite: vi.fn(),
    };
    const statusBarSlot: KvStatusBarBinding = {
      Component: FixtureStatusBar,
      props: {
        connectionId: 'cfg-1',
        dbSessionId: 'sess-1',
        connectionName: 'KV Local',
        databaseType: 'redis' as never,
        database: 'db5',
        dbIndex: 5,
        state,
      },
    };

    render(
      <ContentStatusBar
        databaseType="redis"
        connectionName="Redis local"
        currentDatabase="db5"
        tableName=""
        columnCount={0}
        totalRows={0}
        statusBarSlot={statusBarSlot}
      />,
    );

    const cluster = screen.getByTestId('conn-status-kv-bar');
    expect(cluster.getAttribute('data-slot')).toBe('kv-status-bar');
    const fixture = screen.getByTestId('fixture-status-bar');
    expect(fixture.getAttribute('data-connection-id')).toBe('cfg-1');
    expect(fixture.getAttribute('data-db-session-id')).toBe('sess-1');
    expect(fixture.getAttribute('data-db-index')).toBe('5');
    // The slot only replaces the centre cluster: left status and shortcut hints stay.
    expect(screen.getByRole('status').textContent).toContain('connWin.connected');
    expect(screen.getByRole('status').textContent).toContain('⌘N');
    // The SQL-oriented joined metadata (label · connection · database) is gone.
    expect(screen.getByRole('status').textContent).not.toContain('Redis local');
  });

  it('keeps the joined metadata centre when the driver contributes no status bar', () => {
    render(
      <ContentStatusBar
        databaseType="redis"
        connectionName="Redis local"
        currentDatabase="db5"
        tableName=""
        columnCount={0}
        totalRows={0}
      />,
    );

    expect(screen.queryByTestId('conn-status-kv-bar')).not.toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Redis local · db5');
  });
});

/** Fixture standing in for a driver's KV status cluster (props echoed as data). */
function FixtureStatusBar({
  connectionId,
  dbSessionId,
  database,
  dbIndex,
  state,
}: KvStatusBarProps) {
  return (
    <div
      data-testid="fixture-status-bar"
      data-connection-id={connectionId}
      data-db-session-id={dbSessionId}
      data-database={database ?? ''}
      data-db-index={dbIndex ?? ''}
      data-has-state={state ? 'yes' : 'no'}
    />
  );
}
