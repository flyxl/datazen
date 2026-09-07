import { describe, expect, it } from 'vitest';
import {
  clearParamHistoryEntry,
  emptyParamHistoryStore,
  isSensitiveParamName,
  loadParamHistory,
  MAX_PARAM_HISTORY,
  normalizeParamHistoryKey,
  ParamHistoryCore,
  parseParamHistoryStore,
  rememberParamValue,
  rememberSubmittedValues,
} from '../core';

describe('param history core', () => {
  it('normalizes keys with connection isolation', () => {
    expect(normalizeParamHistoryKey('conn-a', 'UserId')).toBe('conn-a::userid');
    expect(normalizeParamHistoryKey('conn-b', 'userid')).toBe('conn-b::userid');
  });

  it('flags sensitive descriptor keys', () => {
    expect(isSensitiveParamName('db_password')).toBe(true);
    expect(isSensitiveParamName('apiToken')).toBe(true);
    expect(isSensitiveParamName('user_id')).toBe(false);
  });

  it('stores trimmed values with dedupe and max limit', () => {
    let store = emptyParamHistoryStore();
    const conn = 'c1';
    const key = 'named:status';
    for (const value of ['open', 'closed', 'open', 'pending', 'done', 'draft', 'final']) {
      store = rememberParamValue(store, conn, key, value);
    }
    const history = loadParamHistory(store, conn, key);
    expect(history).toHaveLength(MAX_PARAM_HISTORY);
    expect(history[0]).toBe('final');
    expect(history.filter((v) => v === 'open')).toHaveLength(1);
  });

  it('skips empty and sensitive values', () => {
    let store = emptyParamHistoryStore();
    store = rememberParamValue(store, 'c1', 'named:secret_key', 'abc');
    store = rememberParamValue(store, 'c1', 'named:ok', '   ');
    expect(store.entries).toEqual({});
  });

  it('recovers from corrupted storage', () => {
    expect(parseParamHistoryStore('{bad json')).toEqual(emptyParamHistoryStore());
    expect(parseParamHistoryStore(JSON.stringify({ version: 2 }))).toEqual(
      emptyParamHistoryStore(),
    );
  });

  it('clears a single descriptor history entry', () => {
    let store = rememberParamValue(emptyParamHistoryStore(), 'c1', 'named:x', '1');
    store = clearParamHistoryEntry(store, 'c1', 'named:x');
    expect(loadParamHistory(store, 'c1', 'named:x')).toEqual([]);
  });

  it('persists through ParamHistoryCore storage adapter', () => {
    let persisted: string | null = null;
    const storage = {
      read: () => persisted,
      write: (raw: string) => {
        persisted = raw;
        return true;
      },
    };
    const core = new ParamHistoryCore(storage);
    core.remember('conn-1', 'named:city', 'Shanghai');
    core.rememberSubmitted('conn-1', { 'named:city': 'Beijing', 'named:password_hash': 'x' });
    expect(core.load('conn-1', 'named:city')).toEqual(['Beijing', 'Shanghai']);
    expect(core.load('conn-2', 'named:city')).toEqual([]);

    const reloaded = new ParamHistoryCore(storage);
    expect(reloaded.load('conn-1', 'named:city')).toEqual(['Beijing', 'Shanghai']);
  });

  it('rememberSubmitted merges multiple keys', () => {
    const store = rememberSubmittedValues(emptyParamHistoryStore(), 'c1', {
      'named:a': '1',
      'named:b': '2',
    });
    expect(loadParamHistory(store, 'c1', 'named:a')).toEqual(['1']);
    expect(loadParamHistory(store, 'c1', 'named:b')).toEqual(['2']);
  });
});
