/**
 * The find bar of the result grid (DB-14): what it does to the loaded rows
 * and to the writes it hands over.
 *
 * The rules live in `src/lib/resultFindReplace.ts` and have their own tests;
 * these lock the **wiring**: the bar filters the rows the grid shows, a
 * replacement speaks the statement's own row (never the filtered view's),
 * and Safe Mode keeps the find but takes the replace away.
 */
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { toId } from '@datazen/backend-client';
import type { StatementResult } from '../../../../types';
import { ResultTableView } from '../ResultTableView';
import { usePanelResultEdit } from '../../../../stores/panelResultEdit';

const dataTableMock = vi.hoisted(() => ({ render: vi.fn() }));

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

const settingsState = vi.hoisted(() => ({ queryResultLimit: 5000, safeMode: false }));

vi.mock('../../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: typeof settingsState }) => unknown) =>
    selector({ settings: settingsState }),
}));

vi.mock('../../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    rows: unknown[][];
    statusBar?: React.ReactNode;
    onCellEdit?: (rowIndex: number, column: string, value: unknown) => void;
  }) => {
    dataTableMock.render(props);
    return (
      <div role="grid">
        {props.statusBar}
        <span data-testid="mock-row-count">{props.rows.length}</span>
        <button
          type="button"
          onClick={() => props.onCellEdit?.(0, 'name', 'written')}
          data-testid="mock-commit"
        >
          commit
        </button>
      </div>
    );
  },
}));

function writableResult(): StatementResult {
  const executionId = toId('execution-1')!;
  return {
    sql: 'select id, name from users',
    columns: [
      { name: 'id', dataType: 'int4', nullable: false },
      { name: 'name', dataType: 'text', nullable: true },
    ],
    rows: [
      [1, 'acme corp'],
      [2, 'beta llc'],
      [3, 'acme and acme again'],
    ],
    executionTimeMs: 2,
    runtimeBinding: {
      executionId,
      resourceBindingId: toId('resource-1')!,
      handle: { dbSessionId: toId('session-1')!, runtimeEpoch: toId('epoch-1')! },
    },
    source: {
      executionId,
      statementIndex: 0,
      writableMapping: 'verified',
      relation: {
        connectionId: toId('connection-1')!,
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        object: { kind: 'table', name: 'users', signature: null },
      },
      context: {
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        searchPath: null,
        effectiveIdentity: null,
        transactionState: 'none',
        autocommit: true,
        confidence: 'confirmed',
      },
    },
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  settingsState.safeMode = false;
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
});

function openBar() {
  fireEvent.click(screen.getByTestId('result-find-toggle'));
}

function typeNeedle(text: string) {
  fireEvent.change(screen.getByTestId('result-find-needle'), { target: { value: text } });
}

describe('the result find bar wiring (DB-14)', () => {
  it('is closed until the status bar opens it, and the grid shows every row meanwhile', () => {
    render(<ResultTableView result={writableResult()} />);

    expect(screen.queryByTestId('result-find-bar')).toBeNull();
    expect(screen.getByTestId('mock-row-count').textContent).toBe('3');
  });

  it('filters the loaded rows as the needle is typed', () => {
    render(<ResultTableView result={writableResult()} />);
    openBar();
    typeNeedle('acme');

    expect(screen.getByTestId('mock-row-count').textContent).toBe('2');
    expect(screen.getByTestId('result-find-count').textContent).toContain('"matched":2');
  });

  it('keeps every row for an empty needle — opening the bar must not blank the grid', () => {
    render(<ResultTableView result={writableResult()} />);
    openBar();

    expect(screen.getByTestId('mock-row-count').textContent).toBe('3');
  });

  it('closes and drops the filter together', () => {
    render(<ResultTableView result={writableResult()} />);
    openBar();
    typeNeedle('acme');

    fireEvent.click(screen.getByTestId('result-find-close'));

    expect(screen.queryByTestId('result-find-bar')).toBeNull();
    expect(screen.getByTestId('mock-row-count').textContent).toBe('3');
  });

  it('shows the no-match note when the filter hides every row', () => {
    render(<ResultTableView result={writableResult()} />);
    openBar();
    typeNeedle('nothing-matches-this');

    expect(screen.getByTestId('result-find-no-matches')).toBeTruthy();
  });

  it('replaces the next matching cell through the write path, in the statement coordinates', async () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    render(<ResultTableView result={writableResult()} onCellEdit={onCellEdit} />);
    openBar();
    typeNeedle('acme');
    fireEvent.change(screen.getByTestId('result-find-replacement'), {
      target: { value: 'acorp' },
    });

    fireEvent.click(screen.getByTestId('result-find-replace-next'));

    await waitFor(() => expect(onCellEdit).toHaveBeenCalled());
    expect(onCellEdit).toHaveBeenCalledWith(0, 'name', 'acorp corp');
  });

  it('replaces every matching cell, one write each, in row order', async () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    render(<ResultTableView result={writableResult()} onCellEdit={onCellEdit} />);
    openBar();
    typeNeedle('acme');
    fireEvent.change(screen.getByTestId('result-find-replacement'), {
      target: { value: 'acorp' },
    });

    fireEvent.click(screen.getByTestId('result-find-replace-all'));

    // Two cells hold the needle: row 0 and row 2, both in `name`.
    await waitFor(() => expect(onCellEdit).toHaveBeenCalledTimes(2));
    expect(onCellEdit).toHaveBeenNthCalledWith(1, 0, 'name', 'acorp corp');
    expect(onCellEdit).toHaveBeenNthCalledWith(2, 2, 'name', 'acorp and acorp again');
    // Row 1 holds no match and stays untouched.
    expect(onCellEdit).not.toHaveBeenCalledWith(1, expect.anything(), expect.anything());
  });

  it('translates the grid write back to the statement row under a filter', () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    render(<ResultTableView result={writableResult()} onCellEdit={onCellEdit} />);
    openBar();
    // Only row 3 (`acme and acme again`) matches 'again', so the grid's
    // first visible row IS the statement's third row.
    typeNeedle('again');

    fireEvent.click(screen.getByTestId('mock-commit'));

    expect(onCellEdit).toHaveBeenCalledWith(2, 'name', 'written');
  });

  it('hides the replace row in Safe Mode and says why', () => {
    settingsState.safeMode = true;
    render(<ResultTableView result={writableResult()} onCellEdit={vi.fn()} />);
    openBar();

    expect(screen.queryByTestId('result-find-replacement')).toBeNull();
    expect(screen.getByTestId('result-find-replace-blocked').textContent).toContain(
      'resultFind.safeMode',
    );
  });

  it('hides the replace row when the result cannot write back', () => {
    const result = writableResult();
    delete result.runtimeBinding;
    render(<ResultTableView result={result} />);
    openBar();

    expect(screen.queryByTestId('result-find-replacement')).toBeNull();
    expect(screen.getByTestId('result-find-replace-blocked').textContent).toContain(
      'resultFind.notWritable',
    );
  });
});
