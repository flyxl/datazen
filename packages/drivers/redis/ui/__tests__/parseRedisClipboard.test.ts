import { describe, expect, it, vi } from 'vitest';
import {
  applyRedisClipboardToForm,
  clipboardHasRedisScheme,
  isPristineRedisForm,
  looksLikeRedisClipboard,
  parseRedisClipboard,
  parseRedisConnectionClipboard,
  type RedisClipboardForm,
} from '../parseRedisClipboard';

function stubForm(overrides: Partial<RedisClipboardForm> = {}): RedisClipboardForm {
  return {
    name: '',
    host: '127.0.0.1',
    port: '6379',
    database: '0',
    username: '',
    password: '',
    options: { topology: 'standalone' },
    setName: vi.fn(),
    setHost: vi.fn(),
    setPort: vi.fn(),
    setDatabase: vi.fn(),
    setUsername: vi.fn(),
    setPassword: vi.fn(),
    setOptions: vi.fn(),
    setSslMode: vi.fn(),
    setShowAdvanced: vi.fn(),
    ...overrides,
  };
}

describe('parseRedisClipboard', () => {
  it('parses redis:// with auth, db, and default port', () => {
    expect(parseRedisClipboard('redis://alice:s3cret@db.example.com/2')).toEqual({
      topology: 'standalone',
      host: 'db.example.com',
      port: '6379',
      database: '2',
      username: 'alice',
      password: 's3cret',
    });
  });

  it('parses rediss:// and query db', () => {
    expect(parseRedisClipboard('rediss://cache.internal:6380?db=4')).toEqual({
      topology: 'standalone',
      host: 'cache.internal',
      port: '6380',
      database: '4',
      tlsEnabled: true,
    });
  });

  it('parses password-only userinfo and REDIS_URL prefix', () => {
    expect(parseRedisClipboard('REDIS_URL=redis://:p%40ss@10.0.0.8:6379/0')).toEqual({
      topology: 'standalone',
      host: '10.0.0.8',
      port: '6379',
      database: '0',
      password: 'p@ss',
    });
  });

  it('parses comma-separated redis URL as cluster', () => {
    expect(parseRedisClipboard('redis://10.0.0.1:7000,10.0.0.2:7001')).toEqual({
      topology: 'cluster',
      host: '10.0.0.1',
      port: '7000',
      clusterNodes: ['10.0.0.1:7000', '10.0.0.2:7001'],
    });
  });

  it('parses multiple redis URLs as cluster', () => {
    const parsed = parseRedisClipboard('redis://a:7000\nredis://b:7001');
    expect(parsed?.topology).toBe('cluster');
    expect(parsed?.clusterNodes).toEqual(['a:7000', 'b:7001']);
  });

  it('parses redis-sentinel URL with master name and nodes', () => {
    expect(
      parseRedisClipboard('redis-sentinel://:sent@127.0.0.1:26379,127.0.0.1:26380/mymaster'),
    ).toEqual({
      topology: 'sentinel',
      host: '127.0.0.1',
      port: '26379',
      password: 'sent',
      sentinelNodes: ['127.0.0.1:26379', '127.0.0.1:26380'],
      sentinelMasterName: 'mymaster',
      sentinelNodePassword: 'sent',
    });
  });

  it('parses host:port, host:port:db, and host:port:password', () => {
    expect(parseRedisClipboard('10.1.2.3:6380')).toEqual({
      topology: 'standalone',
      host: '10.1.2.3',
      port: '6380',
    });
    expect(parseRedisClipboard('10.1.2.3:6379:2')).toEqual({
      topology: 'standalone',
      host: '10.1.2.3',
      port: '6379',
      database: '2',
    });
    expect(parseRedisClipboard('10.1.2.3:6379:secret')).toEqual({
      topology: 'standalone',
      host: '10.1.2.3',
      port: '6379',
      password: 'secret',
    });
    expect(parseRedisClipboard('10.1.2.3:6379:1:secret')).toEqual({
      topology: 'standalone',
      host: '10.1.2.3',
      port: '6379',
      database: '1',
      password: 'secret',
    });
  });

  it('parses IPv6 host:port', () => {
    expect(parseRedisClipboard('[2001:db8::1]:6379')).toEqual({
      topology: 'standalone',
      host: '[2001:db8::1]',
      port: '6379',
    });
  });

  it('treats a host:port list as cluster, and 26379 list as sentinel', () => {
    expect(parseRedisClipboard('10.0.0.1:7000\n10.0.0.2:7001')).toEqual({
      topology: 'cluster',
      host: '10.0.0.1',
      port: '7000',
      clusterNodes: ['10.0.0.1:7000', '10.0.0.2:7001'],
    });
    expect(parseRedisClipboard('10.0.0.1:26379,10.0.0.2:26379')).toEqual({
      topology: 'sentinel',
      host: '10.0.0.1',
      port: '26379',
      sentinelNodes: ['10.0.0.1:26379', '10.0.0.2:26379'],
    });
  });

  it('rejects unrelated clipboard text', () => {
    expect(parseRedisClipboard('')).toBeNull();
    expect(parseRedisClipboard('hello world')).toBeNull();
    expect(parseRedisClipboard('postgres://db:5432/app')).toBeNull();
    expect(parseRedisClipboard('localhost')).toBeNull();
  });
});

describe('redis clipboard helpers', () => {
  it('detects redis URL schemes', () => {
    expect(clipboardHasRedisScheme('redis://localhost:6379')).toBe(true);
    expect(clipboardHasRedisScheme('10.0.0.1:6379')).toBe(false);
  });

  it('auto-applies redis URLs, cluster lists, and common redis ports', () => {
    const url = parseRedisClipboard('redis://prod:6380')!;
    expect(looksLikeRedisClipboard('redis://prod:6380', url)).toBe(true);
    const custom = parseRedisClipboard('db.internal:9000')!;
    expect(looksLikeRedisClipboard('db.internal:9000', custom)).toBe(false);
    const common = parseRedisClipboard('db.internal:6379')!;
    expect(looksLikeRedisClipboard('db.internal:6379', common)).toBe(true);
  });

  it('treats a new redis form as pristine and filled ones as not', () => {
    expect(isPristineRedisForm(stubForm())).toBe(true);
    expect(isPristineRedisForm(stubForm({ name: 'prod' }))).toBe(false);
    expect(isPristineRedisForm(stubForm({ host: '10.0.0.1' }))).toBe(false);
    expect(isPristineRedisForm(stubForm({ options: { topology: 'cluster' } }))).toBe(false);
  });

  it('applies parsed fields onto the connection form', () => {
    const form = stubForm();
    applyRedisClipboardToForm(
      form,
      parseRedisClipboard('rediss://alice:s3cret@cache.internal:6380/3')!,
    );
    expect(form.setHost).toHaveBeenCalledWith('cache.internal');
    expect(form.setPort).toHaveBeenCalledWith('6380');
    expect(form.setDatabase).toHaveBeenCalledWith('3');
    expect(form.setUsername).toHaveBeenCalledWith('alice');
    expect(form.setPassword).toHaveBeenCalledWith('s3cret');
    expect(form.setName).toHaveBeenCalledWith('cache.internal:6380');
    expect(form.setSslMode).toHaveBeenCalledWith('require');
    expect(form.setShowAdvanced).toHaveBeenCalledWith(true);
    expect(form.setOptions).toHaveBeenCalledWith(
      expect.objectContaining({
        topology: 'standalone',
        tls: { enabled: true },
      }),
    );
  });

  it('exposes a host clipboard parser that only auto-matches redis-like text', () => {
    expect(parseRedisConnectionClipboard('postgres://db:5432/app')).toBeNull();
    expect(parseRedisConnectionClipboard('db.internal:9000')).toBeNull();
    expect(parseRedisConnectionClipboard('rediss://cache:6380')).toEqual(
      expect.objectContaining({
        host: 'cache',
        port: '6380',
        sslMode: 'require',
        expandAdvanced: true,
      }),
    );
  });

  it('parses quoted env URLs, username-only auth, query params, and redis+tls', () => {
    expect(parseRedisClipboard('"export REDIS_URL=redis://alice@host:6379"')).toEqual(
      expect.objectContaining({ host: 'host', port: '6379', username: 'alice' }),
    );
    expect(
      parseRedisClipboard('redis://cache:6379?username=bob&password=s3cret&db=3'),
    ).toEqual(
      expect.objectContaining({
        host: 'cache',
        username: 'bob',
        password: 's3cret',
        database: '3',
      }),
    );
    expect(parseRedisClipboard('redis+tls://[::1]:6380')).toEqual(
      expect.objectContaining({
        host: '[::1]',
        port: '6380',
        tlsEnabled: true,
      }),
    );
    expect(
      parseRedisClipboard(
        'sentinel://127.0.0.1:26379/mymaster?sentinelPassword=sent&password=redis',
      ),
    ).toEqual(
      expect.objectContaining({
        topology: 'sentinel',
        sentinelMasterName: 'mymaster',
        sentinelNodePassword: 'sent',
        password: 'redis',
      }),
    );
  });

  it('merges mixed URL lists and keeps a sentinel URL when present', () => {
    expect(
      parseRedisClipboard('redis://a:7000\nredis-sentinel://127.0.0.1:26379/mymaster'),
    ).toEqual(
      expect.objectContaining({
        topology: 'sentinel',
        sentinelMasterName: 'mymaster',
      }),
    );
    expect(parseRedisClipboard('redis://a:7000,a:7000\nredis://b:7001')?.clusterNodes).toEqual([
      'a:7000',
      'b:7001',
    ]);
  });

  it('rejects malformed host:port tokens and invalid IPv6', () => {
    expect(parseRedisClipboard('10.0.0.1:7000:db:secret')).toBeNull();
    expect(parseRedisClipboard('[::1')).toBeNull();
    expect(parseRedisClipboard('[::1]6379')).toBeNull();
    expect(parseRedisClipboard('[::1]:abc')).toBeNull();
    expect(parseRedisClipboard('host:0')).toBeNull();
    expect(parseRedisClipboard('redis://')).toBeNull();
  });

  it('keeps invalid percent-encoding in passwords', () => {
    expect(parseRedisClipboard('redis://:p%@host:6379')?.password).toBe('p%');
  });

  it('applies cluster and sentinel options without TLS and preserves an existing name', () => {
    const clusterForm = stubForm({ name: 'keep' });
    applyRedisClipboardToForm(clusterForm, parseRedisClipboard('10.0.0.1:7000\n10.0.0.2:7001')!);
    expect(clusterForm.setOptions).toHaveBeenCalledWith(
      expect.objectContaining({
        topology: 'cluster',
        clusterNodes: ['10.0.0.1:7000', '10.0.0.2:7001'],
      }),
    );
    expect(clusterForm.setName).not.toHaveBeenCalled();
    expect(clusterForm.setShowAdvanced).not.toHaveBeenCalled();

    const sentinelForm = stubForm();
    applyRedisClipboardToForm(
      sentinelForm,
      parseRedisClipboard('redis-sentinel://:sent@127.0.0.1:26379/mymaster')!,
    );
    expect(sentinelForm.setOptions).toHaveBeenCalledWith(
      expect.objectContaining({
        topology: 'sentinel',
        sentinelMasterName: 'mymaster',
        sentinelNodePassword: 'sent',
      }),
    );
    expect(sentinelForm.setName).toHaveBeenCalledWith('mymaster');
  });

  it('treats localhost defaults as pristine and other fields as dirty', () => {
    expect(isPristineRedisForm(stubForm({ host: 'localhost', port: '', database: '' }))).toBe(
      true,
    );
    expect(isPristineRedisForm(stubForm({ username: 'alice' }))).toBe(false);
    expect(isPristineRedisForm(stubForm({ password: 'x' }))).toBe(false);
    expect(isPristineRedisForm(stubForm({ database: '1' }))).toBe(false);
    expect(isPristineRedisForm(stubForm({ port: '6380' }))).toBe(false);
    expect(
      isPristineRedisForm(
        stubForm({ options: { topology: 'standalone', clusterNodes: ['a:1'] } }),
      ),
    ).toBe(false);
    expect(
      isPristineRedisForm(
        stubForm({ options: { topology: 'standalone', sentinelNodes: ['a:26379'] } }),
      ),
    ).toBe(false);
  });

  it('maps cluster/sentinel clipboard into the host fill shape', () => {
    expect(parseRedisConnectionClipboard('10.0.0.1:7000\n10.0.0.2:7001')).toEqual(
      expect.objectContaining({
        options: expect.objectContaining({
          topology: 'cluster',
          clusterNodes: ['10.0.0.1:7000', '10.0.0.2:7001'],
        }),
      }),
    );
    expect(
      parseRedisConnectionClipboard('redis-sentinel://127.0.0.1:26379/mymaster'),
    ).toEqual(
      expect.objectContaining({
        name: 'mymaster',
        options: expect.objectContaining({
          topology: 'sentinel',
          sentinelMasterName: 'mymaster',
        }),
      }),
    );
  });
});
