import { describe, expect, it, beforeEach } from 'vitest';
import { useSchemaStore } from '../../../src/stores/schemaStore';
import type { TableInfo } from '../../../src/types';
import {
  bindSchemaStore,
  cachePathItems,
  getCachedPathItems,
  subscribeSchemaPathItems,
} from '../src/schemaStoreBridge';

describe('driver-sdk path item cache', () => {
  beforeEach(() => {
    useSchemaStore.getState().reset();
    bindSchemaStore(useSchemaStore);
  });

  it('round-trips cached get_tables rows', () => {
    const items: TableInfo[] = [
      { name: '1/hive', tableType: 'table', schema: 'CATALOG', rowCount: undefined },
    ];
    expect(getCachedPathItems('1', 'session-1')).toBeUndefined();
    cachePathItems('1', items, 'session-1');
    expect(getCachedPathItems('1', 'session-1')).toEqual(items);
  });

  it('notifies subscribers when autocomplete writes the cache', () => {
    const seen: string[] = [];
    const stop = subscribeSchemaPathItems((cache) => {
      seen.push(...Object.keys(cache));
    }, 'session-1');
    cachePathItems(
      '42/hive',
      [{ name: 't', tableType: 'table', schema: 'snap', rowCount: undefined }],
      'session-1',
    );
    stop();
    expect(seen).toContain('42/hive');
  });
});
