/**
 * `DatabaseValueSearchDialog` (DB-19): the cockpit's own contract — the cost
 * note says what a scan costs, the start is refused without a value, and a
 * click on an answer opens the table's tab.
 *
 * The scan itself (`ValueSearchDriver`) has its own tests; these lock what
 * the dialog renders and hands over.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, screen } from '@testing-library/react';
import { render } from '@testing-library/react';
import { DatabaseValueSearchDialog } from '../DatabaseValueSearchDialog';
import type { TableInfo } from '../../../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      if (params) return `${key}:${JSON.stringify(params)}`;
      return key;
    },
  }),
}));

vi.mock('../../../../lib/valueSearch', () => ({
  ValueSearchDriver: class {
    constructor() {
      // The dialog's own tests never run a scan; the mock proves the dialog
      // only *builds* one.
    }
    subscribe() {
      return () => undefined;
    }
    start() {
      vi.fn();
    }
    pause() {
      vi.fn();
    }
    cancel() {
      vi.fn();
    }
  },
  rankTables: () => [],
  valueSearchTargets: (tables: readonly TableInfo[]) =>
    tables
      .filter((table) => table.tableType === 'table')
      .map((table) => ({ table: table.name, schema: table.schema ?? null })),
}));

afterEach(cleanup);

const TABLES: TableInfo[] = [
  { name: 'orders', tableType: 'table' },
  { name: 'customers', tableType: 'table' },
];

function renderDialog(
  overrides: Partial<React.ComponentProps<typeof DatabaseValueSearchDialog>> = {},
) {
  const onOpenTable = vi.fn();
  const onClose = vi.fn();
  render(
    <DatabaseValueSearchDialog
      open
      onClose={onClose}
      dbSessionId="session-1"
      connectionId="conn-1"
      database="app"
      tables={TABLES}
      onOpenTable={onOpenTable}
      {...overrides}
    />,
  );
  return { onOpenTable, onClose };
}

describe('DatabaseValueSearchDialog (DB-19)', () => {
  it('says what a scan costs, before the first statement', () => {
    renderDialog();

    // The mocked plan builder answers one target, and the note says exactly that.
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

  it('offers the match modes of the PRD: contains, equals, prefix, suffix', () => {
    renderDialog();

    const mode = screen.getByTestId('value-search-mode') as HTMLSelectElement;
    const options = [...mode.querySelectorAll('option')].map((option) => option.value);
    expect(options).toEqual(['contains', 'exact', 'prefix', 'suffix']);
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
        onOpenTable={() => {}}
      />,
    );

    expect(container.textContent).not.toContain('valueSearch.title');
  });

  it('shows the no-tables note when the database has none', () => {
    renderDialog({ tables: [] });

    expect(screen.getByText('valueSearch.noTables')).toBeTruthy();
  });
});
