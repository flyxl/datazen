/**
 * `DatabaseValueSearchDialog` (DB-19 + DBR-03): the cockpit's own contract —
 * the cost note says what a scan costs, the start is refused without a
 * value, a click on an answer opens the table's tab carrying the scan's own
 * conditions, and the DBR-03 surfaces (not-contains mode, scope picker with
 * schemas, type scope by driver declaration, zero-hit vs failed lists) all
 * render what they promise.
 *
 * The scan itself (`ValueSearchDriver`) has its own tests; these lock what
 * the dialog renders and hands over.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, act } from '@testing-library/react';
import { DatabaseValueSearchDialog } from '../DatabaseValueSearchDialog';
import type { ValueSearchProgress } from '../../../../lib/valueSearch';
import type { TableInfo } from '../../../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      if (params) return `${key}:${JSON.stringify(params)}`;
      return key;
    },
  }),
}));

// The ui Select is the production picker; a plain select here keeps the
// assertions about the modes simple (the guard against native selects
// excludes __tests__). The mock honours `triggerDataAttrs` so every Select
// of the dialog keeps its own test id.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  Select: (props: {
    value: string;
    options: { value: string }[];
    onChange: (value: string) => void;
    triggerDataAttrs?: Record<string, string>;
  }) => (
    <select
      data-testid={props.triggerDataAttrs?.['data-testid'] ?? 'value-search-mode'}
      value={props.value}
      onChange={(event) => props.onChange(event.target.value)}
    >
      {props.options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.value}
        </option>
      ))}
    </select>
  ),
}));

// The real helpers (targets, ranking, type scopes) are the production ones;
// only the driver is a programmable stand-in, because these tests render
// answers instead of running scans.
const driverState = vi.hoisted(() => ({
  listener: null as ((progress: ValueSearchProgress) => void) | null,
}));
vi.mock('../../../../lib/valueSearch', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../../lib/valueSearch')>()),
  ValueSearchDriver: class {
    subscribe(listener: (progress: ValueSearchProgress) => void) {
      driverState.listener = listener;
      return () => {
        driverState.listener = null;
      };
    }
    start() {
      // The dialog hands the first progress over on subscribe; the tests
      // that need a rendered scan push their own state through the listener.
    }
    pause() {}
    cancel() {}
  },
}));

afterEach(cleanup);

const TABLES: TableInfo[] = [
  { name: 'orders', tableType: 'table' },
  { name: 'customers', tableType: 'table' },
];

function renderDialog(
  overrides: Partial<React.ComponentProps<typeof DatabaseValueSearchDialog>> = {},
) {
  const onLocate = vi.fn();
  const onClose = vi.fn();
  render(
    <DatabaseValueSearchDialog
      open
      onClose={onClose}
      dbSessionId="session-1"
      connectionId="conn-1"
      database="app"
      tables={TABLES}
      onLocate={onLocate}
      {...overrides}
    />,
  );
  return { onLocate, onClose };
}

/** Push a scan state through the mocked driver's subscription. */
function pushProgress(progress: Partial<ValueSearchProgress>): void {
  act(() => {
    driverState.listener?.({
      status: 'done',
      completed: 0,
      total: 0,
      current: null,
      results: [],
      elapsedMs: null,
      ...progress,
    });
  });
}

/** Subscribe the dialog to the mocked driver: a value, then the start. */
function startScan(): void {
  fireEvent.change(screen.getByTestId('value-search-input'), {
    target: { value: 'acme' },
  });
  fireEvent.click(screen.getByTestId('value-search-start'));
}

describe('DatabaseValueSearchDialog (DB-19)', () => {
  it('says what a scan costs, before the first statement', () => {
    renderDialog();

    // The real plan builder answers two targets, and the note says exactly that.
    expect(screen.getByTestId('value-search-cost-note').textContent).toContain('"count":2');
  });

  it('refuses to start without a value', () => {
    renderDialog();

    expect(screen.getByTestId('value-search-start')).toBeDisabled();
  });

  it('enables the start once a value is typed', () => {
    renderDialog();

    const input = screen.getByTestId('value-search-input');
    fireEvent.change(input, { target: { value: 'acme' } });

    expect(screen.getByTestId('value-search-start')).not.toBeDisabled();
  });

  it('offers the match modes of the PRD, not-contains included (DBR-03)', () => {
    renderDialog();

    const mode = screen.getByTestId('value-search-mode') as HTMLSelectElement;
    const options = [...mode.querySelectorAll('option')].map((option) => option.value);
    expect(options).toEqual(['contains', 'notContains', 'exact', 'prefix', 'suffix']);
    fireEvent.change(mode, { target: { value: 'notContains' } });
    expect(mode.value).toBe('notContains');
  });

  it('renders nothing when closed', () => {
    const { container } = render(
      <DatabaseValueSearchDialog
        open={false}
        onClose={() => {}}
        dbSessionId="session-1"
        connectionId="conn-1"
        database="app"
        tables={[]}
        onLocate={() => {}}
      />,
    );

    expect(container.textContent).not.toContain('valueSearch.title');
  });

  it('shows the no-tables note when the database has none', () => {
    renderDialog({ tables: [] });

    expect(screen.getByText('valueSearch.noTables')).toBeTruthy();
  });
});

describe('DatabaseValueSearchDialog scope (DBR-03)', () => {
  it('lists the picked tables schema-qualified — a bare name is ambiguous', () => {
    renderDialog({
      tables: [
        { name: 'orders', schema: 'sales', tableType: 'table' },
        { name: 'orders', schema: 'archive', tableType: 'table' },
      ],
    });

    fireEvent.change(screen.getByTestId('value-search-scope'), {
      target: { value: 'selected' },
    });

    // Both same-named tables appear, each under its own schema.
    expect(screen.getByTestId('value-search-table-option-sales.orders')).toBeTruthy();
    expect(screen.getByTestId('value-search-table-option-archive.orders')).toBeTruthy();
  });

  it('narrows the plan to the picked tables and says the smaller cost', () => {
    renderDialog();

    fireEvent.change(screen.getByTestId('value-search-scope'), {
      target: { value: 'selected' },
    });
    expect(screen.getByTestId('value-search-cost-note').textContent).toContain('"count":0');

    fireEvent.click(
      screen.getByTestId('value-search-table-option-customers').querySelector('input')!,
    );
    expect(screen.getByTestId('value-search-cost-note').textContent).toContain('"count":1');
  });

  it('filters the picker list by the typed needle', () => {
    renderDialog();

    fireEvent.change(screen.getByTestId('value-search-scope'), {
      target: { value: 'selected' },
    });
    fireEvent.change(screen.getByTestId('value-search-table-filter'), {
      target: { value: 'cust' },
    });

    expect(screen.queryByTestId('value-search-table-option-orders')).toBeNull();
    expect(screen.getByTestId('value-search-table-option-customers')).toBeTruthy();
  });
});

describe('DatabaseValueSearchDialog type scope (DBR-03)', () => {
  it('offers only the base scope to a driver that declares nothing', () => {
    renderDialog({ driverType: 'postgresql' });

    const types = screen.getByTestId('value-search-types') as HTMLSelectElement;
    expect([...types.querySelectorAll('option')].map((option) => option.value)).toEqual(['text']);
  });

  it('offers the declared groups to a driver that can match them as text', () => {
    renderDialog({ driverType: 'mysql' });

    const types = screen.getByTestId('value-search-types') as HTMLSelectElement;
    expect([...types.querySelectorAll('option')].map((option) => option.value)).toEqual([
      'text',
      'text+number',
      'text+datetime',
      'text+number+datetime',
    ]);

    // The base scope says nothing about matching kinds; a wider one must.
    expect(screen.queryByTestId('value-search-types-note')).toBeNull();
    fireEvent.change(types, { target: { value: 'text+number' } });
    expect(screen.getByTestId('value-search-types-note').textContent).toBe(
      'valueSearch.types.note',
    );
  });
});

describe('DatabaseValueSearchDialog answers (DBR-03)', () => {
  it('separates the zero-hit tables from the failed ones', () => {
    renderDialog();
    startScan();
    pushProgress({
      status: 'done',
      completed: 3,
      total: 3,
      results: [
        { table: 'orders', schema: null, matches: 2, columns: ['name'] },
        { table: 'customers', schema: null, matches: 0, columns: [] },
        { table: 'archive', schema: null, matches: null, columns: [], reason: 'denied' },
      ],
      elapsedMs: 1200,
    });

    // The hit list holds the hit; the zero-hit and the error lists hold one
    // entry each — three different facts, three different places.
    const results = screen.getByTestId('value-search-results');
    expect(results.textContent).toContain('orders');
    expect(results.textContent).not.toContain('customers');

    expect(screen.getByTestId('value-search-zero-hits-summary').textContent).toContain('"count":1');
    expect(screen.getByTestId('value-search-errors-summary').textContent).toContain('"count":1');
    // The measured cost, no invented ETA.
    expect(screen.getByTestId('value-search-done-note').textContent).toContain('"seconds":1');
  });

  it("locates with the scan's own question — value, mode and the searched columns", () => {
    const { onLocate, onClose } = renderDialog();

    fireEvent.change(screen.getByTestId('value-search-input'), {
      target: { value: '  acme  ' },
    });
    fireEvent.change(screen.getByTestId('value-search-mode'), {
      target: { value: 'notContains' },
    });
    // The run happens (start); the answer below is what it produced.
    fireEvent.click(screen.getByTestId('value-search-start'));
    pushProgress({
      status: 'done',
      completed: 1,
      total: 1,
      results: [{ table: 'orders', schema: 'sales', matches: 5, columns: ['name', 'email'] }],
      elapsedMs: 100,
    });

    fireEvent.click(screen.getAllByTestId('value-search-locate-sales.orders')[0]);

    expect(onLocate).toHaveBeenCalledWith({
      table: 'orders',
      schema: 'sales',
      database: 'app',
      value: 'acme',
      mode: 'notContains',
      columns: ['name', 'email'],
    });
    expect(onClose).toHaveBeenCalled();
  });

  it('keeps the schema on the answer — same-named tables stay distinct', () => {
    renderDialog({
      tables: [
        { name: 'orders', schema: 'sales', tableType: 'table' },
        { name: 'orders', schema: 'archive', tableType: 'table' },
      ],
    });
    startScan();
    pushProgress({
      status: 'done',
      completed: 2,
      total: 2,
      results: [
        { table: 'orders', schema: 'sales', matches: 1, columns: ['name'] },
        { table: 'orders', schema: 'archive', matches: 2, columns: ['name'] },
      ],
      elapsedMs: 50,
    });

    // Two answers, one per schema-qualified identity — never merged, never
    // duplicated as the same key. Each renders twice by design (the "most
    // likely" box and the full hit list), so read them as lists.
    expect(screen.getAllByTestId('value-search-result-sales.orders')).toHaveLength(2);
    expect(screen.getAllByTestId('value-search-result-archive.orders')).toHaveLength(2);
  });

  it('refuses to locate an answer that was never queried — nothing to seed', () => {
    renderDialog();
    startScan();
    pushProgress({
      status: 'done',
      completed: 1,
      total: 1,
      results: [{ table: 'customers', schema: null, matches: 0, columns: [] }],
      elapsedMs: 10,
    });

    // The zero-hit table without searchable columns renders no locate entry
    // in the hit list at all; the empty-state line says the honest thing.
    expect(screen.queryByTestId('value-search-locate-customers')).toBeNull();
  });
});
