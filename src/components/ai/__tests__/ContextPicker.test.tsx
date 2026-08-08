import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { ContextPicker } from '../ContextPicker';
import type { ContextEntry, ContextItem } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const mockListFiles = vi.fn<(query?: string) => Promise<ContextEntry[]>>();
const mockGetTables = vi.fn<(connectionId: string, database: string) => Promise<{ name: string; tableType: string; schema?: string }[]>>();

vi.mock('../../../commands/context', () => ({
  contextCommands: {
    listFiles: (...args: Parameters<typeof mockListFiles>) => mockListFiles(...args),
  },
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTables: (...args: Parameters<typeof mockGetTables>) => mockGetTables(...args),
  },
}));

const RECENT_KEY = 'datazen.contextRecent';

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mockListFiles.mockResolvedValue([]);
  mockGetTables.mockResolvedValue([]);
});

const sampleFiles: ContextEntry[] = [
  { name: 'schema.sql', path: 'schema.sql', isDir: false, size: 1024 },
  { name: 'users.sql', path: 'users.sql', isDir: false, size: 256 },
];

const sampleTables = [
  { name: 'users', tableType: 'table' as const },
  { name: 'orders', tableType: 'table' as const },
];

interface RenderOptions {
  connectionId?: string;
  database?: string;
  query?: string;
}

function renderPicker(
  query = '',
  onSelect = vi.fn<(item: ContextItem) => void>(),
  onClose = vi.fn(),
  options: RenderOptions = {},
) {
  const anchorRef = { current: document.createElement('div') };
  return {
    onSelect,
    onClose,
    ...render(
      <ContextPicker
        query={query}
        onSelect={onSelect}
        onClose={onClose}
        anchorRef={anchorRef}
        connectionId={options.connectionId}
        database={options.database}
      />,
    ),
  };
}

describe('ContextPicker', () => {
  it('shows Tables and Files categories at root', async () => {
    const { getByTestId } = renderPicker('', vi.fn(), vi.fn(), {
      connectionId: 'conn-1',
      database: 'mydb',
    });

    await waitFor(() => {
      expect(getByTestId('context-picker')).toBeInTheDocument();
      expect(getByTestId('context-cat-tables')).toBeInTheDocument();
      expect(getByTestId('context-cat-files')).toBeInTheDocument();
    });
  });

  it('shows only Files category when connectionId is missing', async () => {
    const { getByTestId, queryByTestId } = renderPicker();

    await waitFor(() => {
      expect(getByTestId('context-cat-files')).toBeInTheDocument();
      expect(queryByTestId('context-cat-tables')).not.toBeInTheDocument();
    });
  });

  it('drills into Tables and lists names', async () => {
    mockGetTables.mockResolvedValue(sampleTables);
    const { getByTestId } = renderPicker('', vi.fn(), vi.fn(), {
      connectionId: 'conn-1',
      database: 'mydb',
    });

    await waitFor(() => {
      expect(getByTestId('context-cat-tables')).toBeInTheDocument();
    });

    fireEvent.click(getByTestId('context-cat-tables'));

    await waitFor(() => {
      expect(getByTestId('context-picker-back')).toBeInTheDocument();
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'mydb');
    });

    const items = document.querySelectorAll('[data-testid="context-item"]');
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveAttribute('data-kind', 'table');
    expect(items[0]).toHaveAttribute('data-id', 'users');
    expect(items[1]).toHaveAttribute('data-id', 'orders');
  });

  it('filters across categories when query set at root', async () => {
    mockGetTables.mockResolvedValue(sampleTables);
    mockListFiles.mockResolvedValue(sampleFiles);

    const { queryAllByTestId } = renderPicker('users', vi.fn(), vi.fn(), {
      connectionId: 'conn-1',
      database: 'mydb',
    });

    await waitFor(() => {
      const items = queryAllByTestId('context-item');
      expect(items).toHaveLength(2);
      expect(items.some((el) => el.getAttribute('data-kind') === 'table')).toBe(true);
      expect(items.some((el) => el.getAttribute('data-kind') === 'file')).toBe(true);
    });

    expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'mydb');
    expect(mockListFiles).toHaveBeenCalledWith(undefined);
  });

  it('back returns to root', async () => {
    mockListFiles.mockResolvedValue(sampleFiles);
    const { getByTestId } = renderPicker();

    await waitFor(() => {
      expect(getByTestId('context-cat-files')).toBeInTheDocument();
    });

    fireEvent.click(getByTestId('context-cat-files'));

    await waitFor(() => {
      expect(getByTestId('context-picker-back')).toBeInTheDocument();
    });

    fireEvent.click(getByTestId('context-picker-back'));

    await waitFor(() => {
      expect(getByTestId('context-cat-files')).toBeInTheDocument();
      expect(document.querySelector('[data-testid="context-picker-back"]')).not.toBeInTheDocument();
    });
  });

  it('Escape pops nested view before closing', async () => {
    mockListFiles.mockResolvedValue(sampleFiles);
    const onClose = vi.fn();
    renderPicker('', vi.fn(), onClose);

    await waitFor(() => {
      expect(document.querySelector('[data-testid="context-cat-files"]')).toBeInTheDocument();
    });

    fireEvent.click(document.querySelector('[data-testid="context-cat-files"]')!);

    await waitFor(() => {
      expect(document.querySelector('[data-testid="context-picker-back"]')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(document.querySelector('[data-testid="context-cat-files"]')).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('calls onSelect and saves to recent on item click', async () => {
    mockListFiles.mockResolvedValue(sampleFiles);
    const onSelect = vi.fn();
    const { getByTestId } = renderPicker('', onSelect);

    await waitFor(() => {
      expect(getByTestId('context-cat-files')).toBeInTheDocument();
    });

    fireEvent.click(getByTestId('context-cat-files'));

    await waitFor(() => {
      expect(document.querySelectorAll('[data-testid="context-item"]').length).toBeGreaterThan(0);
    });

    fireEvent.click(document.querySelector('[data-testid="context-item"]')!);

    expect(onSelect).toHaveBeenCalledWith({
      kind: 'file',
      id: 'schema.sql',
      name: 'schema.sql',
      path: 'schema.sql',
    });

    const recent = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]') as ContextItem[];
    expect(recent).toHaveLength(1);
    expect(recent[0].id).toBe('schema.sql');
  });

  it('shows recent items at root', async () => {
    const recent: ContextItem[] = [
      { kind: 'table', id: 'users', name: 'users', database: 'mydb' },
    ];
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent));

    const { getByText } = renderPicker('', vi.fn(), vi.fn(), {
      connectionId: 'conn-1',
      database: 'mydb',
    });

    await waitFor(() => {
      expect(getByText('context.recent')).toBeInTheDocument();
      expect(getByText('users')).toBeInTheDocument();
    });
  });
});
