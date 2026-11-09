/**
 * The "Set Value" dialog entries at the panel's editing layer (DBR-05):
 * the gates `resolveSetValueState` derives, and the routing `useTableEditing`
 * gives the two dialog actions.
 *
 * The hook is mounted through a tiny host component — the same registry a
 * real panel populates — so the exported `resolveSetValueState` and the
 * `cellEditing` context are the **production** paths, not a re-implemented
 * shadow of them.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import {
  useTableEditing,
  resolveSetValueState,
  type UseTableEditingOptions,
} from '../tableEditing';
import type { TableEditing } from '../tableEditing';
import type { TableState } from '../../../stores/tableData/types';
import type { ColumnSchema } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const tableStore = vi.hoisted(() => ({
  byPanel: new Map<string, TableState>(),
  stageCellValue: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const COLUMNS: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
  { name: 'created_at', dataType: 'date', nullable: true },
  { name: 'seen_at', dataType: 'timestamptz', nullable: true },
  { name: 'avatar', dataType: 'bytea', nullable: true },
  { name: 'note', dataType: 'text', nullable: true },
];

/** The captured context, read through an accessor: a function's return type
 * is the declared union, out of reach of the flow analysis that would
 * narrow a plain variable to its last visible assignment. */
let capturedValue: TableEditing | null = null;

function readCaptured(): TableEditing | null {
  return capturedValue;
}

function Host({ options }: { options: UseTableEditingOptions }) {
  capturedValue = useTableEditing(options);
  return null;
}

function mountHost(overrides: Partial<UseTableEditingOptions> = {}) {
  capturedValue = null;
  render(
    <Host
      options={{
        panelId: 'p1',
        dbSessionId: 's1',
        database: 'app',
        schema: null,
        tableName: 'people',
        databaseType: 'postgresql',
        connectionReadOnly: false,
        columns: COLUMNS,
        ...overrides,
      }}
    />,
  );
  const result = readCaptured();
  expect(result).not.toBeNull();
  if (!result) throw new Error('the host did not capture its editing context');
  return result;
}

afterEach(() => {
  cleanup();
  capturedValue = null;
  vi.clearAllMocks();
  tableStore.byPanel = new Map();
});

describe('resolveSetValueState — the dialog gates (DBR-05)', () => {
  it('offers the picker for a plain date column and a datetime column', () => {
    mountHost();
    expect(resolveSetValueState('p1', 0, 'created_at')?.canPickDate).toBe(true);
  });

  it('keeps a zoned column on the text path — no picker', () => {
    mountHost();
    // The picker cannot hold an offset; dropping it would move the instant
    // (05 §7 #8). The gate is the only place this is decided.
    expect(resolveSetValueState('p1', 0, 'seen_at')?.canPickDate).toBe(false);
  });

  it('offers the binary file loader for a binary column', () => {
    mountHost();
    const state = resolveSetValueState('p1', 0, 'avatar');
    expect(state?.canLoadBinary).toBe(true);
    // The in-place editor's refusal (`binaryColumn`) is exactly what the
    // loader answers: the column reason stays, the write entry appears.
    expect(state?.canLoadCandidates).toBe(false);
  });

  it('hides both entries on a read-only panel', () => {
    mountHost({ connectionReadOnly: true });
    expect(resolveSetValueState('p1', 0, 'created_at')?.canPickDate).toBe(false);
    expect(resolveSetValueState('p1', 0, 'avatar')?.canLoadBinary).toBe(false);
  });

  it('hides both entries on a panel whose table has no primary key', () => {
    mountHost({
      columns: COLUMNS.map((column) => ({ ...column, isPrimaryKey: false })),
    });
    expect(resolveSetValueState('p1', 0, 'created_at')?.canPickDate).toBe(false);
    expect(resolveSetValueState('p1', 0, 'avatar')?.canLoadBinary).toBe(false);
  });

  it('keeps the file loader off an auto-increment column', () => {
    // A binary identity column is the pure form of the rule: the type alone
    // would allow the loader, the auto-increment flag is what refuses it —
    // the database fills the column itself, no dangerous entry (PRD §8).
    mountHost({
      columns: [
        ...COLUMNS,
        { name: 'token', dataType: 'bytea', nullable: false, isAutoIncrement: true },
      ],
    });
    expect(resolveSetValueState('p1', 0, 'token')?.canLoadBinary).toBe(false);
    // The plain binary column beside it keeps its loader.
    expect(resolveSetValueState('p1', 0, 'avatar')?.canLoadBinary).toBe(true);
  });

  it('offers nothing dialog-shaped for a plain text column', () => {
    mountHost();
    const state = resolveSetValueState('p1', 0, 'note');
    expect(state?.canPickDate).toBe(false);
    expect(state?.canLoadBinary).toBe(false);
  });
});

describe('useTableEditing — the dialog routing (DBR-05)', () => {
  it('hands the two dialog actions to their host', () => {
    const onSetValueDialog = vi.fn();
    const editing = mountHost({ onSetValueDialog });

    editing.cellEditing.onSetValue?.(0, 'created_at', 'pickDate');
    expect(onSetValueDialog).toHaveBeenCalledWith('pickDate', 0, 'created_at');

    editing.cellEditing.onSetValue?.(0, 'avatar', 'loadBinaryFromFile');
    expect(onSetValueDialog).toHaveBeenCalledWith('loadBinaryFromFile', 0, 'avatar');

    // The dialog actions never reach the direct staging path.
    expect(tableStore.stageCellValue).not.toHaveBeenCalled();
  });

  it('keeps the value intents on the direct staging path', () => {
    const editing = mountHost();
    editing.cellEditing.onSetValue?.(0, 'note', 'setNull');
    expect(tableStore.stageCellValue).toHaveBeenCalledWith('p1', 0, 'note', null);
  });

  it('hides the dialog entries when the panel has no host for them', () => {
    const editing = mountHost();
    // Without an onSetValueDialog the state the menu reads must not offer
    // what no one can finish.
    const state = editing.cellEditing.setValueState?.(0, 'created_at');
    expect(state?.canPickDate).toBe(false);
    expect(state?.canLoadBinary).toBe(false);
  });
});
