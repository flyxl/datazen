import { describe, expect, it, vi } from 'vitest';
import {
  invokeCreateKey,
  invokeHashSet,
  invokeSetString,
  invokeSetTtl,
  type PluginInvokeFn,
} from '../../../../packages/drivers/redis/ui/KeyEditors';
import {
  invokeBatchDeletePattern,
  invokeDeleteKeys,
} from '../../../../packages/drivers/redis/ui/BatchBar';

describe('redis editor invoke helpers', () => {
  const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);

  it('invokeSetString uses camelCase plugin args', async () => {
    await invokeSetString('conn-1', 2, 'mykey', 'hello', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      connectionId: 'conn-1',
      dbIndex: 2,
      key: 'mykey',
      value: 'hello',
    });
  });

  it('invokeHashSet passes field and value', async () => {
    await invokeHashSet('conn-1', 0, 'hash:1', 'f1', 'v1', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'hash_set', {
      connectionId: 'conn-1',
      dbIndex: 0,
      key: 'hash:1',
      field: 'f1',
      value: 'v1',
    });
  });

  it('invokeSetTtl supports persist sentinel', async () => {
    await invokeSetTtl('conn-1', 1, 'k', -1, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
      connectionId: 'conn-1',
      dbIndex: 1,
      key: 'k',
      ttlSeconds: -1,
    });
  });

  it('invokeCreateKey routes string type to set_string', async () => {
    await invokeCreateKey('conn-1', 0, 'new', 'string', 'data', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      connectionId: 'conn-1',
      dbIndex: 0,
      key: 'new',
      value: 'data',
    });
  });
});

describe('redis batch invoke helpers', () => {
  const invoke = vi.fn<PluginInvokeFn>();

  it('invokeDeleteKeys passes key list and returns deleted count', async () => {
    invoke.mockResolvedValue(2);
    const deleted = await invokeDeleteKeys('c', 3, ['a', 'b'], invoke);
    expect(deleted).toBe(2);
    expect(invoke).toHaveBeenCalledWith('redis', 'delete_keys', {
      connectionId: 'c',
      dbIndex: 3,
      keys: ['a', 'b'],
    });
  });

  it('invokeBatchDeletePattern returns result', async () => {
    invoke.mockResolvedValue({ deleted: 5, errors: [] });
    const result = await invokeBatchDeletePattern('c', 0, 'user:*', invoke);
    expect(result.deleted).toBe(5);
    expect(invoke).toHaveBeenCalledWith('redis', 'batch_delete_pattern', {
      connectionId: 'c',
      dbIndex: 0,
      pattern: 'user:*',
    });
  });
});
