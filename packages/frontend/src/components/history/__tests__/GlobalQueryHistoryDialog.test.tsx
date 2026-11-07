import { render, screen, fireEvent, waitFor, cleanup, within } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { GlobalQueryHistoryDialog } from '../GlobalQueryHistoryDialog';
import { queryCommands } from '../../../commands/query';
import { useConnectionStore } from '../../../stores/connectionStore';
import type { QueryHistoryEntry } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  // Backed by the real English pack, so an assertion on rendered text checks
  // the string users see. A key-only mock would happily pass while the app
  // renders a raw key for a key that was never added to `en/`.
  useI18n: () => ({ t: realT }),
}));

import enPack from '../../../locales/en/query';

function realT(k: string, params?: Record<string, string | number>): string {
  const table = enPack as unknown as Record<string, string>;
  const template = table[k] ?? k;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (m, n: string) =>
    params[n] !== undefined ? String(params[n]) : m,
  );
}

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    getQueryHistoryPage: vi.fn(),
    clearQueryHistory: vi.fn(),
    deleteQueryHistoryEntry: vi.fn(),
    addFavoriteQuery: vi.fn(),
    saveSqlFile: vi.fn(),
  },
}));

const page = (entries: QueryHistoryEntry[], total?: number) => ({
  entries,
  total: total ?? entries.length,
});

describe('GlobalQueryHistoryDialog', () => {
  const mockEntries: QueryHistoryEntry[] = [
    {
      id: 'entry-1',
      connectionId: 'conn-1',
      database: 'db_a',
      schema: null,
      sql: 'SELECT * FROM users',
      executedAt: '2026-09-07T10:00:00Z',
      executionTimeMs: 12,
      rowsAffected: 5,
      success: true,
    },
    {
      id: 'entry-2',
      connectionId: 'conn-2',
      database: 'db_b',
      schema: null,
      sql: 'INSERT INTO orders VALUES (1)',
      executedAt: '2026-09-07T11:00:00Z',
      executionTimeMs: 40,
      rowsAffected: 1,
      success: true,
    },
    {
      id: 'entry-3',
      connectionId: 'conn-1',
      database: 'db_a',
      schema: null,
      sql: 'SELECT * FROM bad_table',
      executedAt: '2026-09-07T11:30:00Z',
      executionTimeMs: 5,
      success: false,
      errorMessage: 'relation "bad_table" does not exist',
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    useConnectionStore.setState({
      connections: [
        {
          id: 'conn-1',
          name: 'Primary Postgres',
          databaseType: 'postgresql',
          host: 'localhost',
          port: 5432,
          sslMode: 'prefer',
        },
        {
          id: 'conn-2',
          name: 'Secondary MySQL',
          databaseType: 'mysql',
          host: 'localhost',
          port: 3306,
          sslMode: 'prefer',
        },
      ],
    });
    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue(page(mockEntries));
    vi.mocked(queryCommands.deleteQueryHistoryEntry).mockResolvedValue(1);
    vi.mocked(queryCommands.addFavoriteQuery).mockResolvedValue({
      id: 'fav-1',
      connectionId: 'conn-1',
      title: 't',
      sql: 's',
    } as never);
    vi.mocked(queryCommands.saveSqlFile).mockResolvedValue(true);
  });

  /** The row for a given id, located through its data attribute, never by index. */
  const row = async (id: string) => {
    await screen.findByText('SELECT * FROM users');
    return document.querySelector(`[data-history-id="${id}"]`) as HTMLElement;
  };

  /**
   * Drive the custom listbox Select. `Select` binds selection on `mouseDown`,
   * and the wrapper `data-testid` locates the real trigger — never an index.
   */
  const chooseOption = async (control: string, optionName: RegExp) => {
    const wrap = document.querySelector(`[data-testid="${control}"]`) as HTMLElement;
    fireEvent.click(within(wrap).getByRole('button'));
    fireEvent.mouseDown(await screen.findByRole('option', { name: optionName }));
  };

  it('renders history entries when open', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    expect(await screen.findByText('SELECT * FROM users')).toBeInTheDocument();
    expect(screen.getByText('INSERT INTO orders VALUES (1)')).toBeInTheDocument();
    expect(screen.getByText('SELECT * FROM bad_table')).toBeInTheDocument();
    expect(screen.getAllByText('Primary Postgres').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Secondary MySQL').length).toBeGreaterThanOrEqual(1);
  });


  // ── C: search must reach the backend, and truncation must be disclosed ──

  it('sends the search term to the backend instead of filtering a capped page locally', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');

    fireEvent.change(screen.getByPlaceholderText('Search history…'), {
      target: { value: 'orders' },
    });

    await waitFor(() => {
      const last = vi.mocked(queryCommands.getQueryHistoryPage).mock.calls.at(-1);
      expect(last?.[0].search).toBe('orders');
    });
  });

  it('shows "showing N of M" when the backend returned fewer rows than it matched', async () => {
    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue(page([mockEntries[0]], 57));
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    const notice = await screen.findByTestId('history-truncation-notice');
    expect(notice).toHaveTextContent('Showing 1 of 57');
  });

  it('shows no truncation notice when the page is the whole match set', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');
    expect(screen.queryByTestId('history-truncation-notice')).not.toBeInTheDocument();
  });

  it('does not claim a truncated set is complete when a search finds nothing', async () => {
    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue(page([], 0));
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    // An empty *page* with a non-zero total is the exact lie C described.
    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue(page([], 12));
    fireEvent.change(screen.getByPlaceholderText('Search history…'), {
      target: { value: 'nothing' },
    });

    await waitFor(() => {
      expect(screen.getByTestId('history-truncation-notice')).toBeInTheDocument();
    });
  });

  // ── B: per-entry delete, favorite bridge, export ────────────────────────

  it('deletes a single entry by id, not the whole table', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    const target = await row('entry-2');

    fireEvent.click(within(target).getByTitle('Delete this entry'));

    await waitFor(() => {
      expect(queryCommands.deleteQueryHistoryEntry).toHaveBeenCalledWith('entry-2');
    });
    expect(queryCommands.deleteQueryHistoryEntry).toHaveBeenCalledTimes(1);
    expect(queryCommands.clearQueryHistory).not.toHaveBeenCalled();
  });

  it('bridges an entry into favorites with the entry connection and sql', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    const target = await row('entry-1');

    fireEvent.click(within(target).getByTitle('Save to favorites'));

    await waitFor(() => {
      expect(queryCommands.addFavoriteQuery).toHaveBeenCalledWith(
        'conn-1',
        'Primary Postgres',
        'SELECT * FROM users',
      );
    });
  });

  it('exports the selected rows as one .sql file and writes no unselected row', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    const target = await row('entry-1');

    fireEvent.click(within(target).getByTestId('global-history-select'));
    await waitFor(() => expect(screen.getByTestId('global-history-export')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('global-history-export'));

    await waitFor(() => expect(queryCommands.saveSqlFile).toHaveBeenCalled());
    const [name, content] = vi.mocked(queryCommands.saveSqlFile).mock.calls[0];
    expect(name).toBe('query.sql');
    expect(content).toContain('SELECT * FROM users');
    expect(content).not.toContain('INSERT INTO orders');
  });

  it('does not export when nothing is selected', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');
    expect(screen.getByTestId('global-history-export')).toBeDisabled();
  });

  it('stays silent about the export when the user cancels the save dialog', async () => {
    vi.mocked(queryCommands.saveSqlFile).mockResolvedValue(false);
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    const target = await row('entry-1');

    fireEvent.click(within(target).getByTestId('global-history-select'));
    await waitFor(() => expect(screen.getByTestId('global-history-export')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('global-history-export'));

    await waitFor(() => expect(queryCommands.saveSqlFile).toHaveBeenCalled());
    expect(screen.queryByText(/Exported/)).not.toBeInTheDocument();
  });

  // ── Retained behaviour ────────────────────────────────────────────────

  it('invokes onSelectQuery and onClose when open in query is clicked', async () => {
    const onSelectQuery = vi.fn();
    const onClose = vi.fn();

    render(<GlobalQueryHistoryDialog open onClose={onClose} onSelectQuery={onSelectQuery} />);
    const target = await row('entry-1');

    fireEvent.click(within(target).getByTitle('Open in query panel'));

    expect(onSelectQuery).toHaveBeenCalledWith(mockEntries[0]);
    expect(onClose).toHaveBeenCalled();
  });

  it('hides the open action when there is nowhere to open into', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    const target = await row('entry-1');
    expect(within(target).queryByTitle('query.historyOpenTitle')).not.toBeInTheDocument();
  });

  it('supports clearing history behind a confirmation', async () => {
    vi.mocked(queryCommands.clearQueryHistory).mockResolvedValue(undefined);
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');

    fireEvent.click(screen.getByTitle('Clear query history'));
    expect(queryCommands.clearQueryHistory).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(queryCommands.clearQueryHistory).toHaveBeenCalled();
    });
  });

  it('abandons the clear when the confirmation is cancelled', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');

    fireEvent.click(screen.getByTitle('Clear query history'));
    fireEvent.click(screen.getByText('Cancel'));

    expect(queryCommands.clearQueryHistory).not.toHaveBeenCalled();
  });

  it('applies the status filter locally, since the backend has no column for it', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');
    const callsBefore = vi.mocked(queryCommands.getQueryHistoryPage).mock.calls.length;

    // The failed statement must disappear without any new IPC round trip.
    await chooseOption('global-history-status', /Failed/);

    expect(screen.getByText('SELECT * FROM bad_table')).toBeInTheDocument();
    expect(screen.queryByText('SELECT * FROM users')).not.toBeInTheDocument();
    expect(vi.mocked(queryCommands.getQueryHistoryPage).mock.calls).toHaveLength(callsBefore);
  });

  it('sends the chosen time range to the backend as a lower bound', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');

    await chooseOption('global-history-range', /Last 7 days/);

    await waitFor(() => {
      const last = vi.mocked(queryCommands.getQueryHistoryPage).mock.calls.at(-1);
      expect(last?.[0].since).toEqual(expect.any(String));
    });
    expect(vi.mocked(queryCommands.getQueryHistoryPage).mock.calls.at(-1)?.[0].search).toBeNull();
  });

  it('sends the chosen sort order to the backend', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    await screen.findByText('SELECT * FROM users');

    await chooseOption('global-history-sort', /Slowest first/);

    await waitFor(() => {
      expect(vi.mocked(queryCommands.getQueryHistoryPage).mock.calls.at(-1)?.[0].order).toBe(
        'slowest',
      );
    });
  });

  it('survives a backend read failure without crashing', async () => {
    vi.mocked(queryCommands.getQueryHistoryPage).mockRejectedValue(new Error('boom'));
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);
    expect(await screen.findByText('No history yet')).toBeInTheDocument();
  });
});
