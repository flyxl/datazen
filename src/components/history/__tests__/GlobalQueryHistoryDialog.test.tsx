import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { GlobalQueryHistoryDialog } from '../GlobalQueryHistoryDialog';
import { queryCommands } from '../../../commands/query';
import { useConnectionStore } from '../../../stores/connectionStore';
import type { QueryHistoryEntry } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    getQueryHistory: vi.fn(),
    clearQueryHistory: vi.fn(),
  },
}));

describe('GlobalQueryHistoryDialog', () => {
  const mockEntries: QueryHistoryEntry[] = [
    {
      id: 'entry-1',
      connectionId: 'conn-1',
      database: 'db_a',
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
        },
        {
          id: 'conn-2',
          name: 'Secondary MySQL',
          databaseType: 'mysql',
          host: 'localhost',
          port: 3306,
        },
      ],
    });
    vi.mocked(queryCommands.getQueryHistory).mockResolvedValue(mockEntries);
  });

  it('renders history entries when open', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    expect(await screen.findByText('SELECT * FROM users')).toBeInTheDocument();
    expect(screen.getByText('INSERT INTO orders VALUES (1)')).toBeInTheDocument();
    expect(screen.getByText('SELECT * FROM bad_table')).toBeInTheDocument();
    expect(screen.getAllByText('Primary Postgres').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Secondary MySQL').length).toBeGreaterThanOrEqual(1);
  });

  it('filters by search input', async () => {
    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    await screen.findByText('SELECT * FROM users');
    const searchInput = screen.getByPlaceholderText('common.search');
    fireEvent.change(searchInput, { target: { value: 'orders' } });

    expect(screen.queryByText('SELECT * FROM users')).not.toBeInTheDocument();
    expect(screen.getByText('INSERT INTO orders VALUES (1)')).toBeInTheDocument();
  });

  it('invokes onSelectQuery and onClose when open in query is clicked', async () => {
    const onSelectQuery = vi.fn();
    const onClose = vi.fn();

    render(<GlobalQueryHistoryDialog open onClose={onClose} onSelectQuery={onSelectQuery} />);

    await screen.findByText('SELECT * FROM users');
    const openBtns = screen.getAllByTitle('在查询面板中打开');
    fireEvent.click(openBtns[0]);

    expect(onSelectQuery).toHaveBeenCalledWith(mockEntries[0]);
    expect(onClose).toHaveBeenCalled();
  });

  it('supports clearing history', async () => {
    vi.mocked(queryCommands.clearQueryHistory).mockResolvedValue(undefined);

    render(<GlobalQueryHistoryDialog open onClose={vi.fn()} />);

    await screen.findByText('SELECT * FROM users');
    const clearBtn = screen.getByTitle('清空历史记录');
    fireEvent.click(clearBtn);

    const confirmBtn = screen.getByText('确认');
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(queryCommands.clearQueryHistory).toHaveBeenCalled();
    });
  });
});
