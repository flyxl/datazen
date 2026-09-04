import { describe, expect, it } from 'vitest';
import {
  activeFlatten,
  createEmptyConnectionSchema,
  DEFAULT_SCHEMA_KEY,
  extractSchemaPatch,
  patchConnectionSchema,
  resolveRealConnectionId,
  resolveTargetConnectionId,
} from '../schemaStoreState';

describe('[tester] schemaStoreState', () => {
  it('activeFlatten reads active session or default key', () => {
    const schemas = new Map<string, ReturnType<typeof createEmptyConnectionSchema>>();
    const entry = createEmptyConnectionSchema();
    entry.currentDatabase = 'app';
    schemas.set('sess-1', entry);

    expect(activeFlatten(schemas, 'sess-1').currentDatabase).toBe('app');
    expect(activeFlatten(schemas, null).currentDatabase).toBeNull();

    schemas.set(DEFAULT_SCHEMA_KEY, { ...entry, currentDatabase: 'default-db' });
    expect(activeFlatten(schemas, null).currentDatabase).toBe('default-db');
  });

  it('extractSchemaPatch picks only known connection keys', () => {
    const patch = extractSchemaPatch({
      currentDatabase: 'app',
      unknownField: 'ignored',
      loading: true,
    });
    expect(patch).toEqual({ currentDatabase: 'app', loading: true });
    expect(patch).not.toHaveProperty('unknownField');
  });

  it('patchConnectionSchema merges into map immutably', () => {
    const schemas = new Map<string, ReturnType<typeof createEmptyConnectionSchema>>();
    const next = patchConnectionSchema(schemas, 'sess-1', { currentDatabase: 'app' });
    expect(next.get('sess-1')?.currentDatabase).toBe('app');
    expect(schemas.has('sess-1')).toBe(false);
  });

  it('resolveTargetConnectionId falls back to default schema key', () => {
    expect(resolveTargetConnectionId({ activeDbSessionId: 'sess-1' })).toBe('sess-1');
    expect(resolveTargetConnectionId({ activeDbSessionId: null })).toBe(DEFAULT_SCHEMA_KEY);
    expect(resolveTargetConnectionId({ activeDbSessionId: null }, 'explicit')).toBe('explicit');
  });

  it('resolveRealConnectionId never uses default schema key', () => {
    expect(resolveRealConnectionId({ activeDbSessionId: 'sess-1' })).toBe('sess-1');
    expect(resolveRealConnectionId({ activeDbSessionId: null })).toBeNull();
  });
});
