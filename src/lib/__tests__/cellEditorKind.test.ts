import { describe, expect, it } from 'vitest';
import {
  CELL_EDIT_MAX_BYTES,
  INLINE_TEXT_MAX_CODE_POINTS,
  cellValueErrorMessageKey,
  coerceValue,
  readOnlyReasonMessageKey,
  resolveCellEditorKind,
  resolveCellEditable,
  type CellEditorKind,
  type CellReadOnlyReason,
  type CellValueErrorReason,
} from '../cellEditorKind';
import type { CellType } from '../cellTypes';

function kind(cellType: CellType, zoned = false, valueCodePoints = 0): CellEditorKind {
  return resolveCellEditorKind({ cellType, zoned, hasCandidates: false, valueCodePoints });
}

describe('resolveCellEditorKind', () => {
  it('dispatches one editor kind per cell type', () => {
    expect(kind('text')).toBe('text');
    expect(kind('number')).toBe('number');
    expect(kind('bool')).toBe('boolean');
    expect(kind('enum')).toBe('enum');
    expect(kind('date')).toBe('date');
    expect(kind('datetime')).toBe('date');
    expect(kind('json')).toBe('json');
    expect(kind('bytes')).toBe('binary');
    expect(kind('uuid')).toBe('text');
    expect(kind('unknown')).toBe('text');
  });

  it('uses the text editor for zoned datetimes', () => {
    // The picker cannot carry an offset: using it would move the instant.
    expect(kind('datetime', true)).toBe('text');
    expect(kind('datetime', false)).toBe('date');
    expect(kind('time', true)).toBe('text');
  });

  it('routes long text and binary to their dedicated kinds', () => {
    expect(kind('text', false, INLINE_TEXT_MAX_CODE_POINTS)).toBe('text');
    expect(kind('text', false, INLINE_TEXT_MAX_CODE_POINTS + 1)).toBe('longText');
    expect(kind('bytes')).toBe('binary');
  });

  it('routes a value with a line break to the multi-line editor', () => {
    expect(
      resolveCellEditorKind({
        cellType: 'text',
        zoned: false,
        hasCandidates: false,
        valueCodePoints: 3,
        hasLineBreak: true,
      }),
    ).toBe('longText');
  });

  it('counts the length in code points, not in UTF-16 units', () => {
    // A hundred emoji are a hundred characters, not two hundred.
    const emoji = '🙂'.repeat(100);
    expect([...emoji].length).toBe(100);
    expect(
      resolveCellEditorKind({
        cellType: 'text',
        zoned: false,
        hasCandidates: false,
        valueCodePoints: [...emoji].length,
      }),
    ).toBe('text');
  });
});

describe('coerceValue', () => {
  it('maps empty text to null for nullable columns', () => {
    const result = coerceValue({
      kind: 'text',
      cellType: 'text',
      text: '',
      nullable: true,
      originalValue: 'something',
    });
    expect(result).toEqual({ ok: true, value: null });
  });

  it('refuses empty text on NOT NULL columns', () => {
    const result = coerceValue({
      kind: 'text',
      cellType: 'text',
      text: '',
      nullable: false,
      originalValue: 'something',
    });
    expect(result).toEqual({ ok: false, reason: 'emptyNotNull' });
  });

  it('rejects javascript numeric quirks', () => {
    for (const text of [' ', '0x10', '١٢', 'NaN', 'Infinity', '-Infinity', '12abc', '1e400']) {
      const result = coerceValue({
        kind: 'number',
        cellType: 'number',
        text,
        nullable: true,
        originalValue: 1,
      });
      expect(result.ok, text).toBe(false);
    }
  });

  it('never turns an empty input into a number', () => {
    // `Number('')` is 0 — the empty input is a NULL (or a refusal), never a
    // zero.
    const result = coerceValue({
      kind: 'number',
      cellType: 'number',
      text: '',
      nullable: true,
      originalValue: 1,
    });
    expect(result).toEqual({ ok: true, value: null });
  });

  it('accepts the numbers a database accepts', () => {
    for (const [text, expected] of [
      ['42', 42],
      ['-42', -42],
      ['+42', 42],
      ['3.14', 3.14],
      ['.5', 0.5],
      ['1e3', 1000],
    ] as const) {
      const result = coerceValue({
        kind: 'number',
        cellType: 'number',
        text,
        nullable: true,
        originalValue: 0,
      });
      expect(result, text).toEqual({ ok: true, value: expected });
    }
  });

  it('refuses a fraction for an integer column', () => {
    const result = coerceValue({
      kind: 'number',
      cellType: 'number',
      text: '1.5',
      nullable: true,
      originalValue: 0,
      integerOnly: true,
    });
    expect(result).toEqual({ ok: false, reason: 'invalidInteger' });
  });

  it('reads the boolean spellings a driver may report', () => {
    for (const [text, expected] of [
      ['true', true],
      ['TRUE', true],
      ['1', true],
      ['false', false],
      ['FALSE', false],
      ['0', false],
    ] as const) {
      const result = coerceValue({
        kind: 'boolean',
        cellType: 'bool',
        text,
        nullable: true,
        originalValue: false,
      });
      expect(result, text).toEqual({ ok: true, value: expected });
    }
  });

  it('refuses a boolean it cannot read', () => {
    const result = coerceValue({
      kind: 'boolean',
      cellType: 'bool',
      text: 'yes',
      nullable: true,
      originalValue: false,
    });
    expect(result).toEqual({ ok: false, reason: 'invalidBoolean' });
  });

  it('rejects invalid calendar dates', () => {
    for (const text of ['2026-02-30', '2026-13-01', 'lorem', '2026-2-3']) {
      const result = coerceValue({
        kind: 'date',
        cellType: 'date',
        text,
        nullable: true,
        originalValue: null,
      });
      expect(result.ok, text).toBe(false);
    }
  });

  it('accepts a real calendar date and keeps it verbatim', () => {
    const result = coerceValue({
      kind: 'date',
      cellType: 'date',
      text: '2026-02-28',
      nullable: true,
      originalValue: null,
    });
    expect(result).toEqual({ ok: true, value: '2026-02-28' });
  });

  it('rejects malformed json but accepts json strings', () => {
    const broken = coerceValue({
      kind: 'json',
      cellType: 'json',
      text: '{not json',
      nullable: true,
      originalValue: null,
    });
    expect(broken).toEqual({ ok: false, reason: 'invalidJson' });

    const asString = coerceValue({
      kind: 'json',
      cellType: 'json',
      text: '"abc"',
      nullable: true,
      originalValue: null,
    });
    expect(asString).toEqual({ ok: true, value: 'abc' });

    const asObject = coerceValue({
      kind: 'json',
      cellType: 'json',
      text: '{"x":1}',
      nullable: true,
      originalValue: null,
    });
    expect(asObject).toEqual({ ok: true, value: { x: 1 } });
  });

  it('preserves zone-less wall-clock timestamps verbatim', () => {
    const result = coerceValue({
      kind: 'date',
      cellType: 'datetime',
      text: '2026-03-01T00:15:30',
      nullable: true,
      originalValue: null,
    });
    // No `new Date()` round trip: the text is the value.
    expect(result).toEqual({ ok: true, value: '2026-03-01T00:15:30' });
  });

  it('keeps the offset of a zoned timestamp untouched', () => {
    const result = coerceValue({
      kind: 'text',
      cellType: 'datetime',
      text: '2026-03-01T00:15:30+02:00',
      nullable: true,
      originalValue: null,
    });
    expect(result).toEqual({ ok: true, value: '2026-03-01T00:15:30+02:00' });
  });

  it('refuses a uuid that is not one', () => {
    const broken = coerceValue({
      kind: 'text',
      cellType: 'uuid',
      text: 'not-a-uuid',
      nullable: true,
      originalValue: null,
    });
    expect(broken).toEqual({ ok: false, reason: 'invalidUuid' });

    const good = coerceValue({
      kind: 'text',
      cellType: 'uuid',
      text: '123e4567-e89b-12d3-a456-426614174000',
      nullable: true,
      originalValue: null,
    });
    expect(good).toEqual({ ok: true, value: '123e4567-e89b-12d3-a456-426614174000' });
  });

  it('refuses to edit a binary value in place', () => {
    const result = coerceValue({
      kind: 'binary',
      cellType: 'bytes',
      text: '[1,2,3]',
      nullable: true,
      originalValue: null,
    });
    expect(result).toEqual({ ok: false, reason: 'notEditable' });
  });
});

describe('resolveCellEditable', () => {
  const base = {
    connectionReadOnly: false,
    relationKind: 'table' as const,
    hasPrimaryKey: true,
    rowIdentityUnique: true,
    column: { name: 'name', nullable: true, isAutoIncrement: false },
    cellType: 'text' as CellType,
    inMemoryByteSize: 10,
    typeReadOnly: false,
    rowKind: 'update' as const,
  };

  it('returns a specific reason for each read-only case', () => {
    expect(resolveCellEditable({ ...base, connectionReadOnly: true }).reason).toBe(
      'connectionReadOnly',
    );
    expect(resolveCellEditable({ ...base, capabilityEditable: false }).reason).toBe(
      'driverCapability',
    );
    expect(resolveCellEditable({ ...base, relationKind: 'view' }).reason).toBe('view');
    expect(resolveCellEditable({ ...base, relationKind: 'materializedView' }).reason).toBe('view');
    expect(resolveCellEditable({ ...base, hasPrimaryKey: false }).reason).toBe('noPrimaryKey');
    expect(resolveCellEditable({ ...base, rowIdentityUnique: false }).reason).toBe(
      'ambiguousRowIdentity',
    );
    expect(
      resolveCellEditable({
        ...base,
        column: { name: 'id', nullable: false, isAutoIncrement: true },
      }).reason,
    ).toBe('autoIncrement');
    expect(resolveCellEditable({ ...base, typeReadOnly: true }).reason).toBe('typeNotEditable');
    expect(resolveCellEditable({ ...base, cellType: 'bytes' }).reason).toBe('binaryColumn');
    expect(resolveCellEditable({ ...base, inMemoryByteSize: CELL_EDIT_MAX_BYTES + 1 }).reason).toBe(
      'largeValue',
    );
  });

  it('lets a new-row draft fill an auto-increment column', () => {
    const result = resolveCellEditable({
      ...base,
      rowKind: 'insert',
      column: { name: 'id', nullable: false, isAutoIncrement: true },
    });
    expect(result.editable).toBe(true);
    expect(result.reason).toBeNull();
  });

  it('prefers the reason the mount point knows', () => {
    // The view panel mounts the grid read-only; the user must be told it is
    // a view, not that the connection is read-only.
    const result = resolveCellEditable({
      ...base,
      connectionReadOnly: true,
      mountReason: 'view',
    });
    expect(result.reason).toBe('view');
  });

  it('carries the error code and the message key of a refusal', () => {
    const result = resolveCellEditable({ ...base, connectionReadOnly: true });
    expect(result.code).toBe('grid.cell.readOnly');
    expect(result.messageKey).toBe('cellEditor.readOnly.connection');
  });

  it('answers "editable" with no reason and no code', () => {
    const result = resolveCellEditable(base);
    expect(result.editable).toBe(true);
    expect(result.reason).toBeNull();
    expect(result.code).toBeNull();
  });
});

describe('readOnlyReasonMessageKey', () => {
  it('is total over every reason', () => {
    const reasons: CellReadOnlyReason[] = [
      'connectionReadOnly',
      'driverCapability',
      'view',
      'noPrimaryKey',
      'ambiguousRowIdentity',
      'autoIncrement',
      'generated',
      'binaryColumn',
      'largeValue',
      'typeNotEditable',
    ];
    for (const reason of reasons) {
      expect(readOnlyReasonMessageKey(reason), reason).toBe(`cellEditor.readOnly.${reason === 'connectionReadOnly' ? 'connection' : reason}`);
    }
  });
});

describe('cellValueErrorMessageKey', () => {
  it('is total over every reason', () => {
    const reasons: CellValueErrorReason[] = [
      'emptyNotNull',
      'invalidNumber',
      'invalidInteger',
      'invalidBoolean',
      'invalidDate',
      'invalidTime',
      'invalidDateTime',
      'invalidJson',
      'invalidUuid',
      'tooLarge',
      'notEditable',
    ];
    for (const reason of reasons) {
      expect(cellValueErrorMessageKey(reason), reason).toMatch(/^cellEditor\./);
    }
  });
});
