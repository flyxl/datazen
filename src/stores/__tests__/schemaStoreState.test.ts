import { describe, expect, it } from 'vitest';
import { createEmptyConnectionSchema, patchConnectionSchema } from '../schemaStoreState';

describe('[tester] schemaStoreState', () => {
  it('creates a session snapshot without legacy column projections', () => {
    const entry = createEmptyConnectionSchema();
    expect(entry.relationColumns).toEqual({});
    expect(entry.tableCatalogs).toEqual({});
    expect(entry).not.toHaveProperty('columnMap');
    expect(entry).not.toHaveProperty('typedColumnMap');
  });

  it('patches the explicit session without mutating other sessions or the original map', () => {
    const a = { ...createEmptyConnectionSchema(), currentDatabase: 'a' };
    const b = { ...createEmptyConnectionSchema(), currentDatabase: 'b' };
    const schemas = new Map([
      ['session-a', a],
      ['session-b', b],
    ]);
    const next = patchConnectionSchema(schemas, 'session-a', { currentDatabase: 'updated' });
    expect(next.get('session-a')?.currentDatabase).toBe('updated');
    expect(schemas.get('session-a')).toBe(a);
    expect(next.get('session-b')).toBe(b);
  });

  it('creates only the requested session when the map is empty', () => {
    const schemas = new Map<string, ReturnType<typeof createEmptyConnectionSchema>>();
    const next = patchConnectionSchema(schemas, 'session', { currentDatabase: 'app' });
    expect([...next.keys()]).toEqual(['session']);
    expect(schemas.size).toBe(0);
  });

  it('keeps mutable collections isolated between freshly created sessions', () => {
    const a = createEmptyConnectionSchema();
    const b = createEmptyConnectionSchema();
    a.expanded.add('db:app');
    a.loadedPaths.add('app');
    expect(b.expanded.size).toBe(0);
    expect(b.loadedPaths.size).toBe(0);
    expect(a.relationColumns).not.toBe(b.relationColumns);
  });
});
