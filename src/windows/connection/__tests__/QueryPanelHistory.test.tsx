import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { QueryPanel } from '../QueryPanel';
import { usePanelStore } from '../../../stores/panelStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';
import type { QueryHistoryEntry } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 280, handleRef: { current: null } }),
}));

vi.mock('../../../hooks/useCompactToolbar', () => ({
  useCompactToolbar: () => ({ ref: { current: null }, compact: false }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown,
  ) => sel({ settings: { safeMode: false, autoCommit: true } }),
}));

// Mutable snapshot so tests can drive currentDatabase / isMultiDatabase.
const schemaSnapshot = {
  tables: [],
  views: [],
  columnMap: new Map(),
  namespaceTree: [],
  pathAliases: {},
  databases: ['app', 'analytics'],
  currentDatabase: 'app',
  isMultiDatabase: true,
  ensuringCount: 0,
  ensureColumns: vi.fn(),
  loadTables: vi.fn(),
  ensureNamespacePath: vi.fn(),
};

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (sel: (s: typeof schemaSnapshot) => unknown) => sel(schemaSnapshot),
}));

vi.mock('../../../components/SqlEditor', () => ({
  SqlEditor: () => <div data-testid="mock-sql-editor" />,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => null,
}));

vi.mock('../../../components/chart/ChartView', () => ({
  ChartView: () => null,
}));

vi.mock('../../../components/ai/Nl2SqlPanel', () => ({
  Nl2SqlPanel: () => null,
}));

vi.mock('../../../components/ai/DiagnosisPanel', () => ({
  DiagnosisPanel: () => null,
}));

vi.mock('../../../components/ai/ExplainPanel', () => ({
  ExplainPanel: () => null,
}));

vi.mock('../../../components/query/BindParamPanel', () => ({
  BindParamPanel: () => null,
}));

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));

vi.mock('../../../components/query/QueryErrorPanel', () => ({
  QueryErrorPanel: () => null,
}));

vi.mock('../dashboard/AddToDashboardDialog', () => ({
  AddToDashboardDialog: () => null,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const PANEL_ID = 'panel-history';

function entry(
  overrides: Partial<QueryHistoryEntry> &
    Pick<QueryHistoryEntry, 'id' | 'sql' | 'database' | 'executedAt'>,
): QueryHistoryEntry {
  return {
    connectionId: 'cfg-1',
    executionTimeMs: 12,
    success: true,
    ...overrides,
  };
}

// Real-shaped history: multiple entries across two databases, newest-first,
// with mixed RFC3339 precision (some timestamps carry milliseconds).
const HISTORY: QueryHistoryEntry[] = [
  entry({
    id: 'h-app-3',
    database: 'app',
    sql: 'SELECT * FROM app_orders',
    executedAt: '2024-01-02T00:00:00.250Z',
  }),
  entry({
    id: 'h-an-2',
    database: 'analytics',
    sql: 'SELECT * FROM analytics_events',
    executedAt: '2024-01-02T08:00:00.500Z',
  }),
  entry({
    id: 'h-app-2',
    database: 'app',
    sql: "UPDATE app_users SET name = 'x'",
    executedAt: '2024-01-01T12:00:00Z',
  }),
  entry({
    id: 'h-an-1',
    database: 'analytics',
    sql: 'CREATE TABLE dwd_sessions (id INT)',
    executedAt: '2024-01-01T09:30:00Z',
  }),
  entry({
    id: 'h-app-1',
    database: 'app',
    sql: 'DELETE FROM app_cache',
    executedAt: '2024-01-01T00:00:00Z',
  }),
];

function setState(overrides: Record<string, unknown> = {}) {
  usePanelStore.setState({
    queryExec: new Map([[PANEL_ID, { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', running: false }]]),
    historyVisible: true,
    favoritesVisible: false,
    queryHistory: HISTORY,
    queryFavorites: [],
    loadHistory: vi.fn().mockResolvedValue(undefined),
    loadFavorites: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as Partial<ReturnType<typeof usePanelStore.getState>>);
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  schemaSnapshot.currentDatabase = 'app';
  schemaSnapshot.isMultiDatabase = true;
});

function renderPanel() {
  return render(
    <QueryPanel
      panelId={PANEL_ID}
      dbSessionId="sess-cfg-1"
      connectionId="cfg-1"
      databaseType="postgresql"
    />,
  );
}

function searchInput() {
  return screen.getByLabelText('query.searchHistory') as HTMLInputElement;
}

function visibleSqls() {
  return screen
    .queryAllByText((_, el) => el?.classList.contains('selectable') === true)
    .map((el) => el.textContent);
}

describe('QueryPanel history sidebar', () => {
  it('defaults to current-database scope showing only that database without group headers', () => {
    setState();
    renderPanel();

    expect(visibleSqls()).toEqual([
      'SELECT * FROM app_orders',
      "UPDATE app_users SET name = 'x'",
      'DELETE FROM app_cache',
    ]);
    expect(screen.queryByText('SELECT * FROM analytics_events')).toBeNull();
    expect(screen.queryByTestId('history-group-label')).toBeNull();
    expect(screen.queryByTestId('history-scope-fallback-hint')).toBeNull();
    expect(screen.getByTestId('history-scope-current').getAttribute('aria-pressed')).toBe('true');
  });

  it('switching to all shows group headers with counts and every entry', () => {
    setState();
    renderPanel();
    fireEvent.click(screen.getByTestId('history-scope-all'));

    const headers = screen.getAllByTestId('history-group-label');
    expect(headers.map((h) => h.textContent)).toEqual(['analytics (2)', 'app (3)']);
    expect(visibleSqls()).toHaveLength(5);
    expect(screen.getByText('SELECT * FROM analytics_events')).toBeInTheDocument();
    expect(screen.getByTestId('history-scope-all').getAttribute('aria-pressed')).toBe('true');
  });

  it('search filters within the active scope', () => {
    setState();
    renderPanel();
    fireEvent.change(searchInput(), { target: { value: 'SELECT * FROM' } });

    expect(visibleSqls()).toEqual(['SELECT * FROM app_orders']);

    fireEvent.click(screen.getByTestId('history-scope-all'));
    expect(visibleSqls()).toEqual(['SELECT * FROM analytics_events', 'SELECT * FROM app_orders']);
    expect(screen.getAllByTestId('history-group-label').map((h) => h.textContent)).toEqual([
      'analytics (1)',
      'app (1)',
    ]);
  });

  it('shows fallback hint when current database has no history yet, keeping all-groups fallback', () => {
    schemaSnapshot.currentDatabase = 'reporting';
    setState();
    renderPanel();

    expect(screen.getByTestId('history-scope-fallback-hint')).toHaveTextContent(
      'query.historyScopeFallbackHint',
    );
    // Fallback behaviour preserved: all groups remain visible.
    expect(screen.getAllByTestId('history-group-label').map((h) => h.textContent)).toEqual([
      'analytics (2)',
      'app (3)',
    ]);

    fireEvent.click(screen.getByTestId('history-scope-all'));
    expect(screen.queryByTestId('history-scope-fallback-hint')).toBeNull();
  });

  it('never shows the fallback hint when history is empty', () => {
    setState({ queryHistory: [] });
    renderPanel();

    expect(screen.getByText('query.noHistory')).toBeInTheDocument();
    expect(screen.queryByTestId('history-scope-current')).toBeNull();
    expect(screen.queryByTestId('history-scope-fallback-hint')).toBeNull();
  });
});
