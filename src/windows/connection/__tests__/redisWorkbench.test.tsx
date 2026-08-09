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

  it('invokeSetString uses snake_case plugin args', async () => {
    await invokeSetString('conn-1', 2, 'mykey', 'hello', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      connection_id: 'conn-1',
      db_index: 2,
      key: 'mykey',
      value: 'hello',
    });
  });

  it('invokeHashSet passes field and value', async () => {
    await invokeHashSet('conn-1', 0, 'hash:1', 'f1', 'v1', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'hash_set', {
      connection_id: 'conn-1',
      db_index: 0,
      key: 'hash:1',
      field: 'f1',
      value: 'v1',
    });
  });

  it('invokeSetTtl supports persist sentinel', async () => {
    await invokeSetTtl('conn-1', 1, 'k', -1, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
      connection_id: 'conn-1',
      db_index: 1,
      key: 'k',
      ttl_seconds: -1,
    });
  });

  it('invokeCreateKey routes string type to set_string', async () => {
    await invokeCreateKey('conn-1', 0, 'new', 'string', 'data', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      connection_id: 'conn-1',
      db_index: 0,
      key: 'new',
      value: 'data',
    });
  });
});

describe('redis batch invoke helpers', () => {
  const invoke = vi.fn<PluginInvokeFn>();

  it('invokeDeleteKeys passes key list', async () => {
    invoke.mockResolvedValue(undefined);
    await invokeDeleteKeys('c', 3, ['a', 'b'], invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'delete_keys', {
      connection_id: 'c',
      db_index: 3,
      keys: ['a', 'b'],
    });
  });

  it('invokeBatchDeletePattern returns result', async () => {
    invoke.mockResolvedValue({ deleted: 5, errors: [] });
    const result = await invokeBatchDeletePattern('c', 0, 'user:*', invoke);
    expect(result.deleted).toBe(5);
    expect(invoke).toHaveBeenCalledWith('redis', 'batch_delete_pattern', {
      connection_id: 'c',
      db_index: 0,
      pattern: 'user:*',
    });
  });
});
