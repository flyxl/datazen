import { describe, expect, it } from 'vitest';
import type { SchemaDiffObjectIdentity } from '../../../commands/schemaDiff';
import { schemaDiffObjectIdentityKey } from '../schemaDiffObjectIdentity';

describe('schema diff object identity', () => {
  it('keeps overloaded routines and relation-bound triggers distinct', () => {
    const integerRoutine: SchemaDiffObjectIdentity = {
      kind: 'function',
      schema: 'public',
      name: 'lookup_order',
      signature: 'integer',
    };
    const textRoutine: SchemaDiffObjectIdentity = {
      ...integerRoutine,
      signature: 'text',
    };
    const ordersTrigger: SchemaDiffObjectIdentity = {
      kind: 'trigger',
      schema: 'public',
      name: 'audit_row',
      targetSchema: 'public',
      targetName: 'orders',
    };
    const usersTrigger: SchemaDiffObjectIdentity = {
      ...ordersTrigger,
      targetName: 'users',
    };

    expect(schemaDiffObjectIdentityKey(integerRoutine)).not.toBe(
      schemaDiffObjectIdentityKey(textRoutine),
    );
    expect(schemaDiffObjectIdentityKey(ordersTrigger)).not.toBe(
      schemaDiffObjectIdentityKey(usersTrigger),
    );
  });

  it('normalizes absent optional identity fields the same as null fields', () => {
    expect(schemaDiffObjectIdentityKey({ kind: 'view', name: 'orders_view' })).toBe(
      schemaDiffObjectIdentityKey({
        kind: 'view',
        schema: null,
        name: 'orders_view',
        signature: null,
        targetSchema: null,
        targetName: null,
      }),
    );
  });
});
