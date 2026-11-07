import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import type { MouseEvent as ReactMouseEvent } from 'react';
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
          onContextMenu,
        }: {
          value: string;
          onChange: (v: string) => void;
          placeholder?: string;
          onContextMenu?: (e: ReactMouseEvent<HTMLTextAreaElement>, sqlText: string) => void;
        },
        _ref: unknown,
      ) => (
        <textarea
          placeholder={placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onContextMenu={(event) => onContextMenu?.(event, value)}
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
  nativeEditMenuItems: () => [],
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
    expect(getDatabaseObjects).toHaveBeenCalledWith('c1', 'function', 'db_a');

    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith(
        'c1',
        'function',
        'fn_ok',
        'public',
        null,
        null,
        null,
        'db_a',
      );
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
      expect(getObjectDdl).toHaveBeenCalledWith(
        'c1',
        'function',
        'fn_ok',
        'public',
        null,
        null,
        null,
        'goecoride',
      );
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
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    const row = await screen.findByText('fn_ok');
    fireEvent.contextMenu(row);
    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
    const items = showNativeContextMenu.mock.calls[0]![0] as Array<{ id?: string }>;
    expect(items.map((i) => i.id)).toEqual(['refresh', 'open', 'copy-name', 'copy-ddl']);
  });

  it('switches kind and shows load errors', async () => {
    getDatabaseObjects.mockResolvedValueOnce([]).mockRejectedValueOnce(new Error('boom'));
    render(<ObjectBrowser dbSessionId="c1" database="db_a" />);
    await screen.findByText('objects.empty');

    fireEvent.click(screen.getByText('objects.procedure'));
    await screen.findByText('boom');
  });

  it('[tester] browses user-defined types and requests their qualified DDL', async () => {
    getDatabaseObjects
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ kind: 'type', schema: 'public', name: 'mood' }]);
    getObjectDdl.mockResolvedValueOnce("CREATE TYPE public.mood AS ENUM ('sad', 'ok', 'happy');");

    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    await screen.findByText('objects.empty');

    fireEvent.click(screen.getByTestId('object-browser-type'));
    const type = await screen.findByText('mood');
    expect(getDatabaseObjects).toHaveBeenLastCalledWith('c1', 'type', 'db_a');

    fireEvent.click(type);
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith(
        'c1',
        'type',
        'mood',
        'public',
        null,
        null,
        null,
        'db_a',
      );
    });
    expect(screen.getByDisplayValue(/CREATE TYPE public\.mood/)).toBeInTheDocument();
  });

  it('shows DDL fetch errors in the editor', async () => {
    getObjectDdl.mockRejectedValueOnce(new Error('no ddl'));
    render(<ObjectBrowser dbSessionId="c1" database="db_a" />);
    await screen.findByText('fn_ok');
    fireEvent.click(screen.getByText('fn_ok'));
    await waitFor(() => {
      expect(screen.getByDisplayValue(/no ddl/)).toBeInTheDocument();
    });
  });

  it('[tester] passes routine and trigger identity metadata to DDL IPC', async () => {
    getDatabaseObjects.mockResolvedValueOnce([
      { kind: 'function', schema: 'public', name: 'lookup', signature: 'integer' },
    ]);
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    const routine = await screen.findByText('lookup');
    fireEvent.click(routine);
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith(
        'c1',
        'function',
        'lookup',
        'public',
        'integer',
        undefined,
        undefined,
        'db_a',
      );
    });

    getDatabaseObjects.mockResolvedValueOnce([
      {
        kind: 'trigger',
        schema: null,
        name: 'audit_trigger',
        targetSchema: null,
        targetName: 'orders',
      },
    ]);
    fireEvent.click(screen.getByTestId('object-browser-trigger'));
    const trigger = await screen.findByText('audit_trigger');
    fireEvent.click(trigger);
    await waitFor(() => {
      expect(getObjectDdl).toHaveBeenCalledWith(
        'c1',
        'trigger',
        'audit_trigger',
        null,
        undefined,
        null,
        'orders',
        'db_a',
      );
    });
  });

  it('[tester] keeps overloaded routines independently selected', async () => {
    getDatabaseObjects.mockResolvedValueOnce([
      { kind: 'function', schema: 'public', name: 'lookup', signature: 'integer' },
      { kind: 'function', schema: 'public', name: 'lookup', signature: 'text' },
    ]);
    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    const items = await screen.findAllByTestId('object-browser-item');
    expect(items).toHaveLength(2);

    fireEvent.click(items[0]!);
    await waitFor(() => expect(items[0]!.className).toContain('bg-surface-raised'));
    fireEvent.click(items[1]!);
    await waitFor(() => expect(items[1]!.className).toContain('bg-surface-raised'));
    expect(items[0]!.className.split(/\s+/)).not.toContain('bg-surface-raised');
  });

  it('[tester] copies the independently addressed overload when another overload is selected', async () => {
    getDatabaseObjects.mockResolvedValueOnce([
      { kind: 'function', schema: 'public', name: 'lookup', signature: 'integer' },
      { kind: 'function', schema: 'public', name: 'lookup', signature: 'text' },
    ]);
    getObjectDdl
      .mockResolvedValueOnce('CREATE FUNCTION lookup(integer) RETURNS int AS $$ SELECT 1 $$;')
      .mockResolvedValueOnce('CREATE FUNCTION lookup(text) RETURNS int AS $$ SELECT 2 $$;');

    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    const items = await screen.findAllByTestId('object-browser-item');
    fireEvent.click(items[0]!);
    await waitFor(() => expect(screen.getByDisplayValue(/lookup\(integer\)/)).toBeInTheDocument());

    showNativeContextMenu.mockClear();
    getObjectDdl.mockClear();
    fireEvent.contextMenu(items[1]!);
    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalledTimes(1));

    const menuItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      id?: string;
      action?: () => void | Promise<void>;
    }>;
    const copyDdl = menuItems.find((item) => item.id === 'copy-ddl');
    expect(copyDdl?.action).toBeDefined();
    await copyDdl!.action!();

    expect(getObjectDdl).toHaveBeenCalledWith(
      'c1',
      'function',
      'lookup',
      'public',
      'text',
      undefined,
      undefined,
      'db_a',
    );
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('lookup(text)'));
  });

  it('[tester] exercises list and editor context actions', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    render(<ObjectBrowser dbSessionId="c1" databaseType="postgresql" database="db_a" />);
    const row = await screen.findByText('fn_ok');
    fireEvent.contextMenu(row);
    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalledTimes(1));

    const listItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      id?: string;
      action?: () => void | Promise<void>;
    }>;
    await listItems.find((item) => item.id === 'open')!.action!();
    await listItems.find((item) => item.id === 'copy-name')!.action!();
    await listItems.find((item) => item.id === 'copy-ddl')!.action!();
    await listItems.find((item) => item.id === 'refresh')!.action!();
    await waitFor(() => expect(getDatabaseObjects).toHaveBeenCalledTimes(2));
    expect(writeText).toHaveBeenCalledWith('fn_ok');

    showNativeContextMenu.mockClear();
    fireEvent.contextMenu(screen.getByDisplayValue(/CREATE FUNCTION/));
    await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalledTimes(1));
    const editorItems = showNativeContextMenu.mock.calls[0]![0] as Array<{
      id?: string;
      action?: () => void | Promise<void>;
    }>;
    await editorItems.find((item) => item.id === 'execute')!.action!();
    await waitFor(() => expect(executeQuery).toHaveBeenCalled());
    await editorItems.find((item) => item.id === 'format')!.action!();
    await editorItems.find((item) => item.id === 'comment')!.action!();
  });
});
