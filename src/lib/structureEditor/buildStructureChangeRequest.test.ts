import { describe, it, expect } from 'vitest';
import { buildStructureChangeRequest } from './buildStructureChangeRequest';
import type { StructureColumnDraft, StructureIndexDraft } from './types';

const sampleColumn = (id: string, name: string): StructureColumnDraft => ({
  id,
  name,
  dataType: 'text',
  nullable: true,
  defaultValue: null,
  comment: null,
  isPrimaryKey: false,
  isAutoIncrement: false,
  isUnique: false,
});

const sampleIndex = (id: string, name: string): StructureIndexDraft => ({
  id,
  name,
  columns: ['email'],
  isUnique: true,
  isPrimary: false,
  indexType: 'btree',
});

describe('buildStructureChangeRequest', () => {
  it('omits blank column names from currentColumns', () => {
    const request = buildStructureChangeRequest({
      mode: 'create',
      table: 'users',
      originalColumns: [],
      currentColumns: [sampleColumn('c1', 'id'), sampleColumn('c2', '  ')],
      originalIndexes: [],
      currentIndexes: [],
    });
    expect(request.currentColumns).toHaveLength(1);
    expect(request.currentColumns[0]?.name).toBe('id');
  });

  it('clears original snapshots in create mode', () => {
    const request = buildStructureChangeRequest({
      mode: 'create',
      table: 'users',
      originalColumns: [sampleColumn('o1', 'old')],
      currentColumns: [sampleColumn('c1', 'id')],
      originalIndexes: [sampleIndex('i1', 'idx_old')],
      currentIndexes: [sampleIndex('i2', 'idx_new')],
    });
    expect(request.originalColumns).toEqual([]);
    expect(request.originalIndexes).toEqual([]);
    expect(request.currentIndexes).toHaveLength(1);
  });

  it('omits blank index names from currentIndexes', () => {
    const request = buildStructureChangeRequest({
      mode: 'create',
      table: 'users',
      originalColumns: [],
      currentColumns: [sampleColumn('c1', 'id')],
      originalIndexes: [],
      currentIndexes: [sampleIndex('i1', 'idx_email'), sampleIndex('i2', '  ')],
    });
    expect(request.currentIndexes).toHaveLength(1);
    expect(request.currentIndexes[0]?.name).toBe('idx_email');
  });

  it('passes schema through to the request', () => {
    const request = buildStructureChangeRequest({
      mode: 'create',
      table: 'users',
      schema: 'app',
      originalColumns: [],
      currentColumns: [sampleColumn('c1', 'id')],
      originalIndexes: [],
      currentIndexes: [],
    });
    expect(request.schema).toBe('app');
  });

  it('preserves original snapshots in alter mode', () => {
    const origCol = sampleColumn('c1', 'email');
    const origIdx = sampleIndex('i1', 'idx_email');
    const request = buildStructureChangeRequest({
      mode: 'alter',
      table: 'users',
      schema: 'public',
      originalColumns: [origCol],
      currentColumns: [{ ...origCol, dataType: 'varchar(255)' }],
      originalIndexes: [origIdx],
      currentIndexes: [origIdx],
    });
    expect(request.originalColumns).toEqual([origCol]);
    expect(request.originalIndexes).toEqual([origIdx]);
    expect(request.schema).toBe('public');
  });
});
