/**
 * [tester] Coverage-driven supplemental tests for the landing-page `home/`
 * components (track: landing-page-opt).
 *
 * Targets uncovered paths measured with `npx vitest run --coverage`:
 * - RecentQueriesList: keyboard activation, Copy hover action, rerun hidden
 *   without callback, source-name fallbacks, 5-row cap, invalid executedAt,
 *   empty state.
 * - ConnectionCardList: connecting-spinner button state, no-match import
 *   hint, paging footer hidden at or below the collapsed page size, filter
 *   against group / dbType label haystacks.
 * - ShortcutFooter: renders nothing when no candidate action has a shortcut.
 */
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectionCardList } from '../ConnectionCardList';
import { RecentQueriesList } from '../RecentQueriesList';
import { ShortcutFooter } from '../ShortcutFooter';
import { getActionShortcut, formatShortcutForDisplay } from '../../../../lib/keymap';
import type { ConnectionConfig, QueryHistoryEntry } from '../../../../types';
import type { ConnectionEntry } from '../../../../stores/activeConnectionStore';

vi.mock('../../../../hooks/useI18n', () => ({
  // Key-style mock (same convention as ConnectionWorkspaceHome.test.tsx):
  // returns the key itself, appending params so placeholders stay assertable.
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (!params) return key;
      const suffix = Object.values(params).map(String).join(' ');
      return suffix ? `${key} ${suffix}` : key;
    },
  }),
}));

vi.mock('../../../../lib/databaseTypes', () => ({
  getDbIcon: () => ({ label: 'Db', bg: 'bg-blue-500' }),
  getDbLabel: (type: string) => type,
  getDriverIconMap: () => ({}),
  getDriverIconParents: () => ({}),
}));

vi.mock('../../../../lib/keymap', () => ({
  getActionShortcut: vi.fn((action: string) => (action === 'newQuery' ? 'Mod-n' : 'Mod-s')),
  formatShortcutForDisplay: vi.fn((key: string) => key),
  toShortcutHookFormat: vi.fn((key: string) => key.toLowerCase()),
  toCodeMirrorKeyFormat: vi.fn((key: string) => key),
}));

afterEach(cleanup);

const connections: ConnectionConfig[] = [
  {
    id: 'conn-1',
    name: 'PostgreSQL-Local',
    databaseType: 'postgresql',
    host: 'localhost',
    port: 5432,
    database: 'postgres',
    group: 'Development',
    sslMode: 'prefer',
  },
  {
    id: 'conn-2',
    name: 'MySQL-Prod',
    databaseType: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    database: 'app',
    group: 'Production',
    sslMode: 'prefer',
  },
];

function entry(overrides: Partial<ConnectionEntry> = {}): ConnectionEntry {
  return {
    dbSessionId: 'session-1',
    connectionId: 'conn-1',
    status: 'connected',
    serverInfo: null,
    currentDatabase: 'postgres',
    error: null,
    ...overrides,
  };
}

function historyEntry(overrides: Partial<QueryHistoryEntry> = {}): QueryHistoryEntry {
  return {
    id: 'hist-1',
    connectionId: 'conn-1',
    database: 'postgres',
    sql: 'SELECT 1;',
    executedAt: new Date().toISOString(),
    executionTimeMs: 12,
    success: true,
    ...overrides,
  };
}

describe('[tester] RecentQueriesList uncovered paths', () => {
  const saved = connections;
  const onSelectHistoryQuery = vi.fn();
  const onOpenHistory = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
  });

  it('[tester] activates a history row via keyboard Enter and Space', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry()]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    const row = screen.getByText('SELECT 1;').closest('[role="button"]');
    expect(row).not.toBeNull();

    fireEvent.keyDown(row!, { key: 'Enter' });
    expect(onSelectHistoryQuery).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(row!, { key: ' ' });
    expect(onSelectHistoryQuery).toHaveBeenCalledTimes(2);

    // Other keys must not trigger activation.
    fireEvent.keyDown(row!, { key: 'Escape' });
    expect(onSelectHistoryQuery).toHaveBeenCalledTimes(2);
  });

  it('[tester] activates a history row via mouse click', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry()]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    fireEvent.click(screen.getByText('SELECT 1;').closest('[role="button"]')!);
    expect(onSelectHistoryQuery).toHaveBeenCalledWith(expect.objectContaining({ id: 'hist-1' }));
  });

  it('[tester] copies SQL from the per-row copy action and shows the copied state', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry()]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    fireEvent.click(screen.getByTestId('home-query-copy-hist-1'));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('SELECT 1;');
    expect(screen.getByText('connWin.home.aiIntegration.copied')).toBeInTheDocument();
    // Row-level onClick must not fire from the copy button.
    expect(onSelectHistoryQuery).not.toHaveBeenCalled();
  });

  it('[tester] hides the rerun action when onSelectHistoryQuery is not provided', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry()]}
        savedConnections={saved}
        onOpenHistory={onOpenHistory}
      />,
    );

    expect(screen.queryByTestId('home-query-rerun-hist-1')).not.toBeInTheDocument();
  });

  it('[tester] falls back to database name and then "default" for unknown sources', () => {
    render(
      <RecentQueriesList
        entries={[
          historyEntry({ id: 'hist-unknown-conn', connectionId: 'ghost-conn' }),
          historyEntry({ id: 'hist-no-db', connectionId: 'ghost-conn', database: '' }),
        ]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    expect(screen.getByTestId('home-query-source-hist-unknown-conn').textContent).toBe('postgres');
    expect(screen.getByTestId('home-query-source-hist-no-db').textContent).toBe('default');
  });

  it('[tester] renders at most five history rows', () => {
    const entries = Array.from({ length: 8 }, (_, i) =>
      historyEntry({ id: `hist-${i}`, sql: `SELECT ${i};` }),
    );
    render(
      <RecentQueriesList
        entries={entries}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    expect(screen.getByText('SELECT 0;')).toBeInTheDocument();
    expect(screen.getByText('SELECT 4;')).toBeInTheDocument();
    expect(screen.queryByText('SELECT 5;')).not.toBeInTheDocument();
  });

  it('[tester] omits the relative-time segment when executedAt is invalid', () => {
    render(
      <RecentQueriesList
        entries={[historyEntry({ executedAt: 'not-a-date' })]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    // SQL still renders; no time separator/segment for the invalid timestamp.
    expect(screen.getByText('SELECT 1;')).toBeInTheDocument();
    expect(screen.queryByText('just now')).not.toBeInTheDocument();
  });

  it('[tester] shows the empty state when there is no history', () => {
    render(
      <RecentQueriesList
        entries={[]}
        savedConnections={saved}
        onSelectHistoryQuery={onSelectHistoryQuery}
        onOpenHistory={onOpenHistory}
      />,
    );

    expect(screen.getByText('connWin.home.noRecentQueries')).toBeInTheDocument();
    expect(screen.getByTestId('view-all-history-button')).toBeInTheDocument();
  });
});

describe('[tester] ConnectionCardList uncovered paths', () => {
  const onConnect = vi.fn();
  const onNewConnection = vi.fn();
  const onImportConnections = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('[tester] renders a disabled spinner without a connect testid while connecting', () => {
    render(
      <ConnectionCardList
        connections={connections}
        activeConnections={{ 'conn-1': entry({ status: 'connecting' }) }}
        onConnect={onConnect}
        onNewConnection={onNewConnection}
        onImportConnections={onImportConnections}
      />,
    );

    // The explicit Connect/Open affordance is replaced by a disabled spinner.
    expect(screen.queryByTestId('home-conn-connect-conn-1')).not.toBeInTheDocument();
    const spinner = screen.getByRole('button', { name: 'conn.connecting' });
    expect(spinner).toBeDisabled();
    expect(screen.getByText('conn.connecting')).toBeInTheDocument(); // status pill
  });

  it('[tester] shows the import hint inside the no-match empty state', () => {
    render(
      <ConnectionCardList
        connections={connections}
        activeConnections={{}}
        onConnect={onConnect}
        onNewConnection={onNewConnection}
        onImportConnections={onImportConnections}
      />,
    );

    fireEvent.change(screen.getByTestId('home-connections-filter'), {
      target: { value: 'zzz-none' },
    });
    expect(screen.getByTestId('home-connections-no-match')).toBeInTheDocument();
    // The hint paragraph is a sibling of the noMatch line inside the empty block.
    expect(screen.getByTestId('home-connections-no-match').parentElement?.textContent).toContain(
      'common.importConnections',
    );
  });

  it('[tester] hides the paging footer when the list fits the collapsed page size', () => {
    render(
      <ConnectionCardList
        connections={connections}
        activeConnections={{}}
        onConnect={onConnect}
        onNewConnection={onNewConnection}
      />,
    );

    expect(screen.queryByTestId('home-connections-show-all')).not.toBeInTheDocument();
    expect(screen.queryByText(/connWin\.home\.connections\.showing/)).not.toBeInTheDocument();
  });

  it('[tester] filters connections by group and by db type label', () => {
    render(
      <ConnectionCardList
        connections={connections}
        activeConnections={{}}
        onConnect={onConnect}
        onNewConnection={onNewConnection}
      />,
    );

    const filter = screen.getByTestId('home-connections-filter');
    fireEvent.change(filter, { target: { value: 'production' } });
    expect(screen.getByTestId('home-conn-card-conn-2')).toBeInTheDocument();
    expect(screen.queryByTestId('home-conn-card-conn-1')).not.toBeInTheDocument();

    fireEvent.change(filter, { target: { value: 'postgresql' } });
    expect(screen.getByTestId('home-conn-card-conn-1')).toBeInTheDocument();
    expect(screen.queryByTestId('home-conn-card-conn-2')).not.toBeInTheDocument();
  });
});

describe('[tester] ShortcutFooter keymap-driven rendering', () => {
  it('[tester] renders nothing when no candidate action has a registered shortcut', () => {
    vi.mocked(getActionShortcut).mockReturnValue('');
    const { container } = render(<ShortcutFooter />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId('home-shortcut-footer')).not.toBeInTheDocument();
  });

  it('[tester] renders one entry per registered action with display formatting', () => {
    // Only three of the keymap actions have a shortcut here; the footer asks
    // about more actions than that, so the table is deliberately partial.
    const shortcuts: Partial<Record<string, string>> = {
      newQuery: 'Mod-n',
      saveQuery: 'Mod-s',
      formatSql: '',
    };
    vi.mocked(getActionShortcut).mockImplementation((action) => shortcuts[action] ?? '');
    vi.mocked(formatShortcutForDisplay).mockImplementation((key) => `⌘${key}`);
    render(<ShortcutFooter />);

    const footer = screen.getByTestId('home-shortcut-footer');
    expect(footer).toBeInTheDocument();
    // formatSql is skipped (empty shortcut), the other two render in order.
    expect(footer.textContent).toContain('⌘Mod-n');
    expect(footer.textContent).toContain('keymap.action.newQuery');
    expect(footer.textContent).toContain('⌘Mod-s');
    expect(footer.textContent).toContain('keymap.action.saveQuery');
    expect(footer.textContent).not.toContain('formatSql');
  });
});
