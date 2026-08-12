import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { ObjectBrowser } from '../ObjectBrowser';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../components/SqlEditor', () => ({
  SqlEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => (
    <textarea
      placeholder={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

const getDatabaseObjects = vi.fn();
const getObjectDdl = vi.fn();
const executeQuery = vi.fn();

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
  getDatabaseObjects.mockResolvedValue([
    { kind: 'function', schema: 'public', name: 'fn_ok' },
  ]);
  getObjectDdl.mockResolvedValue('CREATE FUNCTION fn_ok() RETURNS int AS $$ SELECT 1 $$;');
  executeQuery.mockResolvedValue({});
});

describe('ObjectBrowser', () => {
  it('lists objects, opens DDL, and executes it', async () => {
    render(<ObjectBrowser connectionId="c1" databaseType="postgresql" />);
    await screen.findByText('fn_ok');
    expect(getDatabaseObjects).toHaveBeenCalledWith('c1', 'function');

    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith('c1', 'function', 'fn_ok', 'public');
    });
    expect(screen.getByDisplayValue(/CREATE FUNCTION/)).toBeInTheDocument();

    fireEvent.click(screen.getByText('query.execute'));
    await waitFor(() => {
      expect(executeQuery).toHaveBeenCalled();
      expect(screen.getByText('objects.executeOk')).toBeInTheDocument();
    });

    executeQuery.mockRejectedValueOnce(new Error('exec failed'));
    fireEvent.click(screen.getByText('query.execute'));
    await screen.findByText('exec failed');
  });

  it('switches kind and shows load errors', async () => {
    getDatabaseObjects
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(new Error('boom'));
    render(<ObjectBrowser connectionId="c1" />);
    await screen.findByText('objects.empty');

    fireEvent.click(screen.getByText('objects.procedure'));
    await screen.findByText('boom');
  });

  it('shows DDL fetch errors in the editor', async () => {
    getObjectDdl.mockRejectedValueOnce(new Error('no ddl'));
    render(<ObjectBrowser connectionId="c1" />);
    await screen.findByText('fn_ok');
    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(screen.getByDisplayValue(/no ddl/)).toBeInTheDocument();
    });
  });
});
