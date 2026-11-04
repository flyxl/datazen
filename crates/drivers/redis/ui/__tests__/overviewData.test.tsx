import { describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import type { RedisInvokeFn } from '../shared/redisInvoke';
import { BIG_KEY_LIMIT, SLOWLOG_LIMIT } from '../overview/overviewModel';
import { OVERVIEW_COMMANDS, useOverviewData } from '../overview/useOverviewData';

/**
 * 屏 A 数据源契约（PRD §3.1 硬前提）。
 *
 * Two invariants the whole landing screen rests on:
 *
 * 1. **零 SCAN** — 屏 A issues exactly four driver commands
 *    (`info` / `db_sizes` / `memory_sample` / `slowlog_get`) and nothing else. No
 *    key-space traversal, no per-key round trip. That is what makes the overview
 *    safe as the default screen of a big database.
 * 2. **独立降级** — the two permission-gated sources (`redis:allow-memory-sample`,
 *    `redis:allow-slowlog-get`) fail on their own into a named 未授权 state, so a
 *    locked-down server still renders a complete screen (I-11) instead of an
 *    error page.
 *
 * Assertions use command tokens, statuses and parsed fields only — no rendered UI
 * copy is compared (PRD §7-6).
 */

const SESSION = 'session-1';

const INFO_REPLY = [
  '# Server',
  'redis_version:7.2.4',
  'mode:standalone',
  '# Memory',
  'used_memory:1048576',
  'used_memory_human:1.00M',
  'maxmemory:4194304',
  'maxmemory_human:4.00M',
  'mem_fragmentation_ratio:1.42',
  '# Replication',
  'role:master',
].join('\r\n');

const DB_SIZES_REPLY = [{ db: 0, keys: 7 }];
const MEMORY_REPLY = { samples: [{ key: 'blob:a', bytes: 4096 }], truncated: false };
const SLOWLOG_REPLY = [
  {
    id: 2,
    timestamp: 1700000000,
    durationUs: 12345,
    command: ['KEYS', '*'],
    clientAddr: '127.0.0.1:52114',
    clientName: '',
  },
];

function noperm(command: string): Error {
  return new Error(`NOPERM this user has no permissions to run the '${command}' command`);
}

type SourceKey = 'info' | 'dbSizes' | 'memory' | 'slowlog';

type SourcePlan = Partial<Record<SourceKey, unknown | Error>>;

const KEY_BY_COMMAND: Record<string, SourceKey> = {
  [OVERVIEW_COMMANDS.info]: 'info',
  [OVERVIEW_COMMANDS.dbSizes]: 'dbSizes',
  [OVERVIEW_COMMANDS.memorySample]: 'memory',
  [OVERVIEW_COMMANDS.slowlogGet]: 'slowlog',
};

const DEFAULT_REPLY: Record<SourceKey, unknown> = {
  info: INFO_REPLY,
  dbSizes: DB_SIZES_REPLY,
  memory: MEMORY_REPLY,
  slowlog: SLOWLOG_REPLY,
};

interface Gateway {
  invoke: RedisInvokeFn;
  calls: Array<{ command: string; args: Record<string, unknown> | undefined }>;
}

/**
 * An `invoke` seam that answers the four whitelisted commands from `plan`; an
 * unlisted command rejects, so a fifth call can never slip through unnoticed. A
 * key present in `plan` is used verbatim (including `null`), so an odd payload is
 * testable.
 */
function gateway(plan: SourcePlan = {}): Gateway {
  const calls: Gateway['calls'] = [];
  const invoke = vi.fn(
    async (_pluginId: string, command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      const key = KEY_BY_COMMAND[command];
      if (!key) throw new Error(`unexpected command ${command}`);
      const value = Object.prototype.hasOwnProperty.call(plan, key)
        ? plan[key]
        : DEFAULT_REPLY[key];
      if (value instanceof Error) return Promise.reject(value);
      return value;
    },
  ) as unknown as RedisInvokeFn;
  return { invoke, calls };
}

function issuedCommands(calls: Gateway['calls']): string[] {
  return [...new Set(calls.map((call) => call.command))].sort();
}

const WHITELIST = ['db_sizes', 'info', 'memory_sample', 'slowlog_get'].sort();

describe('useOverviewData · 零 SCAN 不变量', () => {
  it('issues exactly the four whitelisted commands, once each', async () => {
    const { invoke, calls } = gateway();
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 2, invoke }),
    );
    await waitFor(() => expect(calls).toHaveLength(4));

    expect(issuedCommands(calls)).toEqual(WHITELIST);
    await waitFor(() => expect(result.current.slowlog.status).toBe('ready'));
  });

  it('never reaches for a key-space traversal or a per-key round trip', async () => {
    const { invoke, calls } = gateway();
    renderHook(() => useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }));
    await waitFor(() => expect(calls).toHaveLength(4));

    const forbidden = [
      'scan_keys',
      'scan_values',
      'list_children',
      'get_key',
      'keys',
      'type',
      'ttl',
      'memory_usage',
      'object',
      'randomkey',
      'dbsize',
      'hgetall',
    ];
    for (const { command, args } of calls) {
      expect(forbidden).not.toContain(command);
      expect(command.toLowerCase()).not.toContain('scan');
      expect(command.toLowerCase()).not.toContain('children');
      // `memory_sample` samples server-side: no key list or glob may be sent.
      expect(args ?? {}).not.toHaveProperty('keys');
      expect(args ?? {}).not.toHaveProperty('pattern');
      expect(args ?? {}).not.toHaveProperty('cursor');
    }
  });

  it('passes the per-source args the backend commands expect', async () => {
    const { invoke, calls } = gateway();
    renderHook(() => useOverviewData({ dbSessionId: SESSION, dbIndex: 5, invoke }));
    await waitFor(() => expect(calls).toHaveLength(4));

    expect(invoke).toHaveBeenCalledWith('redis', 'info', {
      dbSessionId: SESSION,
      section: null,
    });
    expect(calls.find((call) => call.command === 'db_sizes')?.args).toEqual({
      dbSessionId: SESSION,
    });
    expect(calls.find((call) => call.command === 'memory_sample')?.args).toEqual({
      dbSessionId: SESSION,
      dbIndex: 5,
      limit: BIG_KEY_LIMIT,
    });
    expect(calls.find((call) => call.command === 'slowlog_get')?.args).toEqual({
      dbSessionId: SESSION,
      count: SLOWLOG_LIMIT,
    });
  });

  it('parses the INFO reply into the flat field table the cards derive from', async () => {
    const { invoke } = gateway();
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.info.status).toBe('ready'));

    expect(result.current.info.data?.fields.redis_version).toBe('7.2.4');
    expect(result.current.info.data?.fields.mem_fragmentation_ratio).toBe('1.42');
    expect(result.current.info.data?.sections.length).toBeGreaterThan(1);
    expect(result.current.dbSizes.data).toEqual(DB_SIZES_REPLY);
    expect(result.current.memory.data).toEqual(MEMORY_REPLY);
    expect(result.current.slowlog.data).toEqual(SLOWLOG_REPLY);
  });

  it('re-issues all four on refresh', async () => {
    const { invoke, calls } = gateway();
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(calls).toHaveLength(4));

    result.current.refresh();
    await waitFor(() => expect(calls).toHaveLength(8));
    expect(issuedCommands(calls)).toEqual(WHITELIST);
  });

  it('normalises empty or odd payloads into renderable data instead of a crash', async () => {
    const { invoke } = gateway({ dbSizes: [], memory: null, slowlog: null });
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.slowlog.status).toBe('ready'));

    expect(result.current.dbSizes.data).toEqual([]);
    expect(result.current.memory.data).toEqual({ samples: [], truncated: false });
    expect(result.current.slowlog.data).toEqual([]);
  });
});

describe('useOverviewData · 权限降级', () => {
  it('keeps 未授权 sources independent and drops the raw reply', async () => {
    const { invoke } = gateway({ memory: noperm('memory_sample') });
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.memory.status).toBe('unauthorized'));

    expect(result.current.memory.data).toBeNull();
    expect(result.current.memory.message).toBeNull();
    // The rest of 屏 A still renders: INFO / db_sizes / slowlog all succeeded.
    await waitFor(() => expect(result.current.info.status).toBe('ready'));
    expect(result.current.dbSizes.status).toBe('ready');
    expect(result.current.slowlog.status).toBe('ready');
  });

  it('reports a refused SLOWLOG GET as 未授权, not as a failure', async () => {
    const { invoke } = gateway({ slowlog: noperm('slowlog_get') });
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.slowlog.status).toBe('unauthorized'));
    expect(result.current.slowlog.message).toBeNull();
    expect(result.current.info.status).toBe('ready');
  });

  it('treats an unsupported command as 未授权 (older server / ACL without SLOWLOG)', async () => {
    const { invoke } = gateway({ slowlog: new Error('ERR unknown command SLOWLOG') });
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.slowlog.status).toBe('unauthorized'));
  });

  it('keeps the transport message for a genuine failure', async () => {
    const { invoke } = gateway({ info: new Error('connection reset by peer') });
    const { result } = renderHook(() =>
      useOverviewData({ dbSessionId: SESSION, dbIndex: 0, invoke }),
    );
    await waitFor(() => expect(result.current.info.status).toBe('failed'));
    expect(result.current.info.message).toContain('connection reset by peer');
    // An unrelated source is untouched by INFO blowing up.
    await waitFor(() => expect(result.current.dbSizes.status).toBe('ready'));
  });

  it('fails closed without issuing any command when there is no session', async () => {
    const { invoke, calls } = gateway();
    const { result } = renderHook(() => useOverviewData({ dbSessionId: '', dbIndex: 0, invoke }));
    await waitFor(() => expect(result.current.info.status).toBe('failed'));

    expect(calls).toEqual([]);
    expect(result.current.dbSizes.status).toBe('failed');
    expect(result.current.memory.status).toBe('failed');
    expect(result.current.slowlog.status).toBe('failed');
  });
});

describe('useOverviewData · session switch', () => {
  it('drops an in-flight reply from the previous session', async () => {
    const resolvers: Array<(value: unknown) => void> = [];
    const invoke = vi.fn(
      (_pluginId: string, command: string) =>
        new Promise<unknown>((resolve) => {
          if (command === OVERVIEW_COMMANDS.info) {
            resolvers.push(resolve);
          } else {
            resolve(undefined);
          }
        }),
    ) as unknown as RedisInvokeFn;

    const { rerender, result } = renderHook(
      ({ sessionId }: { sessionId: string }) =>
        useOverviewData({ dbSessionId: sessionId, dbIndex: 0, invoke }),
      { initialProps: { sessionId: 'old-session' } },
    );
    await waitFor(() => expect(resolvers.length).toBe(1));

    rerender({ sessionId: 'new-session' });
    await waitFor(() => expect(resolvers.length).toBe(2));

    // The old session's INFO lands *after* the switch; it must not paint 屏 A.
    resolvers[0](['# Server', 'redis_version:0.0.1'].join('\r\n'));
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current.info.status).toBe('loading');

    resolvers[1](INFO_REPLY);
    await waitFor(() => expect(result.current.info.status).toBe('ready'));
    expect(result.current.info.data?.fields.redis_version).toBe('7.2.4');
  });
});
