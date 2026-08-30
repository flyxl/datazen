import { describe, expect, it } from 'vitest';
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  isCompleteTableChangeContext,
  rowIdentityKey,
  tableChangeContextKey,
  valuesEqual,
} from '../tableChanges';

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

  it('rejects NULL or unstable values in every part of a composite identity', () => {
    const keys = [primaryKey('tenantId'), primaryKey('id')];
    expect(buildRowIdentity({ tenantId: 1, id: null }, keys)).toBeNull();
    expect(buildRowIdentity({ tenantId: undefined, id: 9 }, keys)).toBeNull();
    expect(buildRowIdentity({ tenantId: Number.NaN, id: 9 }, keys)).toBeNull();
  });

  it('detects duplicate composite identities in a loaded result', () => {
    const keys = [primaryKey('tenantId'), primaryKey('id')];
    expect(
      duplicateRowIdentityKeys(
        [
          { tenantId: 1, id: 9 },
          { tenantId: 1, id: 9 },
          { tenantId: 2, id: 9 },
        ],
        keys,
      ),
    ).toEqual(['["id":9,"tenantId":1]']);
  });

  it('compares nullish values as the same value', () => {
    expect(valuesEqual(undefined, null)).toBe(true);
    expect(valuesEqual('Alice', 'Alice')).toBe(true);
    expect(valuesEqual('Alice', 'Bob')).toBe(false);
  });

  it('requires a complete write context and fingerprints every routing field', () => {
    const context = {
      connectionId: 'connection-1',
      dbSessionId: 'session-1',
      driverType: 'postgresql',
      database: 'app',
      schema: 'public',
      table: 'users',
    } as const;
    expect(isCompleteTableChangeContext(context)).toBe(true);
    for (const field of ['connectionId', 'dbSessionId', 'driverType', 'database', 'schema', 'table']) {
      const changed = { ...context, [field]: field === 'schema' ? 'other' : 'other' };
      expect(tableChangeContextKey(changed)).not.toBe(tableChangeContextKey(context));
    }
    expect(isCompleteTableChangeContext({ ...context, database: null })).toBe(false);
    expect(isCompleteTableChangeContext({ ...context, connectionId: null })).toBe(false);
  });
});
