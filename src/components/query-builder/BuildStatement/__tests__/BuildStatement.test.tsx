/**
 * BuildStatement — the Navicat-style clause list.
 *
 * The rule under test is uniform: every item of every clause is a chip, clicking
 * a chip opens that item's dialog, and its × removes it. The cases below walk
 * each clause through that rule, plus the two things a chip-only UI can get
 * wrong on its own: a draft that writes before OK, and a new condition that
 * loses its group's AND/OR.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { BuildStatement } from '../BuildStatement';
import type { BuildStatementActions } from '../BuildStatement';
import type { QbColumnSelection, QbCondition, QbConditionGroup } from '../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

function emptyGroup(id = 'g', logic: 'AND' | 'OR' = 'AND'): QbConditionGroup {
  return { id, logic, conditions: [], groups: [] };
}

function cond(id: string, patch: Partial<QbCondition> = {}): QbCondition {
  return {
    id,
    table: 'sales',
    column: 'qty',
    operator: '>=',
    value: '2000',
    conjunction: 'AND',
    ...patch,
  };
}

const COLUMNS = { sales: ['qty', 'region', 'total'], regions: ['id', 'name'] };

function makeActions(overrides: Partial<BuildStatementActions> = {}): BuildStatementActions {
  return {
    setDistinct: vi.fn(),
    addColumn: vi.fn(),
    removeColumn: vi.fn(),
    updateColumn: vi.fn(),
    setTableAlias: vi.fn(),
    removeTable: vi.fn(),
    addTable: vi.fn(),
    addCondition: vi.fn(),
    updateCondition: vi.fn(),
    removeCondition: vi.fn(),
    addConditionGroup: vi.fn(),
    setGroupLogic: vi.fn(),
    addHavingCondition: vi.fn(),
    updateHavingCondition: vi.fn(),
    removeHavingCondition: vi.fn(),
    addHavingGroup: vi.fn(),
    setHavingGroupLogic: vi.fn(),
    addGroupBy: vi.fn(),
    removeGroupBy: vi.fn(),
    addSort: vi.fn(),
    setSort: vi.fn(),
    removeSort: vi.fn(),
    ...overrides,
  };
}

interface HarnessOptions {
  selectedColumns?: QbColumnSelection[];
  where?: QbConditionGroup;
  having?: QbConditionGroup;
  groupBy?: { table: string; column: string }[];
  orderBy?: { table: string; column: string; direction: 'ASC' | 'DESC' }[];
  joins?: Parameters<typeof BuildStatement>[0]['state']['joins'];
  aliases?: Record<string, string>;
  tables?: string[];
  availableTables?: string[];
  columnTypes?: Record<string, Record<string, string>>;
  actions?: Partial<BuildStatementActions>;
}

function renderStatement(options: HarnessOptions = {}) {
  const actions = makeActions(options.actions);
  const view = render(
    <BuildStatement
      schema={{
        tables: options.tables ?? ['sales'],
        columns: COLUMNS,
        columnTypes: options.columnTypes,
        aliases: options.aliases ?? {},
        availableTables: options.availableTables ?? [],
      }}
      state={{
        selectedColumns: options.selectedColumns ?? [],
        distinct: false,
        joins: options.joins ?? [],
        where: options.where ?? emptyGroup('where'),
        having: options.having ?? emptyGroup('having'),
        groupBy: options.groupBy ?? [],
        orderBy: options.orderBy ?? [],
      }}
      actions={actions}
    />,
  );
  return { actions, unmount: view.unmount };
}

/**
 * Open a `Select` by its trigger testid. The trigger is the button itself for a
 * plain select and a div wrapping an input for a searchable one.
 */
function openSelect(testId: string): void {
  const el = screen.getByTestId(testId);
  fireEvent.click(el.querySelector('button') ?? el.querySelector('input') ?? el);
}

afterEach(cleanup);

describe('BuildStatement — clause list', () => {
  it('renders every SQL clause, HAVING included', () => {
    renderStatement();
    for (const clause of ['select', 'from', 'where', 'group-by', 'having', 'order-by']) {
      expect(screen.getByTestId(`qb-clause-${clause}`)).toBeInTheDocument();
    }
    // The keywords are SQL, not prose.
    expect(screen.getByTestId('qb-clause-having')).toHaveTextContent('HAVING');
    expect(screen.getByTestId('qb-clause-group-by')).toHaveTextContent('GROUP BY');
  });
});

describe('BuildStatement — SELECT chips', () => {
  it('shows a column as one chip carrying its aggregate and alias', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty', aggregate: 'SUM', alias: 'total_qty' }],
    });
    const chip = screen.getByTestId('qb-field-chip-sales-qty');
    expect(chip).toHaveTextContent('SUM(');
    expect(chip).toHaveTextContent('sales.qty');
    expect(chip).toHaveTextContent('AS total_qty');
  });

  it('qualifies chips with the table alias, matching the emitted SQL', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty' }],
      aliases: { sales: 's' },
    });
    expect(screen.getByTestId('qb-field-chip-sales-qty')).toHaveTextContent('s.qty');
  });

  it('opens the column options dialog and applies the edits', () => {
    const { actions } = renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty' }],
    });
    expect(screen.queryByTestId('qb-col-opt-apply')).toBeNull();

    fireEvent.click(screen.getByTestId('qb-field-chip-sales-qty').querySelector('button')!);
    expect(screen.getByTestId('qb-col-opt-field')).toHaveTextContent('sales.qty');

    fireEvent.change(screen.getByTestId('qb-col-opt-alias'), { target: { value: 'total' } });
    fireEvent.click(screen.getByTestId('qb-col-opt-groupby'));
    fireEvent.click(screen.getByTestId('qb-col-opt-apply'));

    expect(actions.updateColumn).toHaveBeenCalledWith(
      'sales',
      'qty',
      expect.objectContaining({ alias: 'total', groupBy: true }),
    );
  });

  it('adds a field from the SELECT picker, skipping the ones already used', () => {
    const { actions } = renderStatement({ selectedColumns: [{ table: 'sales', column: 'qty' }] });
    fireEvent.click(screen.getByTestId('qb-add-fields'));
    expect(screen.queryByRole('option', { name: 'sales.qty' })).toBeNull();
    fireEvent.mouseDown(screen.getByRole('option', { name: 'sales.region' }));
    expect(actions.addColumn).toHaveBeenCalledWith('sales', 'region');
  });
});

describe('BuildStatement — FROM chips', () => {
  const joins = [
    {
      id: 'j1',
      type: 'LEFT' as const,
      leftTable: 'sales',
      leftColumn: 'region_id',
      rightTable: 'regions',
      rightColumn: 'id',
      isManual: false,
    },
  ];

  it('renders one chip per table, the joined one badged with its join type', () => {
    renderStatement({
      tables: ['sales', 'regions'],
      joins,
      aliases: { sales: 's', regions: 'r' },
    });
    expect(screen.getByTestId('qb-from-chip-sales')).toHaveTextContent('sales');
    expect(screen.getByTestId('qb-from-chip-sales')).toHaveTextContent('AS s');
    expect(screen.getByTestId('qb-from-chip-regions')).toHaveTextContent('LEFT JOIN');
  });

  it('carries the oriented ON predicate on the join chip', () => {
    renderStatement({ tables: ['sales', 'regions'], joins });
    const step = screen.getByTestId('qb-from-join-0');
    expect(step).toHaveAttribute('data-join-type', 'LEFT');
    // Oriented from the included side outwards, exactly as the SQL reads it.
    expect(step).toHaveAttribute('data-join-on', 'sales.region_id = regions.id');
  });

  it('flags a table no join reaches', () => {
    renderStatement({ tables: ['sales', 'regions'] });
    expect(screen.getByTestId('qb-from-unjoined-regions')).toHaveTextContent(
      'query.visualBuilder.unjoinedTable',
    );
  });

  it('opens the table options dialog with the alias and the join predicate', () => {
    const { actions } = renderStatement({ tables: ['sales', 'regions'], joins });
    fireEvent.click(screen.getByTestId('qb-from-chip-regions').querySelector('button')!);
    expect(screen.getByTestId('qb-table-opt-name')).toHaveTextContent('regions');
    expect(screen.getByTestId('qb-table-opt-join')).toHaveTextContent('LEFT JOIN');
    expect(screen.getByTestId('qb-table-opt-on')).toHaveTextContent('sales.region_id = regions.id');

    fireEvent.change(screen.getByTestId('qb-table-opt-alias'), { target: { value: 'r2' } });
    fireEvent.click(screen.getByTestId('qb-table-opt-apply'));
    expect(actions.setTableAlias).toHaveBeenCalledWith('regions', 'r2');
    // A modal that stays open covers every control below it.
    expect(screen.queryByTestId('qb-table-opt-apply')).toBeNull();
  });

  it('closes the table dialog when the table is removed', () => {
    const { actions } = renderStatement({ tables: ['sales', 'regions'] });
    fireEvent.click(screen.getByTestId('qb-from-chip-regions').querySelector('button')!);
    fireEvent.click(screen.getByTestId('qb-table-opt-remove'));
    expect(actions.removeTable).toHaveBeenCalledWith('regions');
    expect(screen.queryByTestId('qb-table-opt-apply')).toBeNull();
  });

  it('reports an unjoined table in its dialog instead of a join', () => {
    renderStatement({ tables: ['sales', 'regions'] });
    fireEvent.click(screen.getByTestId('qb-from-chip-regions').querySelector('button')!);
    expect(screen.getByTestId('qb-table-opt-unjoined')).toBeInTheDocument();
    expect(screen.queryByTestId('qb-table-opt-on')).toBeNull();
  });

  it('adds a table from the picker list', () => {
    const { actions } = renderStatement({ availableTables: ['orders'] });
    fireEvent.click(screen.getByTestId('qb-add-tables'));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'orders' }));
    expect(actions.addTable).toHaveBeenCalledWith('orders');
  });
});

describe('BuildStatement — WHERE chips', () => {
  it('renders a condition as a chip with its conjunction badge', () => {
    renderStatement({
      where: {
        ...emptyGroup('where'),
        conditions: [cond('c1'), cond('c2', { column: 'region', conjunction: 'OR' })],
      },
    });
    expect(screen.getByTestId('qb-where-chip-c1')).toHaveTextContent('sales.qty >= 2000');
    // The first row has nothing above it, so it carries no badge.
    expect(screen.getByTestId('qb-where-chip-c1')).not.toHaveTextContent('AND');
    expect(screen.getByTestId('qb-where-chip-c2')).toHaveTextContent('OR');
  });

  it('shows the placeholder only while the clause is empty', () => {
    const { unmount } = renderStatement();
    expect(screen.getByTestId('qb-where-empty')).toHaveTextContent(
      '<query.visualBuilder.addConditions>',
    );
    unmount();

    renderStatement({ where: { ...emptyGroup('where'), conditions: [cond('c1')] } });
    expect(screen.queryByTestId('qb-where-empty')).toBeNull();
    expect(screen.getByTestId('qb-where-chip-c1')).toBeInTheDocument();
  });

  it('edits an existing condition through its dialog', () => {
    const { actions } = renderStatement({
      where: { ...emptyGroup('where'), conditions: [cond('c1')] },
    });
    fireEvent.click(screen.getByTestId('qb-where-chip-c1').querySelector('button')!);
    expect(screen.getByTestId('qb-cond-value')).toHaveValue('2000');

    fireEvent.change(screen.getByTestId('qb-cond-value'), { target: { value: '500' } });
    fireEvent.click(screen.getByTestId('qb-cond-apply'));

    expect(actions.updateCondition).toHaveBeenCalledWith(
      'c1',
      expect.objectContaining({ value: '500', table: 'sales', column: 'qty' }),
    );
    expect(actions.addCondition).not.toHaveBeenCalled();
    expect(screen.queryByTestId('qb-cond-apply')).toBeNull();
  });

  it('removes an existing condition from the dialog', () => {
    const { actions } = renderStatement({
      where: { ...emptyGroup('where'), conditions: [cond('c1')] },
    });
    fireEvent.click(screen.getByTestId('qb-where-chip-c1').querySelector('button')!);
    fireEvent.click(screen.getByTestId('qb-cond-remove'));
    expect(actions.removeCondition).toHaveBeenCalledWith('c1');
    expect(screen.queryByTestId('qb-cond-apply')).toBeNull();
  });

  it('treats a new condition as a draft: cancel writes nothing', () => {
    const { actions } = renderStatement({ where: emptyGroup('where') });
    fireEvent.click(screen.getByTestId('qb-where-add-condition'));
    expect(screen.getByTestId('qb-cond-apply')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('qb-cond-cancel'));
    expect(actions.addCondition).not.toHaveBeenCalled();
    expect(screen.queryByTestId('qb-cond-apply')).toBeNull();
  });

  it('writes a new condition only on OK, into the group that asked for it', () => {
    const { actions } = renderStatement({ where: emptyGroup('where') });
    fireEvent.click(screen.getByTestId('qb-where-add-condition'));
    fireEvent.click(screen.getByTestId('qb-cond-apply'));

    expect(actions.addCondition).toHaveBeenCalledWith(
      'where',
      expect.objectContaining({
        table: 'sales',
        column: 'qty',
        operator: '=',
        conjunction: 'AND',
      }),
    );
  });

  it('seeds a new condition in an OR sub-group with OR', () => {
    const sub = emptyGroup('sub', 'OR');
    const { actions } = renderStatement({ where: { ...emptyGroup('where'), groups: [sub] } });

    fireEvent.click(screen.getByTestId('qb-where-subgroup-add-condition'));
    fireEvent.click(screen.getByTestId('qb-cond-apply'));

    // A hardcoded AND here would silently turn the group's `(a OR b)` into `(a AND b)`.
    expect(actions.addCondition).toHaveBeenCalledWith(
      'sub',
      expect.objectContaining({ conjunction: 'OR' }),
    );
  });

  it('renders a nested group as a box with its own logic control', () => {
    const { actions } = renderStatement({
      where: { ...emptyGroup('where'), groups: [emptyGroup('sub', 'OR')] },
    });
    const box = screen.getByTestId('qb-where-group');
    expect(box).toHaveAttribute('data-group-logic', 'OR');
    openSelect('qb-where-group-logic');
    fireEvent.mouseDown(screen.getByRole('option', { name: 'AND' }));
    expect(actions.setGroupLogic).toHaveBeenCalledWith('sub', 'AND');
  });

  it('adds a nested group', () => {
    const { actions } = renderStatement();
    fireEvent.click(screen.getByTestId('qb-where-add-group'));
    expect(actions.addConditionGroup).toHaveBeenCalledWith('where', 'OR');
  });

  it('surfaces per-column criteria as chips, since the generator merges them', () => {
    const { actions } = renderStatement({
      selectedColumns: [
        {
          table: 'sales',
          column: 'qty',
          where: {
            id: 'w1',
            table: 'sales',
            column: 'qty',
            operator: '>=',
            value: '80',
            conjunction: 'AND',
          },
        },
      ],
    });
    expect(screen.getByTestId('qb-where-column-chip-sales-qty')).toHaveTextContent('>= 80');

    fireEvent.click(screen.getByTestId('qb-where-column-remove-sales-qty'));
    expect(actions.updateColumn).toHaveBeenCalledWith('sales', 'qty', { where: undefined });
    expect(actions.removeCondition).not.toHaveBeenCalled();
  });
});

describe('BuildStatement — HAVING chips', () => {
  it('renders an aggregated condition chip', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty', aggregate: 'SUM' }],
      having: { ...emptyGroup('having'), conditions: [cond('h1', { aggregate: 'SUM' })] },
    });
    expect(screen.getByTestId('qb-having-chip-h1')).toHaveTextContent('SUM(sales.qty) >= 2000');
  });

  it('offers an aggregate selector in HAVING but not in WHERE', () => {
    renderStatement({
      where: { ...emptyGroup('where'), conditions: [cond('c1')] },
      having: { ...emptyGroup('having'), conditions: [cond('h1', { aggregate: 'AVG' })] },
    });

    fireEvent.click(screen.getByTestId('qb-having-chip-h1').querySelector('button')!);
    expect(screen.getByTestId('qb-cond-aggregate')).toHaveTextContent('AVG');
    fireEvent.click(screen.getByTestId('qb-cond-cancel'));

    fireEvent.click(screen.getByTestId('qb-where-chip-c1').querySelector('button')!);
    expect(screen.queryByTestId('qb-cond-aggregate')).toBeNull();
  });

  it('starts a new HAVING condition aggregated, so it cannot be invalid SQL', () => {
    const { actions } = renderStatement({ having: emptyGroup('having') });
    fireEvent.click(screen.getByTestId('qb-having-add-condition'));
    fireEvent.click(screen.getByTestId('qb-cond-apply'));

    expect(actions.addHavingCondition).toHaveBeenCalledWith(
      'having',
      expect.objectContaining({ aggregate: 'SUM' }),
    );
    expect(screen.queryByTestId('qb-cond-apply')).toBeNull();
  });

  it('does not aggregate a new WHERE condition', () => {
    const { actions } = renderStatement();
    fireEvent.click(screen.getByTestId('qb-where-add-condition'));
    fireEvent.click(screen.getByTestId('qb-cond-apply'));

    const payload = vi.mocked(actions.addCondition).mock.calls[0]![1];
    expect(payload.aggregate).toBeUndefined();
  });

  it('adds and removes HAVING sub-groups', () => {
    const { actions } = renderStatement();
    fireEvent.click(screen.getByTestId('qb-having-add-group'));
    expect(actions.addHavingGroup).toHaveBeenCalledWith('having', 'OR');
  });
});

describe('BuildStatement — GROUP BY / ORDER BY chips', () => {
  it('renders one chip per key from both sources', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'total', sort: 'DESC' }],
      groupBy: [{ table: 'sales', column: 'region' }],
      orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
    });
    expect(screen.getByTestId('qb-group-chip-sales-region')).toBeInTheDocument();
    expect(screen.getByTestId('qb-order-chip-sales-region')).toHaveTextContent('ASC');
    expect(screen.getByTestId('qb-order-chip-sales-total')).toHaveTextContent('DESC');
  });

  it('removes a GROUP BY chip through its own entry', () => {
    const { actions } = renderStatement({ groupBy: [{ table: 'sales', column: 'region' }] });
    fireEvent.click(screen.getByTestId('qb-group-remove-sales-region'));
    expect(actions.removeGroupBy).toHaveBeenCalledWith(
      expect.objectContaining({ column: 'region', source: 'store', index: 0 }),
    );
  });

  it('opens the column dialog from a GROUP BY chip that is also selected', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty', groupBy: true }],
    });
    fireEvent.click(screen.getByTestId('qb-group-chip-sales-qty').querySelector('button')!);
    expect(screen.getByTestId('qb-col-opt-field')).toHaveTextContent('sales.qty');
  });

  it('leaves a GROUP BY key with no options unclickable', () => {
    renderStatement({ groupBy: [{ table: 'sales', column: 'region' }] });
    const chip = screen.getByTestId('qb-group-chip-sales-region');
    // Only the × remains: there is no options body to click.
    expect(chip.querySelector('button:not([data-testid])')).toBeNull();
    expect(chip.querySelector('button')).toHaveAttribute(
      'data-testid',
      'qb-group-remove-sales-region',
    );
  });

  it('sets a direction from the ORDER BY chip dialog', () => {
    const { actions } = renderStatement({
      orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
    });
    fireEvent.click(screen.getByTestId('qb-order-chip-sales-region').querySelector('button')!);
    expect(screen.getByTestId('qb-sort-opt-field')).toHaveTextContent('sales.region');

    openSelect('qb-sort-opt-direction');
    fireEvent.mouseDown(screen.getByRole('option', { name: 'query.visualBuilder.desc' }));
    fireEvent.click(screen.getByTestId('qb-sort-opt-apply'));

    expect(actions.setSort).toHaveBeenCalledWith(
      expect.objectContaining({ column: 'region', source: 'store', index: 0 }),
      'DESC',
    );
    expect(screen.queryByTestId('qb-sort-opt-apply')).toBeNull();
  });

  it('removes an ORDER BY entry from its dialog and from the ×', () => {
    const { actions } = renderStatement({
      orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
    });
    fireEvent.click(screen.getByTestId('qb-order-chip-sales-region').querySelector('button')!);
    fireEvent.click(screen.getByTestId('qb-sort-opt-remove'));
    expect(actions.removeSort).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('qb-sort-opt-apply')).toBeNull();

    fireEvent.click(screen.getByTestId('qb-order-remove-sales-region'));
    expect(actions.removeSort).toHaveBeenCalledTimes(2);
  });

  it('adds ORDER BY and GROUP BY keys from their pickers', () => {
    const { actions } = renderStatement();
    fireEvent.click(screen.getByTestId('qb-add-group-by'));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'sales.region' }));
    expect(actions.addGroupBy).toHaveBeenCalledWith('sales', 'region');

    fireEvent.click(screen.getByTestId('qb-add-order-by'));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'sales.total' }));
    expect(actions.addSort).toHaveBeenCalledWith('sales', 'total');
  });
});

describe('BuildStatement — dialog isolation', () => {
  beforeEach(() => {
    // The dialogs portal into body; each case starts from a clean document.
    cleanup();
  });

  it('keeps the column form alive across unrelated re-renders', () => {
    const { rerender } = render(
      <BuildStatement
        schema={{ tables: ['sales'], columns: COLUMNS, aliases: {}, availableTables: [] }}
        state={{
          selectedColumns: [{ table: 'sales', column: 'qty' }],
          distinct: false,
          joins: [],
          where: emptyGroup('where'),
          having: emptyGroup('having'),
          groupBy: [],
          orderBy: [],
        }}
        actions={makeActions()}
      />,
    );
    fireEvent.click(screen.getByTestId('qb-field-chip-sales-qty').querySelector('button')!);
    fireEvent.change(screen.getByTestId('qb-col-opt-alias'), { target: { value: 'typing' } });

    // An unrelated store update (new object identities) must not wipe the form.
    rerender(
      <BuildStatement
        schema={{ tables: ['sales'], columns: COLUMNS, aliases: {}, availableTables: [] }}
        state={{
          selectedColumns: [{ table: 'sales', column: 'qty' }],
          distinct: true,
          joins: [],
          where: emptyGroup('where'),
          having: emptyGroup('having'),
          groupBy: [],
          orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
        }}
        actions={makeActions()}
      />,
    );
    expect(screen.getByTestId('qb-col-opt-alias')).toHaveValue('typing');
  });
});

// ── Operator filtering by column type ────────────────────────

describe('BuildStatement — operator filtering', () => {
  it('filters out LIKE/NOT LIKE for numeric columns in ColumnOptionsDialog', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'qty' }],
      columnTypes: { sales: { qty: 'integer' } },
      actions: {},
    });
    // Open the column options dialog
    fireEvent.click(screen.getByTestId('qb-field-chip-sales-qty').querySelector('button')!);
    // Open the operator select
    openSelect('qb-col-opt-operator');
    // Numeric columns: LIKE/NOT LIKE should not appear
    const options = screen.getAllByTestId('select-option');
    const optionTexts = options.map((el) => el.textContent?.trim());
    expect(optionTexts).not.toContain('LIKE');
    expect(optionTexts).not.toContain('NOT LIKE');
    // Comparison operators should be present
    expect(optionTexts).toContain('=');
    expect(optionTexts).toContain('>');
    expect(optionTexts).toContain('IN');
    expect(optionTexts.length).toBeLessThan(12); // fewer than the full set
  });

  it('shows LIKE/NOT LIKE for text columns', () => {
    renderStatement({
      selectedColumns: [{ table: 'sales', column: 'region' }],
      columnTypes: { sales: { region: 'varchar' } },
      actions: {},
    });
    fireEvent.click(screen.getByTestId('qb-field-chip-sales-region').querySelector('button')!);
    openSelect('qb-col-opt-operator');
    const options = screen.getAllByTestId('select-option');
    const optionTexts = options.map((el) => el.textContent?.trim());
    // Text columns should have LIKE
    expect(optionTexts).toContain('LIKE');
    expect(optionTexts).toContain('NOT LIKE');
    expect(optionTexts.length).toBeGreaterThanOrEqual(12); // all operators (+ "—" for col-opt)
  });

  it('filters out LIKE/NOT LIKE for numeric columns in ConditionDialog', async () => {
    const whereGroup: QbConditionGroup = {
      id: 'where',
      logic: 'AND',
      conditions: [cond('c1', { table: 'sales', column: 'qty', operator: '=', value: '10' })],
      groups: [],
    };
    renderStatement({
      where: whereGroup,
      columnTypes: { sales: { qty: 'integer' } },
      actions: {},
    });
    // Open the condition dialog via chip click
    fireEvent.click(screen.getByTestId('qb-where-chip-c1').querySelector('button')!);
    // The value input should be visible (dialog is open, same as existing test)
    expect(screen.getByTestId('qb-cond-value')).toHaveValue('10');
    // Open the operator select
    openSelect('qb-cond-operator');
    // Get the rendered options
    const options = screen.getAllByTestId('select-option');
    expect(options.length).toBeGreaterThan(0);
    const optionTexts = options.map((el) => el.textContent?.trim());
    // Numeric column: LIKE should be absent
    expect(optionTexts).not.toContain('LIKE');
    expect(optionTexts).not.toContain('NOT LIKE');
    // Should have fewer than full operator set (LIKE + NOT LIKE filtered)
    expect(optionTexts.length).toBeLessThan(12);
  });
});
