import { describe, it, expect } from 'vitest';
import { inferFieldType, inferAllFields, isChartableResult } from '../fieldInference';
import type { ColumnInfo, StatementResult } from '../../../types';

function col(name: string, dataType: string): ColumnInfo {
  return { name, dataType, nullable: true };
}

describe('inferFieldType', () => {
  it('detects numeric by data type', () => {
    expect(inferFieldType(col('id', 'int4'), [1, 2, 3])).toBe('numeric');
    expect(inferFieldType(col('price', 'numeric(10,2)'), [1.5])).toBe('numeric');
    expect(inferFieldType(col('amount', 'float8'), [0.5])).toBe('numeric');
    expect(inferFieldType(col('x', 'bigint'), [100])).toBe('numeric');
  });

  it('detects datetime by data type', () => {
    expect(inferFieldType(col('created', 'timestamp'), ['2024-01-01'])).toBe('datetime');
    expect(inferFieldType(col('day', 'date'), ['2024-01-01'])).toBe('datetime');
  });

  it('detects boolean by data type', () => {
    expect(inferFieldType(col('active', 'bool'), [true, false])).toBe('boolean');
  });

  it('infers numeric from sample values when type is text', () => {
    expect(inferFieldType(col('x', 'text'), [1, 2, 3])).toBe('numeric');
  });

  it('infers boolean from sample values when type is text', () => {
    expect(inferFieldType(col('x', 'text'), [true, false, true])).toBe('boolean');
  });

  it('infers datetime from sample values', () => {
    expect(inferFieldType(col('d', 'varchar'), ['2024-01-15', '2024-02-01'])).toBe('datetime');
  });

  it('falls back to categorical for string values', () => {
    expect(inferFieldType(col('name', 'text'), ['Alice', 'Bob'])).toBe('categorical');
  });

  it('returns unknown when all values are null', () => {
    expect(inferFieldType(col('x', 'text'), [null, null])).toBe('unknown');
  });
});

describe('inferAllFields', () => {
  it('infers types for all columns', () => {
    const result: StatementResult = {
      sql: 'SELECT ...',
      columns: [col('name', 'text'), col('age', 'int4'), col('date', 'timestamp')],
      rows: [
        ['Alice', 30, '2024-01-01'],
        ['Bob', 25, '2024-02-01'],
      ],
      executionTimeMs: 10,
    };
    const fields = inferAllFields(result);

    expect(fields).toHaveLength(3);
    expect(fields[0].name).toBe('name');
    expect(fields[0].inferredType).toBe('categorical');
    expect(fields[1].name).toBe('age');
    expect(fields[1].inferredType).toBe('numeric');
    expect(fields[2].name).toBe('date');
    expect(fields[2].inferredType).toBe('datetime');
  });

  it('counts distinct values', () => {
    const result: StatementResult = {
      sql: 'SELECT ...',
      columns: [col('status', 'text')],
      rows: [['active'], ['inactive'], ['active'], ['active']],
      executionTimeMs: 10,
    };
    const fields = inferAllFields(result);
    expect(fields[0].distinctCount).toBe(2);
  });
});

describe('isChartableResult', () => {
  it('returns true when numeric column present', () => {
    const result: StatementResult = {
      sql: '',
      columns: [col('val', 'int4')],
      rows: [[1], [2]],
      executionTimeMs: 0,
    };
    expect(isChartableResult(result)).toBe(true);
  });

  it('returns false for empty rows', () => {
    const result: StatementResult = {
      sql: '',
      columns: [col('val', 'int4')],
      rows: [],
      executionTimeMs: 0,
    };
    expect(isChartableResult(result)).toBe(false);
  });

  it('returns false when no numeric column', () => {
    const result: StatementResult = {
      sql: '',
      columns: [col('name', 'text')],
      rows: [['Alice']],
      executionTimeMs: 0,
    };
    expect(isChartableResult(result)).toBe(false);
  });
});
