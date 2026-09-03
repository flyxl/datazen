import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { ObjectBrowser } from '../ObjectBrowser';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef } = await import('react');
  return {
    SqlEditor: forwardRef(
      (
        {
          value,
          onChange,
          placeholder,
        }: { value: string; onChange: (v: string) => void; placeholder?: string },
        _ref: unknown,
      ) => (
        <textarea
          placeholder={placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      ),
    ),
  };
});

const showNativeContextMenu = vi.fn();
const getDatabaseObjects = vi.fn();
const getObjectDdl = vi.fn();
const executeQuery = vi.fn();

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabaseObjects: (...args: unknown[]) => getDatabaseObjects(...args),
    getObjectDdl: (...args: unknown[]) => getObjectDdl(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => executeQuery(...args),
  },
}));

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  getDatabaseObjects.mockResolvedValue([{ kind: 'function', schema: 'public', name: 'fn_ok' }]);
  getObjectDdl.mockResolvedValue('CREATE FUNCTION fn_ok() RETURNS int AS $$ SELECT 1 $$;');
  executeQuery.mockResolvedValue({});
});

describe('ObjectBrowser', () => {
  it('lists objects, opens DDL, and executes it', async () => {
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    await screen.findByText('fn_ok');
    expect(getDatabaseObjects).toHaveBeenCalledWith('c1', 'function');

    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith('c1', 'function', 'fn_ok', 'public');
    });
    expect(screen.getByDisplayValue(/CREATE FUNCTION/)).toBeInTheDocument();

    fireEvent.click(screen.getByText('query.execute'));
    await waitFor(() => {
      expect(executeQuery).toHaveBeenCalledWith(
        'c1',
        expect.stringContaining('CREATE FUNCTION'),
        undefined,
        'db_a',
        'public',
      );
      expect(screen.getByText('objects.executeOk')).toBeInTheDocument();
    });

    executeQuery.mockRejectedValueOnce(new Error('exec failed'));
    fireEvent.click(screen.getByText('query.execute'));
    await screen.findByText('exec failed');
  });

  it('executes DDL against the panel database pin regardless of global schema store', async () => {
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="goecoride" />);
    await screen.findByText('fn_ok');
    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith('c1', 'function', 'fn_ok', 'public');
    });

    fireEvent.click(screen.getByText('query.execute'));
    await waitFor(() => {
      expect(executeQuery).toHaveBeenCalledWith(
        'c1',
        expect.stringContaining('CREATE FUNCTION'),
        undefined,
        'goecoride',
        'public',
      );
    });
  });

  it('opens a web context menu on a routine item', async () => {
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" />);
    const row = await screen.findByText('fn_ok');
    fireEvent.contextMenu(row);
    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const items = showNativeContextMenu.mock.calls[0]![0] as Array<{ id?: string }>;
    expect(items.map((i) => i.id)).toEqual(['refresh', 'open', 'copy-name', 'copy-ddl']);
  });

  it('switches kind and shows load errors', async () => {
    getDatabaseObjects.mockResolvedValueOnce([]).mockRejectedValueOnce(new Error('boom'));
    render(<ObjectBrowser dbSessionId="c1" />);
    await screen.findByText('objects.empty');

    fireEvent.click(screen.getByText('objects.procedure'));
    await screen.findByText('boom');
  });

  it('shows DDL fetch errors in the editor', async () => {
    getObjectDdl.mockRejectedValueOnce(new Error('no ddl'));
    render(<ObjectBrowser dbSessionId="c1" />);
    await screen.findByText('fn_ok');
    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(screen.getByDisplayValue(/no ddl/)).toBeInTheDocument();
    });
  });
});
