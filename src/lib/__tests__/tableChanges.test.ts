import { describe, expect, it } from 'vitest';
import { buildRowIdentity, rowIdentityKey, valuesEqual } from '../tableChanges';

const primaryKey = (name: string) => ({
  name,
  dataType: 'integer',
  nullable: false,
  isPrimaryKey: true,
});

describe('tableChanges', () => {
  it('builds a stable composite identity and key independent of column order', () => {
    const row = { tenantId: 4, id: 9, name: 'Alice' };
    const identity = buildRowIdentity(row, [primaryKey('tenantId'), primaryKey('id')]);
    const reordered = buildRowIdentity(row, [primaryKey('id'), primaryKey('tenantId')]);

    expect(identity).toEqual({ tenantId: 4, id: 9 });
    expect(rowIdentityKey(identity!)).toBe(rowIdentityKey(reordered!));
  });

  it('does not create an identity for a table without primary keys', () => {
    expect(buildRowIdentity({ name: 'Alice' }, [])).toBeNull();
  });

  it('compares nullish values as the same value', () => {
    expect(valuesEqual(undefined, null)).toBe(true);
    expect(valuesEqual('Alice', 'Alice')).toBe(true);
    expect(valuesEqual('Alice', 'Bob')).toBe(false);
  });
});
