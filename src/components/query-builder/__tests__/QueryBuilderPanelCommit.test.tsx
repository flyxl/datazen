/**
 * Query builder shell tests: bottom tabs, the OK commit strategy and cancel.
 *
 * These pin the three PRD behaviours that are easy to regress:
 *  - PRD §6.2 — Build / Preview are mutually exclusive (only one is mounted).
 *  - PRD §6.3 — OK never silently overwrites hand-written SQL in the editor.
 *  - PRD §6.3 — Cancel discards the canvas back to the entry snapshot.
 *
 * Note: `Dialog`'s `testId` prop routes through `tid()`, which is stripped from
 * non-E2E builds — so dialog presence is asserted through its real buttons,
 * which carry plain `data-testid`s.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryBuilderPanel } from '../QueryBuilderPanel';
import { generateSql } from '../hooks/useSqlGenerator';
import { formatSql } from '../../../lib/sqlFormat';
import { useQueryBuilderStore } from '../../../stores/queryBuilderStore';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: vi.fn().mockResolvedValue({ foreignKeys: [] }),
}));

/**
 * Give the panel a seeded canvas and make *that* the clean baseline.
 *
 * A brand-new panel starts blank (the builder belongs to its panel), so the
 * seed has to be committed away and restored — that restore is what captures
 * the entry snapshot the tests compare against.
 */
function openWith(table = 'users', columns = ['id', 'name']) {
  const store = useQueryBuilderStore.getState();
  store.openFor('panel-test');
  useQueryBuilderStore.getState().toggleTable(table);
  useQueryBuilderStore.getState().setAllColumns(table, columns, true);
  useQueryBuilderStore.getState().closeFor('ok');
  useQueryBuilderStore.getState().openFor('panel-test');
  return { table, columns };
}

/** Open with an empty canvas (valid for the "nothing to generate" cases). */
function openEmpty() {
  useQueryBuilderStore.getState().openFor('panel-test');
}

function renderPanel(overrides: Partial<Parameters<typeof QueryBuilderPanel>[0]> = {}) {
  const onCommit = vi.fn();
  const onCancel = vi.fn();
  const view = render(
    <QueryBuilderPanel
      panelId="panel-test"
      dbSessionId="session-1"
      databaseType="postgresql"
      currentSql=""
      onCommit={onCommit}
      onCancel={onCancel}
      {...overrides}
    />,
  );
  return { view, onCommit, onCancel };
}

/** SQL the real generator produces for the currently seeded store state. */
function generatedSql(): string {
  const s = useQueryBuilderStore.getState();
  return generateSql({
    selectedTables: s.selectedTables,
    selectedColumns: s.selectedColumns,
    joins: s.joins,
    tableAliases: s.tableAliases,
    where: s.where,
    orderBy: s.orderBy,
    groupBy: s.groupBy,
    distinct: s.distinct,
    limit: s.limit,
    offset: s.offset,
    databaseType: 'postgresql',
  });
}

beforeEach(() => {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('QueryBuilderPanel bottom tabs', () => {
  it('opens on the Build tab and mounts only the clause list', () => {
    openEmpty();
    renderPanel();
    expect(screen.getByTestId('qb-tab-build')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('qb-statement')).toBeInTheDocument();
    // Every clause of the statement is present, not just SELECT/WHERE.
    expect(screen.getByTestId('qb-clause-select')).toBeInTheDocument();
    expect(screen.getByTestId('qb-clause-from')).toBeInTheDocument();
    expect(screen.getByTestId('qb-clause-where')).toBeInTheDocument();
    expect(screen.getByTestId('qb-clause-group-by')).toBeInTheDocument();
    expect(screen.getByTestId('qb-clause-having')).toBeInTheDocument();
    expect(screen.getByTestId('qb-clause-order-by')).toBeInTheDocument();
    // Preview is not mounted at the same time — that is what buys the space back.
    expect(screen.queryByTestId('qb-sql-preview')).toBeNull();
  });

  it('swaps content when the Preview tab is selected', async () => {
    openWith();
    renderPanel();

    fireEvent.click(screen.getByTestId('qb-tab-preview'));

    await waitFor(() => {
      expect(screen.getByTestId('qb-tab-content')).toHaveAttribute('data-active-tab', 'preview');
    });
    expect(screen.getByTestId('qb-sql-preview')).toBeInTheDocument();
    expect(screen.queryByTestId('qb-statement')).toBeNull();
    expect(useQueryBuilderStore.getState().bottomTab).toBe('preview');
  });

  it('shows the empty state on Preview when nothing is selectable yet', () => {
    openEmpty();
    renderPanel();
    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    expect(screen.getByTestId('qb-sql-preview-empty')).toBeInTheDocument();
  });

  it('flags a generation problem on the Build tab', () => {
    // A table with no selected columns cannot produce SQL.
    useQueryBuilderStore.getState().openFor('panel-test');
    useQueryBuilderStore.getState().toggleTable('users');
    renderPanel();
    expect(screen.getByTestId('qb-tab-build-badge')).toBeInTheDocument();
  });

  it('keeps the footer visible on both tabs', () => {
    openEmpty();
    renderPanel();
    expect(screen.getByTestId('qb-ok')).toBeInTheDocument();
    expect(screen.getByTestId('qb-cancel')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    expect(screen.getByTestId('qb-footer')).toBeInTheDocument();
    expect(screen.getByTestId('qb-ok')).toBeInTheDocument();
  });
});

describe('QueryBuilderPanel preview tab', () => {
  it('shows the SQL pretty-printed, not as one long line', () => {
    openWith();
    renderPanel();
    fireEvent.click(screen.getByTestId('qb-tab-preview'));

    const preview = screen.getByTestId('qb-sql-preview');
    const shown = preview.textContent ?? '';
    expect(shown).toBe(formatSql(generatedSql(), 'postgresql'));
    // The generator's own output is single-line; the preview must not be.
    expect(generatedSql()).not.toContain('\n');
    expect(shown).toContain('\n');
  });

  it('highlights tokens instead of rendering plain text', () => {
    openWith();
    renderPanel();
    fireEvent.click(screen.getByTestId('qb-tab-preview'));

    const preview = screen.getByTestId('qb-sql-preview');
    expect(preview.querySelectorAll('span').length).toBeGreaterThan(5);
    // Exact text is preserved for the E2E probe, which reads textContent.
    expect(preview.textContent).toContain('SELECT');
  });

  it('commits exactly what the preview showed (WYSIWYG)', () => {
    openWith();
    const { onCommit } = renderPanel({ currentSql: '' });
    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    const shown = screen.getByTestId('qb-sql-preview').textContent ?? '';

    fireEvent.click(screen.getByTestId('qb-ok'));

    expect(onCommit).toHaveBeenCalledWith(shown, 'replace');
  });

  it('keeps the preview region able to fill the tab height', () => {
    openWith();
    renderPanel();
    fireEvent.click(screen.getByTestId('qb-tab-preview'));

    // flex-1 chain: tab-content → preview-region → preview root → <pre>
    expect(screen.getByTestId('qb-tab-content')).toHaveClass('flex-col');
    expect(screen.getByTestId('qb-preview-region')).toHaveClass('flex-1');
    expect(screen.getByTestId('qb-sql-preview')).toHaveClass('flex-1');
  });

  it('gives the build tab its own scroll container', () => {
    openWith();
    renderPanel();
    expect(screen.getByTestId('qb-build-scroll')).toHaveClass('overflow-auto');
    expect(screen.getByTestId('qb-build-scroll')).toHaveClass('flex-1');
  });
});

describe('QueryBuilderPanel OK commit', () => {
  it('writes straight through when the editor is empty', () => {
    openWith();
    const { onCommit } = renderPanel({ currentSql: '' });
    fireEvent.click(screen.getByTestId('qb-ok'));
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit.mock.calls[0]![1]).toBe('replace');
    expect(onCommit.mock.calls[0]![0]).toContain('SELECT');
  });

  it('does not prompt when the editor already holds the identical SQL', () => {
    openWith();
    // The builder commits the pretty-printed SQL, so the "identical" baseline is
    // the formatted text — otherwise every reopen would look like a conflict.
    const { onCommit } = renderPanel({ currentSql: formatSql(generatedSql(), 'postgresql') });

    fireEvent.click(screen.getByTestId('qb-ok'));

    // null = "nothing to change, just close and focus"
    expect(onCommit).toHaveBeenCalledWith(null, 'replace');
    expect(screen.queryByTestId('qb-conflict-replace')).toBeNull();
  });

  it('prompts replace / append / keep when the editor content differs', async () => {
    openWith();
    renderPanel({ currentSql: 'SELECT 1;' });

    fireEvent.click(screen.getByTestId('qb-ok'));

    await waitFor(() => {
      expect(screen.getByTestId('qb-conflict-replace')).toBeInTheDocument();
    });
    expect(screen.getByTestId('qb-conflict-append')).toBeInTheDocument();
    expect(screen.getByTestId('qb-conflict-keep')).toBeInTheDocument();
  });

  it('replaces when the user picks Replace', async () => {
    openWith();
    const { onCommit } = renderPanel({ currentSql: 'SELECT 1;' });
    fireEvent.click(screen.getByTestId('qb-ok'));
    await waitFor(() => expect(screen.getByTestId('qb-conflict-replace')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('qb-conflict-replace'));

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit.mock.calls[0]![1]).toBe('replace');
    await waitFor(() => expect(screen.queryByTestId('qb-conflict-replace')).toBeNull());
  });

  it('appends when the user picks Append', async () => {
    openWith();
    const { onCommit } = renderPanel({ currentSql: 'SELECT 1;' });
    fireEvent.click(screen.getByTestId('qb-ok'));
    await waitFor(() => expect(screen.getByTestId('qb-conflict-append')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('qb-conflict-append'));

    expect(onCommit.mock.calls[0]![1]).toBe('append');
  });

  it('stays open and writes nothing when the user keeps the editor content', async () => {
    openWith();
    const { onCommit, onCancel } = renderPanel({ currentSql: 'SELECT 1;' });
    fireEvent.click(screen.getByTestId('qb-ok'));
    await waitFor(() => expect(screen.getByTestId('qb-conflict-keep')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('qb-conflict-keep'));

    await waitFor(() => expect(screen.queryByTestId('qb-conflict-replace')).toBeNull());
    expect(onCommit).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('disables OK while no SQL can be generated', () => {
    openEmpty();
    renderPanel();
    expect(screen.getByTestId('qb-ok')).toBeDisabled();
  });
});

describe('QueryBuilderPanel cancel', () => {
  it('closes immediately when the canvas is untouched', () => {
    openWith();
    const { onCancel } = renderPanel();
    fireEvent.click(screen.getByTestId('qb-cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('asks for confirmation before discarding changes', async () => {
    openWith();
    // Mutate the canvas after opening so the entry snapshot differs.
    useQueryBuilderStore.getState().setDistinct(true);
    const { onCancel } = renderPanel();

    fireEvent.click(screen.getByTestId('qb-cancel'));

    // The confirmation gates the cancel: nothing happens until it is answered.
    await waitFor(() => expect(onCancel).not.toHaveBeenCalled());
    expect(useQueryBuilderStore.getState().openPanelId).toBe('panel-test');
  });

  it('the header × follows the same cancel path when untouched', async () => {
    openWith();
    const { onCancel } = renderPanel();
    fireEvent.click(screen.getByTestId('qb-close'));
    await waitFor(() => expect(onCancel).toHaveBeenCalledTimes(1));
  });
});

describe('QueryBuilderPanel canvas region', () => {
  it('collapses and expands the canvas from the header', () => {
    openEmpty();
    renderPanel();
    expect(screen.getByTestId('qb-diagram-canvas')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('qb-toggle-canvas'));
    expect(screen.queryByTestId('qb-diagram-canvas')).toBeNull();
    expect(useQueryBuilderStore.getState().canvasCollapsed).toBe(true);

    fireEvent.click(screen.getByTestId('qb-toggle-canvas'));
    expect(screen.getByTestId('qb-diagram-canvas')).toBeInTheDocument();
  });

  it('hints when two tables are selected without any relation', async () => {
    useQueryBuilderStore.getState().openFor('panel-test');
    useQueryBuilderStore.getState().toggleTable('users');
    useQueryBuilderStore.getState().toggleTable('orders');
    renderPanel();
    await waitFor(() => expect(screen.getByTestId('qb-no-relation-hint')).toBeInTheDocument());
  });

  it('renders the splitter so the canvas height is user-controlled', () => {
    openEmpty();
    renderPanel();
    expect(screen.getByTestId('qb-splitter')).toBeInTheDocument();
    expect(screen.getByTestId('qb-bottom-region')).toBeInTheDocument();
  });
});

describe('QueryBuilderPanel pagination inputs', () => {
  it('writes LIMIT / OFFSET into the store as the user types', () => {
    openWith();
    renderPanel();

    fireEvent.change(screen.getByTestId('qb-limit-input'), { target: { value: '50' } });
    fireEvent.change(screen.getByTestId('qb-offset-input'), { target: { value: '10' } });

    expect(useQueryBuilderStore.getState().limit).toBe(50);
    expect(useQueryBuilderStore.getState().offset).toBe(10);
  });

  it('clears the bound when the field is emptied', () => {
    openWith();
    useQueryBuilderStore.getState().setLimit(20);
    renderPanel();

    fireEvent.change(screen.getByTestId('qb-limit-input'), { target: { value: '' } });
    expect(useQueryBuilderStore.getState().limit).toBeNull();
  });

  it('reflects the store value back into the field', () => {
    openWith();
    useQueryBuilderStore.getState().setLimit(7);
    renderPanel();
    expect((screen.getByTestId('qb-limit-input') as HTMLInputElement).value).toBe('7');
  });
});

describe('QueryBuilderPanel diagnostics', () => {
  /** Add a WHERE condition with an empty value — not buildable. */
  function addEmptyCondition() {
    useQueryBuilderStore.getState().addCondition(useQueryBuilderStore.getState().where.id, {
      table: 'users',
      column: 'name',
      operator: '=',
      value: '',
      conjunction: 'AND',
    });
  }

  it('lists the blocking problem and disables OK', () => {
    openWith();
    addEmptyCondition();
    renderPanel();

    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    expect(screen.getByTestId('qb-diagnostics')).toBeInTheDocument();
    expect(screen.getByTestId('qb-ok')).toBeDisabled();
  });

  it('marks the Build tab while a problem exists', () => {
    openWith();
    addEmptyCondition();
    renderPanel();
    expect(screen.getByTestId('qb-tab-build-badge')).toBeInTheDocument();
  });

  it('re-enables OK once the value is filled in', () => {
    openWith();
    addEmptyCondition();
    renderPanel();

    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    expect(screen.getByTestId('qb-ok')).toBeDisabled();

    const conditionId = useQueryBuilderStore.getState().where.conditions[0]!.id;
    act(() => {
      useQueryBuilderStore.getState().updateCondition(conditionId, { value: 'Alice' });
    });
    expect(screen.queryByTestId('qb-diagnostics')).toBeNull();
    expect(screen.getByTestId('qb-ok')).toBeEnabled();
  });

  it('blocks an unsupported pagination request instead of dropping it', () => {
    openWith();
    useQueryBuilderStore.getState().setLimit(10);
    renderPanel({ databaseType: 'sqlserver' });

    fireEvent.click(screen.getByTestId('qb-tab-preview'));
    expect(screen.getByTestId('qb-diagnostics')).toBeInTheDocument();
    expect(screen.getByTestId('qb-ok')).toBeDisabled();
  });
});
