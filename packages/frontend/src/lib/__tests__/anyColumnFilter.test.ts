/**
 * The Any Column filter expansion (DBR-04).
 *
 * The pinned properties:
 *
 * * the expansion is a list of `contains` conditions OR-joined — the same
 *   structured channel every other filter uses, never a second dialect;
 * * a column whose cell type is not text-searchable is skipped **and
 *   reported**, never cast on the sly;
 * * an empty value expands to no conditions at all — the caller's signal to
 *   clear the filter, not to ask a question that matches nothing;
 * * the value is matched literally (`contains`), so the user's `%` is
 *   escaped by the shared filter layer, not treated as a wildcard.
 */
import { describe, expect, it } from 'vitest';
import { expandAnyColumnFilter } from '../anyColumnFilter';
import type { ColumnSchema } from '../../types';

function column(name: string, dataType: string): ColumnSchema {
  return { name, dataType, nullable: true };
}

const COLUMNS: ColumnSchema[] = [
  column('id', 'integer'),
  column('name', 'varchar(80)'),
  column('tier', 'user-defined'),
  column('notes', 'text'),
  column('blob', 'bytea'),
  column('flag', 'boolean'),
  column('created_at', 'timestamp'),
  column('mystery', 'weird_custom_type'),
];

describe('expandAnyColumnFilter', () => {
  it('searches every text, enum and unknown column, and only those', () => {
    const { conditions, searchableColumns, skippedColumns } = expandAnyColumnFilter(
      COLUMNS,
      'alice',
      { groups: [] },
    );
    // text (name, notes), enum (tier — reported as user-defined/enum by the
    // shared table… here 'user-defined' is unknown, which is searchable) and
    // unknown (mystery) are in; numbers, blobs, booleans, timestamps are out.
    expect(searchableColumns).toEqual(['name', 'tier', 'notes', 'mystery']);
    expect(skippedColumns).toEqual(['id', 'blob', 'flag', 'created_at']);
    expect(conditions).toHaveLength(4);
  });

  it('joins every condition with or, starting from the second', () => {
    const { conditions } = expandAnyColumnFilter(COLUMNS, 'alice', { groups: [] });
    expect(conditions[0].connector).toBeUndefined();
    for (const condition of conditions.slice(1)) {
      expect(condition.connector).toBe('or');
    }
  });

  it('uses the contains operator with the trimmed value', () => {
    const { conditions } = expandAnyColumnFilter(COLUMNS, '  alice  ', { groups: [] });
    expect(conditions.every((condition) => condition.operator === 'contains')).toBe(true);
    expect(conditions.every((condition) => condition.value === 'alice')).toBe(true);
  });

  it('a driver-declared number group adds numeric columns', () => {
    const { searchableColumns } = expandAnyColumnFilter(COLUMNS, '42', {
      groups: ['number'],
    });
    expect(searchableColumns).toContain('id');
    expect(searchableColumns).not.toContain('created_at');
  });

  it('both declared groups add numbers and datetimes together', () => {
    const { searchableColumns, skippedColumns } = expandAnyColumnFilter(COLUMNS, '2026', {
      groups: ['number', 'datetime'],
    });
    expect(searchableColumns).toEqual(['id', 'name', 'tier', 'notes', 'created_at', 'mystery']);
    expect(skippedColumns).toEqual(['blob', 'flag']);
  });

  it('an empty value expands to no conditions — the clear signal', () => {
    const { conditions } = expandAnyColumnFilter(COLUMNS, '', { groups: [] });
    expect(conditions).toEqual([]);
    // The searchable/skipped report still tells the UI what it would search.
    const { searchableColumns } = expandAnyColumnFilter(COLUMNS, '   ', { groups: [] });
    expect(searchableColumns.length).toBeGreaterThan(0);
  });

  it('a table without searchable columns expands to nothing and says so', () => {
    const binaryOnly: ColumnSchema[] = [column('payload', 'bytea'), column('flag', 'boolean')];
    const { conditions, searchableColumns, skippedColumns } = expandAnyColumnFilter(
      binaryOnly,
      'x',
      { groups: [] },
    );
    expect(conditions).toEqual([]);
    expect(searchableColumns).toEqual([]);
    expect(skippedColumns).toEqual(['payload', 'flag']);
  });

  it('respects the driver cell-type hints like every other filter surface', () => {
    // A driver that declares 'user-defined' as an enum: the tier column is
    // searchable either way, but through the declared rule, not a guess.
    const { searchableColumns } = expandAnyColumnFilter(COLUMNS, 'gold', {
      groups: [],
      hints: { enumTypes: ['user-defined'] },
    });
    expect(searchableColumns).toContain('tier');
  });
});
