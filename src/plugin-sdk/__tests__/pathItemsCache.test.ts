import { describe, expect, it, beforeEach } from 'vitest';
import { useSchemaStore } from '../../stores/schemaStore';
import { cachePathItems, getCachedPathItems, subscribeSchemaPathItems } from '../index';

describe('plugin-sdk path item cache', () => {
  beforeEach(() => {
    useSchemaStore.getState().reset();
  });

  it('round-trips cached get_tables rows', () => {
    const items = [{ name: '1/hive', tableType: 'table', schema: 'CATALOG', rowCount: null }];
    expect(getCachedPathItems('1')).toBeUndefined();
    cachePathItems('1', items);
    expect(getCachedPathItems('1')).toEqual(items);
  });

  it('notifies subscribers when autocomplete writes the cache', () => {
    const seen: string[] = [];
    const stop = subscribeSchemaPathItems((cache) => {
      seen.push(...Object.keys(cache));
    });
    cachePathItems('42/hive', [{ name: 't', tableType: 'table', schema: 'snap', rowCount: null }]);
    stop();
    expect(seen).toContain('42/hive');
  });
});
