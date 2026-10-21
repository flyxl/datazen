import { describe, expect, it } from 'vitest';
import type { Value } from '../../types';
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  draftWriteColumns,
  isCompleteTableChangeContext,
  rowIdentityKey,
  tableChangeContextKey,
  valuesEqual,
  type PendingRowChange,
  type RowChangePlan,
} from '../tableChanges';

/**
 * `true` only when the key `K` of `T` is **optional**.
 *
 * The parity assertion in `packages/backend-client/__tests__/tableData.test.ts`
 * compares the key **sets**, not their optionality: a field that is optional
 * on one side and required on the other has the same keys and stays assignable
 * in both directions. That gap matters here — the host omits `insertMarked`
 * when it is false, so a mirror that declared the flag as required would
 * claim a key the wire does not carry, and no other assertion would notice.
 */
type IsOptional<T, K extends keyof T> = {} extends Pick<T, K> ? true : false;

/** Fails to compile when the checked condition is not `true`. */
type Assert<T extends true> = T;

/**
 * The insert flag stays optional in the **desktop** mirror.
 *
 * Fail-closed: making the flag required turns this into `false` and the
 * program red (the type assertion, not a runtime one — the runtime half of
 * the property is pinned by `pendingChangesForWire`, which is exercised in
 * `src/stores/tableData/__tests__/`).
 */
export type InsertMarkedStaysOptional = Assert<
  IsOptional<PendingRowChange, 'insertMarked'>
>;

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
    for (const field of [
      'connectionId',
      'dbSessionId',
      'driverType',
      'database',
      'schema',
      'table',
    ]) {
      const changed = { ...context, [field]: field === 'schema' ? 'other' : 'other' };
      expect(tableChangeContextKey(changed)).not.toBe(tableChangeContextKey(context));
    }
    expect(isCompleteTableChangeContext({ ...context, database: null })).toBe(false);
    expect(isCompleteTableChangeContext({ ...context, connectionId: null })).toBe(false);
  });

  it('lists the written columns of a draft in a stable, sorted order', () => {
    // The key membership is the write intent: a present key is written, an
    // absent key is not written at all.
    expect(draftWriteColumns({ name: 'ab', note: null })).toEqual(['name', 'note']);
    // The order is the column order of the statement, so it must not depend
    // on the order the user typed the columns in.
    expect(draftWriteColumns({ zeta: 'z', alpha: 'a', mid: 'm' })).toEqual([
      'alpha',
      'mid',
      'zeta',
    ]);
    // Falsy values are values, not "not filled".
    expect(draftWriteColumns({ n: 0, b: false, s: '' })).toEqual(['b', 'n', 's']);
    // A cleared cell loses its key and therefore leaves the column list.
    const values: Record<string, Value | null> = { name: 'ab' };
    delete values.name;
    expect(draftWriteColumns(values)).toEqual([]);
  });

  it('requires the insert list on a plan, because the host always sends it', () => {
    // Fail-closed: the host declares `inserts` with `#[serde(default)]` — a
    // tolerance for *deserializing* a plan that does not carry the field — and
    // serializes it unconditionally. A mirror that made the field optional
    // would describe a shape the host never produces, so the requirement is
    // pinned here: making the field optional again turns the directive below
    // into an unused one (TS2578) and the typecheck goes red.
    const plan: RowChangePlan = {
      planId: 'plan-1',
      fingerprint: 'fingerprint-1',
      table: {
        connectionId: 'connection-1',
        dbSessionId: 'session-1',
        driverType: 'postgresql',
        database: 'app',
        schema: 'public',
        table: 'users',
      },
      updates: [],
      deletes: [],
      warnings: [],
      // @ts-expect-error `inserts` is required: the host always serializes it.
      inserts: undefined,
    };

    // A plan without inserts carries an **empty list**, not a missing key.
    const empty: RowChangePlan = { ...plan, inserts: [] };
    expect(JSON.parse(JSON.stringify(empty)).inserts).toEqual([]);
  });
});
