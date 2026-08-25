import { describe, expect, it, vi } from 'vitest';
import { invokeGetKey, invokeScanKeys, type RedisInvokeFn } from '../redisInvoke';

describe('redis KV invoke helpers', () => {
  const invoke = vi.fn<RedisInvokeFn>();

  it('invokeScanKeys uses scan_keys driver command with camelCase args', async () => {
    invoke.mockResolvedValue({ cursor: 0, keys: [], dbSize: 0 });
    await invokeScanKeys('conn-1', 2, 'user:*', 42, 100, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'scan_keys', {
      dbSessionId: 'conn-1',
      dbIndex: 2,
      pattern: 'user:*',
      cursor: 42,
      count: 100,
    });
  });

  it('invokeGetKey uses get_key driver command', async () => {
    invoke.mockResolvedValue({
      key: 'user:1',
      keyType: 'string',
      ttl: -1,
      value: 'alice',
    });
    const detail = await invokeGetKey('conn-1', 0, 'user:1', invoke);
    expect(detail.key).toBe('user:1');
    expect(invoke).toHaveBeenCalledWith('redis', 'get_key', {
      dbSessionId: 'conn-1',
      dbIndex: 0,
      key: 'user:1',
    });
  });
});
