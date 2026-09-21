import { describe, expect, it, vi } from 'vitest';
import {
  invokeCreateKey,
  invokeSetExpireAt,
  invokeSetString,
  invokeSetTtl,
  invokeHashScan,
  invokeListRange,
  invokeSetScan,
  invokeZsetScan,
  type PluginInvokeFn,
} from '../value-editors/keyEditorsInvokes';

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
    expect(invoke).toHaveBeenCalledWith(
      'redis',
      'list_push',
      expect.objectContaining({ key: 'l' }),
    );
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

// ---- PR-3: Collection editors invoke functions (tester) ----

describe('invokeHashScan (PR-3)', () => {
  it('sends hash_scan command with cursor and count', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({
      cursor: 0,
      entries: [{ field: 'f1', value: 'v1' }],
    });
    const result = await invokeHashScan('sess-1', 0, 'myhash', 0, 100, undefined, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'hash_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'myhash',
      cursor: 0,
      count: 100,
    });
    expect(result.cursor).toBe(0);
    expect(result.entries).toHaveLength(1);
    expect(result.entries[0]).toEqual({ field: 'f1', value: 'v1' });
  });

  it('includes matchPattern when provided', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ cursor: 0, entries: [] });
    await invokeHashScan('sess-1', 0, 'h', 10, 50, 'f*', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'hash_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'h',
      cursor: 10,
      count: 50,
      matchPattern: 'f*',
    });
  });

  it('omits matchPattern when undefined', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ cursor: 0, entries: [] });
    await invokeHashScan('sess-1', 0, 'h', 0, 100, undefined, invoke);
    const args = invoke.mock.calls[0][2] as Record<string, unknown>;
    expect(args).not.toHaveProperty('matchPattern');
  });
});

describe('invokeListRange (PR-3)', () => {
  it('sends list_range with start and stop', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ items: ['a', 'b', 'c'] });
    const result = await invokeListRange('sess-1', 2, 'mylist', 0, 99, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'list_range', {
      dbSessionId: 'sess-1',
      dbIndex: 2,
      key: 'mylist',
      start: 0,
      stop: 99,
    });
    expect(result.items).toEqual(['a', 'b', 'c']);
  });

  it('returns empty items for empty range', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ items: [] });
    const result = await invokeListRange('sess-1', 0, 'empty-list', 0, -1, invoke);
    expect(result.items).toEqual([]);
  });
});

describe('invokeSetScan (PR-3)', () => {
  it('sends set_scan command with cursor and count', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({
      cursor: 42,
      members: ['m1', 'm2'],
    });
    const result = await invokeSetScan('sess-1', 0, 'myset', 0, 100, undefined, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'myset',
      cursor: 0,
      count: 100,
    });
    expect(result.cursor).toBe(42);
    expect(result.members).toEqual(['m1', 'm2']);
  });

  it('includes matchPattern when provided', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ cursor: 0, members: [] });
    await invokeSetScan('sess-1', 0, 's', 5, 25, 'abc*', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'set_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 's',
      cursor: 5,
      count: 25,
      matchPattern: 'abc*',
    });
  });
});

describe('invokeZsetScan (PR-3)', () => {
  it('sends zset_scan command with cursor and count', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({
      cursor: 0,
      members: [{ member: 'm1', score: 1.5 }],
    });
    const result = await invokeZsetScan('sess-1', 0, 'myzset', 0, 100, undefined, invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'zset_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'myzset',
      cursor: 0,
      count: 100,
    });
    expect(result.members).toHaveLength(1);
    expect(result.members[0]).toEqual({ member: 'm1', score: 1.5 });
  });

  it('includes matchPattern when provided', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ cursor: 0, members: [] });
    await invokeZsetScan('sess-1', 0, 'z', 0, 100, 'prefix:*', invoke);
    expect(invoke).toHaveBeenCalledWith('redis', 'zset_scan', {
      dbSessionId: 'sess-1',
      dbIndex: 0,
      key: 'z',
      cursor: 0,
      count: 100,
      matchPattern: 'prefix:*',
    });
  });

  it('omits matchPattern when undefined', async () => {
    const invoke = vi.fn<PluginInvokeFn>().mockResolvedValue({ cursor: 0, members: [] });
    await invokeZsetScan('sess-1', 0, 'z', 0, 100, undefined, invoke);
    const args = invoke.mock.calls[0][2] as Record<string, unknown>;
    expect(args).not.toHaveProperty('matchPattern');
  });
});
