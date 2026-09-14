import { describe, expect, it, vi } from 'vitest';
import {
  invokeCreateKey,
  invokeSetExpireAt,
  invokeSetString,
  invokeSetTtl,
  type PluginInvokeFn,
} from '../keyEditorsInvokes';

describe('invokeSetString (PR-1 KEEPTTL)', () => {
  it('passes keepTtl=false by default', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeSetString('sess-1', 0, 'k1', 'v1', false, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'k1',
      value: 'v1',
      keepTtl: false,
    });
  });

  it('passes keepTtl=true for KEEPTTL path', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeSetString('sess-2', 3, 'cache:tmp', 'payload', true, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_string', {
      dbSessionId: 'sess-2',
      dbIndex: 3,
      key: 'cache:tmp',
      value: 'payload',
      keepTtl: true,
    });
  });
});

describe('invokeSetExpireAt (PR-1 EXPIREAT)', () => {
  it('sends expireAt unix timestamp via set_ttl command', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    const ts = 1_700_000_000;
    await invokeSetExpireAt('sess-1', 1, 'k1', ts, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
      dbSessionId: 'sess-1',
      dbIndex: 1,
      key: 'k1',
      expireAt: ts,
    });
  });
});

describe('invokeSetTtl (relative / persist)', () => {
  it('sends ttlSeconds for relative EXPIRE', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeSetTtl('sess-1', 0, 'k1', 3600, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'k1',
      ttlSeconds: 3600,
    });
  });

  it('sends ttlSeconds=-1 for PERSIST', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeSetTtl('sess-1', 0, 'k1', -1, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_ttl', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'k1',
      ttlSeconds: -1,
    });
  });
});

describe('invokeCreateKey', () => {
  it('creates string key via set_string', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeCreateKey('sess-1', 0, 'new:str', 'string', 'hello', invoke);
    expect(invoke).toHaveBeenCalledWith(
      'redis',
      'set_string',
      expect.objectContaining({
        key: 'new:str',
        value: 'hello',
        keepTtl: false,
      }),
    );
  });

  it('creates hash key via hash_set', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeCreateKey('sess-1', 0, 'new:hash', 'hash', 'v', invoke);
    expect(invoke).toHaveBeenCalledWith(
      'redis',
      'hash_set',
      expect.objectContaining({ key: 'new:hash', field: 'field', value: 'v' }),
    );
  });

  it('creates list/set/zset with expected commands', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeCreateKey('s', 0, 'l', 'list', 'a', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'list_push', expect.objectContaining({ key: 'l' }));
    invoke.mockClear();
    await invokeCreateKey('s', 0, 's1', 'set', 'm', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_add', expect.objectContaining({ key: 's1' }));
    invoke.mockClear();
    await invokeCreateKey('s', 0, 'z', 'zset', 'm', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'zset_add', expect.objectContaining({ key: 'z' }));
  });

  it('creates ReJSON key via json_set with valid JSON', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeCreateKey('s', 0, 'j', 'ReJSON', '{"a":1}', invoke);
    expect(invoke).toHaveBeenCalledWith(
      'redis',
      'json_set',
      expect.objectContaining({ key: 'j', path: '$', value: '{"a":1}' }),
    );
  });

  it('wraps non-JSON ReJSON initial value as JSON string', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await invokeCreateKey('s', 0, 'j', 'ReJSON', 'plain', invoke);
    expect(invoke).toHaveBeenCalledWith(
      'redis',
      'json_set',
      expect.objectContaining({ value: '"plain"' }),
    );
  });

  it('rejects unsupported key types', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue(undefined);
    await expect(invokeCreateKey('s', 0, 'x', 'stream', '', invoke)).rejects.toThrow(
      /Unsupported key type/,
    );
  });
});
