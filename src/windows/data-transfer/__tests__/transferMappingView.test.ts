import { describe, expect, it } from 'vitest';
import type { TransferTableResult } from '../../../commands/transfer';
import {
  mappingGateAllowsAdvance,
  mappingGateBlockReason,
  normalizeColumnMappings,
  tableHasActiveMappings,
} from '../transferMappingView';

function createNewRow(overrides: Partial<TransferTableResult> = {}): TransferTableResult {
  return {
    sourceTable: 'users',
    targetTable: 'users',
    status: 'DISABLED',
    createNew: true,
    enabled: false,
    sourceColumns: ['id', 'active'],
    targetColumns: [],
    columnMappings: [
      { sourceColumn: 'id', targetColumn: 'id', skip: false },
      { sourceColumn: 'active', targetColumn: 'active', skip: false, targetNativeType: 'BOOLEAN' },
    ],
    ...overrides,
  };
}

describe('transfer mapping view state', () => {
  it('[tester] keeps every disabled create-new source column ready for explicit selection', () => {
    const row = createNewRow();

    expect(normalizeColumnMappings(row)).toEqual(row.columnMappings);
    expect(tableHasActiveMappings(row)).toBe(true);
    expect(row.enabled).toBe(false);
  });

  it('§8.4: the mapping gate is one predicate over the rows it is given, not a snapshot', () => {
    const mapped = createNewRow({ enabled: true, status: 'MATCHED' });
    const disabledButMapped = createNewRow({ enabled: false });
    const enabledButUnmapped = createNewRow({
      enabled: true,
      status: 'MATCHED',
      sourceColumns: [],
      columnMappings: [],
    });

    expect(mappingGateAllowsAdvance([mapped])).toBe(true);
    // A disabled row's mappings do not count, however complete they are.
    expect(mappingGateAllowsAdvance([disabledButMapped])).toBe(false);
    // Nor does an enabled row with nothing mapped survive the gate.
    expect(mappingGateAllowsAdvance([enabledButUnmapped])).toBe(false);
    expect(mappingGateAllowsAdvance([enabledButUnmapped, mapped])).toBe(true);
    expect(mappingGateAllowsAdvance([])).toBe(false);
    // The verdict follows the rows handed in, which is what lets the same call
    // run on the click-time rows and on the rows that survived the prepare.
    expect(mappingGateAllowsAdvance([mapped])).toBe(true);
    expect(mappingGateAllowsAdvance([{ ...mapped, sourceColumns: [] }])).toBe(false);
  });

  it('D-2: a create-new row nobody has named does not clear the mapping gate', () => {
    const ready = { enabled: true, status: 'CREATE_NEW' as const };

    // The only thing missing is the name. Everything else the gate asks for is
    // already true, so the gate is failing on the name alone.
    expect(mappingGateAllowsAdvance([createNewRow({ ...ready, targetTable: '' })])).toBe(false);
    // Whitespace is not a name. It is what an input left holding a space reads
    // as, and the backend writes it verbatim.
    expect(mappingGateAllowsAdvance([createNewRow({ ...ready, targetTable: '   ' })])).toBe(false);
    // Padded with real characters it is a name, and it clears.
    expect(mappingGateAllowsAdvance([createNewRow({ ...ready, targetTable: '  users_v2  ' })])).toBe(
      true,
    );

    // One unnamed create-new row does not veto an otherwise ready one: the gate
    // asks whether the step may be left, not whether every row is finished.
    expect(
      mappingGateAllowsAdvance([
        createNewRow({ ...ready, targetTable: '' }),
        createNewRow({ ...ready, targetTable: 'users_v2' }),
      ]),
    ).toBe(true);
  });

  it('D-2: only an enabled, mapped, unnamed create-new row earns a gate reason', () => {
    const ready = { enabled: true, status: 'CREATE_NEW' as const };

    // The reason the user can act on.
    expect(mappingGateBlockReason([createNewRow({ ...ready, targetTable: '' })])).toBe(
      'transfer.mapping.targetNameRequired',
    );
    // Whitespace counts as unnamed too.
    expect(mappingGateBlockReason([createNewRow({ ...ready, targetTable: '  ' })])).toBe(
      'transfer.mapping.targetNameRequired',
    );

    // Half-work is not a dead end, so it gets no banner: the user is still on
    // their way to a row the gate accepts.
    const notMappedYet = createNewRow({
      ...ready,
      targetTable: '',
      sourceColumns: [],
      columnMappings: [],
    });
    expect(mappingGateBlockReason([notMappedYet])).toBeNull();
    // A disabled row is not the user's to fix.
    expect(mappingGateBlockReason([createNewRow({ targetTable: '' })])).toBeNull();
    // An existing target is named by the backend; the gate is not about it.
    expect(
      mappingGateBlockReason([
        createNewRow({ enabled: true, createNew: false, status: 'MATCHED', targetTable: '' }),
      ]),
    ).toBeNull();
    // Nothing blocked, nothing to say.
    expect(mappingGateBlockReason([])).toBeNull();
    expect(
      mappingGateBlockReason([createNewRow({ ...ready, targetTable: 'users_v2' })]),
    ).toBeNull();
  });
});
