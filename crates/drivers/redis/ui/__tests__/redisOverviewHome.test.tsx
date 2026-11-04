import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { BROWSE_HISTORY_STORAGE_KEY, pushBrowseEntry } from '../lib/redisBrowseHistory';

/**
 * 屏 A — Redis 连接总览组件测试（PRD §3.1 七个区块逐行）.
 *
 * `useI18n` is overridden so `t(key)` returns the **key**: every text assertion
 * below compares an i18n key or a server token (db name, key name, command line,
 * command token), never rendered English copy. Structural assertions go through
 * the stable `data-overview-*` attributes the components expose for exactly this
 * purpose (PRD §7-6).
 */
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const redisInvoke = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) => redisInvoke(...args),
}));

import { RedisOverviewHome, type RedisOverviewHomeProps } from '../overview/RedisOverviewHome';

const SESSION = 'sess-1';
const CONNECTION = 'conn-1';

const INFO_OK = [
  '# Server',
  'redis_version:7.2.4',
  'arch_bits:64',
  'os:Linux 6.1 x86_64',
  'uptime_in_days:12',
  '# Clients',
  'connected_clients:9',
  'blocked_clients:0',
  '# Memory',
  'used_memory:1048576',
  'used_memory_human:1.00M',
  'maxmemory:4194304',
  'maxmemory_human:4.00M',
  'maxmemory_policy:allkeys-lru',
  'mem_fragmentation_ratio:1.42',
  '# Stats',
  'instantaneous_ops_per_sec:250',
  'total_commands_processed:98765',
  'evicted_keys:0',
  'expired_keys:41',
  '# Replication',
  'role:master',
  'mode:standalone',
].join('\r\n');

const DB_SIZES_OK = [
  { db: 0, keys: 3 },
  { db: 2, keys: 1 },
];

const MEMORY_OK = {
  samples: [
    { key: 'cache:small', bytes: 1024, type: 'string', ttlMs: 60_000, missing: false },
    // rank 1: no-expiry string
    { key: 'blob:a', bytes: 5 * 1024 * 1024, type: 'string', ttlMs: -1, missing: false },
    // rank 2: hash with a live ttl (jump must forward this type — BUG-001)
    { key: 'blob:b', bytes: 4 * 1024 * 1024, type: 'hash', ttlMs: 8_000, missing: false },
    // rank 3: TTL reply unreadable → null
    { key: 'blob:c', bytes: 3 * 1024 * 1024, type: 'list', ttlMs: null, missing: false },
    // rank 4: deleted between SCAN and field read → distinguishable empty state
    { key: 'blob:d', bytes: 2 * 1024 * 1024, type: null, ttlMs: -2, missing: true },
    // rank 5: type unreadable → unknown badge
    { key: 'blob:e', bytes: 1 * 1024 * 1024, type: null, ttlMs: 30_000, missing: false },
  ],
  truncated: false,
};

const SLOWLOG_OK = [
  {
    id: 1,
    timestamp: 1700000000,
    durationUs: 500,
    command: ['GET', 'user:1'],
    clientAddr: '127.0.0.1:52114',
    clientName: 'worker',
  },
  {
    id: 2,
    timestamp: 1700000001,
    durationUs: 12345,
    command: ['KEYS', '*'],
    clientAddr: '',
    clientName: '',
  },
];

function stub(overrides: Record<string, unknown> = {}) {
  redisInvoke.mockImplementation(async (_pluginId: string, command: string) => {
    if (Object.prototype.hasOwnProperty.call(overrides, command)) {
      const value = overrides[command];
      if (value instanceof Error) throw value;
      return value;
    }
    switch (command) {
      case 'info':
        return INFO_OK;
      case 'db_sizes':
        return DB_SIZES_OK;
      case 'memory_sample':
        return MEMORY_OK;
      case 'slowlog_get':
        return SLOWLOG_OK;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

function baseProps(overrides: Partial<RedisOverviewHomeProps> = {}): RedisOverviewHomeProps {
  return {
    connectionId: CONNECTION,
    dbSessionId: SESSION,
    connectionName: 'local-redis',
    databaseType: 'redis',
    ...overrides,
  } as unknown as RedisOverviewHomeProps;
}

function attr(root: HTMLElement, selector: string, name: string): string | null {
  return root.querySelector(selector)?.getAttribute(name) ?? null;
}

function ids(root: HTMLElement, selector: string): string[] {
  // The attributes read through this helper are always rendered by the component
  // under test, so the cast keeps the expectation lists plain `string[]`
  // instead of widening every call site to `string | null`.
  return [...root.querySelectorAll(`[${selector}]`)].map(
    (el) => el.getAttribute(selector) as string,
  );
}

function cardState(root: HTMLElement, card: string): string | null {
  return attr(root, `[data-overview-card="${card}"]`, 'data-overview-card-state');
}

function pillValue(root: HTMLElement, pill: string): string | null | undefined {
  return root.querySelector(`[data-overview-pill-value="${pill}"]`)?.textContent;
}

function issuedCommands(): string[] {
  return [...new Set(redisInvoke.mock.calls.map((call) => String(call[1])))].sort();
}

beforeEach(() => {
  cleanup();
  redisInvoke.mockReset();
  stub();
  globalThis.localStorage.clear();
});

describe('屏 A 组装', () => {
  it('renders the banner plus three cards in a two-row layout', () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);

    expect(container.querySelector('[data-overview-banner]')).not.toBeNull();
    expect(ids(container, 'data-overview-card').sort()).toEqual([
      'actions',
      'performance',
      'server',
    ]);

    const grid = container.querySelector('[data-overview-grid]');
    expect(grid).not.toBeNull();
    // InstanceCard is a direct child of grid; PerformanceCard + NavigationCard
    // share a side-by-side grid wrapper as the second row.
    const serverCard = container.querySelector('[data-overview-card="server"]');
    expect(serverCard?.parentElement).toBe(grid);
    const slowlogCard = container.querySelector('[data-overview-card="performance"]');
    expect(slowlogCard?.parentElement).not.toBe(grid); // inside 2-col wrapper
    const actionsCard = container.querySelector('[data-overview-card="actions"]');
    expect(actionsCard?.parentElement).not.toBe(grid); // inside 2-col wrapper
    // The two-col wrapper is the second child of grid.
    const secondRow = grid?.children[1];
    expect(secondRow?.classList.contains('grid-cols-2')).toBe(true);
    expect(secondRow?.contains(slowlogCard)).toBe(true);
    expect(secondRow?.contains(actionsCard)).toBe(true);
  });

  it('sources the whole screen through the four whitelisted commands only', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'performance')).toBe('ready'));

    expect(issuedCommands()).toEqual(['db_sizes', 'info', 'memory_sample', 'slowlog_get']);
    expect(redisInvoke).toHaveBeenCalledTimes(4);
    for (const call of redisInvoke.mock.calls) {
      expect(call[0]).toBe('redis');
      expect(String(call[1]).toLowerCase()).not.toContain('scan');
    }
  });

  it('binds every block to the command it reads, so 区块空态有名字', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'performance')).toBe('ready'));

    expect(container.querySelector('[data-overview-card-source="server"]')?.textContent).toBe(
      'info',
    );
    expect(container.querySelector('[data-overview-card-source="performance"]')?.textContent).toBe(
      'slowlog_get + memory_sample',
    );
  });

  it('keeps loading state named instead of blank', () => {
    stub({ info: new Promise(() => {}) });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);

    expect(cardState(container, 'server')).toBe('loading');
    expect(container.querySelector('[data-overview-loading="server"]')).not.toBeNull();
  });
});

describe('横幅（区块 1）', () => {
  it('is 56px tall and shows three read-only pills', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    const banner = container.querySelector('[data-overview-banner]');
    expect(banner?.classList.contains('h-12')).toBe(true);

    await waitFor(() => expect(pillValue(container, 'version')).toBe('7.2.4'));
    expect(ids(container, 'data-overview-pill')).toEqual(['version', 'mode', 'usedMemory']);
  });

  it('renders pill values from INFO output', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(pillValue(container, 'version')).toBe('7.2.4'));
    expect(pillValue(container, 'mode')).toBe('standalone');
    expect(pillValue(container, 'usedMemory')).toBeTruthy();
  });
});

describe('卡 1 — Server 概览', () => {
  it('renders the ten PRD rows in order, two columns', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    expect(ids(container, 'data-overview-server-row')).toEqual([
      'version',
      'mode',
      'arch',
      'uptime',
      'connectedClients',
      'blockedClients',
      'opsPerSec',
      'totalCommands',
      'evictedKeys',
      'expiredKeys',
    ]);
    const rows = container.querySelector('[data-overview-card="server"] dl');
    // In InstanceCard the server/memory side-by-side uses sm:grid-cols-2 on the parent div;
    // the dl itself is a single-column list within its half.
    expect(rows?.classList.contains('grid-cols-1')).toBe(true);
  });

  it('keeps server tokens raw and maps the known mode values to their key', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    expect(container.querySelector('[data-overview-server-value="version"]')?.textContent).toBe(
      '7.2.4',
    );
    expect(container.querySelector('[data-overview-server-value="mode"]')?.textContent).toBe(
      'redis.overview.mode.standalone',
    );
    expect(container.querySelector('[data-overview-server-value="arch"]')?.textContent).toBe(
      'redis.overview.unit.bits',
    );
  });

  it('colours evicted_keys > 0 and leaves a clean server neutral', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));
    expect(attr(container, '[data-overview-server-row="evictedKeys"]', 'data-overview-warn')).toBe(
      'false',
    );
    cleanup();

    stub({
      info: INFO_OK.replace('evicted_keys:0', 'evicted_keys:18').replace(
        'mode:standalone',
        'mode:cluster',
      ),
    });
    const second = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(second.container, 'server')).toBe('ready'));
    expect(
      attr(second.container, '[data-overview-server-row="evictedKeys"]', 'data-overview-warn'),
    ).toBe('true');
    expect(second.container.querySelector('[data-overview-server-value="mode"]')?.textContent).toBe(
      'redis.overview.mode.cluster',
    );
  });

  it('shows a dash for a field the server did not report, not a hole', async () => {
    stub({ info: '# Server\r\nredis_version:7.2.4\r\n' });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    expect(attr(container, '[data-overview-server-row="uptime"]', 'data-overview-server-row')).toBe(
      'uptime',
    );
    expect(container.querySelector('[data-overview-server-value="uptime"]')?.textContent).toBe('—');
  });

  it('falls back to the named empty state when INFO carries nothing renderable', async () => {
    stub({ info: '# Server\r\n' });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('empty'));
    expect(container.querySelector('[data-overview-empty="server"]')).not.toBeNull();
  });
});

describe('内存 gauge（在 InstanceCard 内）', () => {
  it('draws the used/max gauge from INFO and flags fragmentation above 1.5', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(
        attr(container, '[data-overview-memory-bar]', 'data-overview-memory-bar-percent'),
      ).toBe('25'),
    );

    expect(attr(container, '[data-overview-memory-bar]', 'data-overview-memory-bar-state')).toBe(
      'bounded',
    );
    expect(container.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe(
      '25',
    );
    expect(attr(container, '[data-overview-memory-frag]', 'data-overview-memory-frag')).toBe('ok');
    cleanup();

    stub({ info: INFO_OK.replace('mem_fragmentation_ratio:1.42', 'mem_fragmentation_ratio:2.9') });
    const second = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(
        attr(second.container, '[data-overview-memory-frag]', 'data-overview-memory-frag'),
      ).toBe('warn'),
    );
  });

  it('names the unlimited-maxmemory case instead of dividing by zero', async () => {
    stub({
      info: INFO_OK.replace('maxmemory:4194304', 'maxmemory:0').replace(
        'maxmemory_human:4.00M',
        '',
      ),
    });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(attr(container, '[data-overview-memory-bar]', 'data-overview-memory-bar-state')).toBe(
        'unlimited',
      ),
    );

    expect(attr(container, '[data-overview-memory-max]', 'data-overview-memory-max')).toBe(
      'unlimited',
    );
    expect(attr(container, '[data-overview-memory-bar]', 'data-overview-memory-bar-percent')).toBe(
      '0',
    );
    // 无上限时不显示百分比，具名破折号占位而不是 0%
    expect(container.querySelector('[data-overview-memory-percent]')?.textContent).toBe('—');
  });

  it('lists the memory_sample Top 3 ranked by size, and says when the sample was truncated', async () => {
    stub({ memory_sample: { samples: MEMORY_OK.samples, truncated: true } });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-bigkey]').length).toBe(3),
    );

    const ranks = ids(container, 'data-overview-bigkey');
    expect(ranks).toEqual(['1', '2', '3']);
    expect(attr(container, '[data-overview-bigkey="1"]', 'data-overview-key')).toBe('blob:a');
    // BUG-003: the type / TTL columns come from the same memory_sample reply.
    expect(attr(container, '[data-overview-bigkey="1"]', 'data-overview-bigkey-type')).toBe(
      'string',
    );
    expect(attr(container, '[data-overview-bigkey="1"]', 'data-overview-bigkey-ttl-ms')).toBe('-1');
    expect(attr(container, '[data-overview-bigkey="2"]', 'data-overview-bigkey-type')).toBe('hash');
    expect(attr(container, '[data-overview-bigkey="2"]', 'data-overview-bigkey-ttl-ms')).toBe(
      '8000',
    );
    // rank 3 TTL null → dash.
    expect(attr(container, '[data-overview-bigkey="3"]', 'data-overview-bigkey-ttl-ms')).toBe('');
    // The type badge is a *visible* cell, not only a data attribute.
    expect(container.querySelector('[data-overview-bigkey="2"]')?.textContent).toContain('hash');
    expect(container.querySelector('[data-overview-bigkey-truncated]')).not.toBeNull();
    expect(container.querySelector('[data-overview-memory-sample-db]')).not.toBeNull();
  });

  it('shows the named 未授权 state when redis:allow-memory-sample is missing, gauge intact', async () => {
    stub({
      memory_sample: new Error(
        "NOPERM this user has no permissions to run the 'memory_sample' command",
      ),
    });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(attr(container, '[data-overview-bigkey-state]', 'data-overview-bigkey-state')).toBe(
        'unauthorized',
      ),
    );

    expect(container.querySelector('[data-overview-memory-gauge]')).not.toBeNull();
    expect(cardState(container, 'server')).toBe('ready');
    expect(container.querySelector('[data-overview-failed="server"]')).toBeNull();
    expect(container.querySelector('[data-overview-bigkey]')).toBeNull();
  });

  it('names the empty sample and the failure separately', async () => {
    stub({ memory_sample: { samples: [], truncated: false } });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(attr(container, '[data-overview-bigkey-state]', 'data-overview-bigkey-state')).toBe(
        'empty',
      ),
    );
    cleanup();

    stub({ memory_sample: new Error('READONLY You cant write against a read only replica') });
    const second = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(
        attr(second.container, '[data-overview-bigkey-state]', 'data-overview-bigkey-state'),
      ).toBe('failed'),
    );
  });

  it('samples the connections configured database, not a hard-coded db0', async () => {
    render(<RedisOverviewHome {...baseProps({ initialDatabase: 'db3' })} />);
    await waitFor(() => expect(redisInvoke).toHaveBeenCalledTimes(4));

    const sample = redisInvoke.mock.calls.find((call) => call[1] === 'memory_sample');
    expect(sample?.[2]).toMatchObject({ dbSessionId: SESSION, dbIndex: 3 });
  });
});

describe('卡 4 — 慢查询', () => {
  it('shows the Top 3 as 序号 / 耗时 µs / 命令摘要 / 客户端', async () => {
    stub({
      slowlog_get: [
        ...SLOWLOG_OK,
        {
          id: 3,
          timestamp: 1700000002,
          durationUs: 700,
          command: ['SET', 'k', 'v'],
          clientAddr: '10.0.0.9:400',
          clientName: '',
        },
        {
          id: 4,
          timestamp: 1700000003,
          durationUs: 800,
          command: ['LPUSH', 'q'],
          clientAddr: '10.0.0.10:401',
          clientName: '',
        },
        {
          id: 5,
          timestamp: 1700000004,
          durationUs: 900,
          command: ['SMEMBERS', 's'],
          clientAddr: '10.0.0.11:402',
          clientName: '',
        },
        {
          id: 6,
          timestamp: 1700000005,
          durationUs: 950,
          command: ['HGETALL', 'h'],
          clientAddr: '10.0.0.12:403',
          clientName: '',
        },
      ],
    });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-slowlog-row]').length).toBe(3),
    );

    expect(ids(container, 'data-overview-slowlog-row')).toEqual(['1', '2', '3']);
    // newest id first — Redis orders oldest-first, the card must not mirror that.
    expect(attr(container, '[data-overview-slowlog-row="1"]', 'data-overview-slowlog-row')).toBe(
      '1',
    );
    expect(
      attr(container, '[data-overview-slowlog-duration="1"]', 'data-overview-duration-us'),
    ).toBe('950');
    expect(container.querySelector('[data-overview-slowlog-command="1"]')?.textContent).toBe(
      'HGETALL h',
    );
    expect(container.querySelector('[data-overview-slowlog-client="1"]')?.textContent).toBe(
      '10.0.0.12:403',
    );
    // entry 2 is the id-5 SMEMBERS row
    expect(
      container
        .querySelector('[data-overview-slowlog-duration="2"]')
        ?.getAttribute('data-overview-duration-us'),
    ).toBe('900');
  });

  it('names a missing client rather than leaving the cell blank', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-slowlog-row]').length).toBe(2),
    );

    expect(attr(container, '[data-overview-slowlog-row="2"]', 'data-overview-slowlog-row')).toBe(
      '2',
    );
    // rank 1 是 id=2 那条（clientAddr / clientName 皆空），rank 2 才是带客户端的那条
    expect(
      attr(container, '[data-overview-slowlog-client="1"]', 'data-overview-client-state'),
    ).toBe('unknown');
    expect(
      attr(container, '[data-overview-slowlog-client="2"]', 'data-overview-client-state'),
    ).toBe('known');
    // 客户端名与地址拼在一行
    expect(container.querySelector('[data-overview-slowlog-client="2"]')?.textContent).toContain(
      'worker',
    );
  });

  it('explains an empty SLOWLOG with the SLOWLOG GET semantics (I-11)', async () => {
    stub({ slowlog_get: [], memory_sample: { samples: [], truncated: false } });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'performance')).toBe('empty'));
    expect(container.querySelector('[data-overview-empty="performance"]')).not.toBeNull();
    expect(container.querySelector('[data-overview-slowlog-row]')).toBeNull();
  });

  it('degrades a refused SLOWLOG GET into the named 未授权 state, not an error', async () => {
    stub({
      slowlog_get: new Error('NOPERM this user has no permissions'),
      memory_sample: { samples: [], truncated: false },
    });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    // Combined PerformanceCard shows slowlog unauthorized at section level
    await waitFor(() =>
      expect(
        container.querySelector('[data-overview-slowlog-state="unauthorized"]'),
      ).not.toBeNull(),
    );

    expect(container.querySelector('[data-overview-failed="slowlog"]')).toBeNull();
    // 其余区块照常
    expect(cardState(container, 'server')).toBe('ready');
  });
});

describe('区块 5 — KV 快捷动作', () => {
  it('offers key-value entries and none of the SQL ones', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    expect(ids(container, 'data-overview-action').sort()).toEqual(
      ['browseDb', 'console', 'importExport', 'newKey', 'pubsub'].sort(),
    );
    for (const sqlAction of ['newQuery', 'newTable', 'erDiagram', 'runQuery']) {
      expect(container.querySelector(`[data-overview-action="${sqlAction}"]`)).toBeNull();
    }
  });

  it('sends browse-db to the connections configured database, db0 by default', async () => {
    const onOpenTarget = vi.fn();
    const { container } = render(<RedisOverviewHome {...baseProps({ onOpenTarget })} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-action]').length).toBe(5),
    );

    fireEvent.click(container.querySelector('[data-overview-action="browseDb"]') as Element);
    expect(onOpenTarget).toHaveBeenLastCalledWith({ kind: 'database', dbIndex: 0 });
    expect(
      attr(container, '[data-overview-action="browseDb"]', 'data-overview-action-target'),
    ).toBe('database');

    cleanup();
    const second = render(
      <RedisOverviewHome {...baseProps({ onOpenTarget, initialDatabase: 'db5' })} />,
    );
    await waitFor(() =>
      expect(second.container.querySelectorAll('[data-overview-action]').length).toBe(5),
    );
    fireEvent.click(second.container.querySelector('[data-overview-action="newKey"]') as Element);
    expect(onOpenTarget).toHaveBeenLastCalledWith({ kind: 'newKey', dbIndex: 5 });
  });
});

describe('区块 6 — 最近浏览键', () => {
  it('names the empty history and says what fills it', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'actions')).toBe('ready'));
    // No recent entries → empty state text shown
    expect(container.querySelector('[data-overview-recent-key]')).toBeNull();
    expect(container.querySelector('[data-overview-recent-clear]')).toBeNull();
  });

  it('lists the connections own entries, newest first, with a relative unit key', async () => {
    // 组件按真实时间渲染，所以基准取 Date.now()，只断言单位 key + 数量级
    const now = Date.now();
    pushBrowseEntry(
      CONNECTION,
      { key: 'user:1', dbIndex: 0, keyType: 'string' },
      undefined,
      now - 3 * 3_600_000,
    );
    pushBrowseEntry(
      CONNECTION,
      { key: 'queue:jobs', dbIndex: 4, keyType: null },
      undefined,
      now - 5 * 60_000,
    );
    pushBrowseEntry('other-connection', { key: 'secret', dbIndex: 0 }, undefined, now);

    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-recent-key]').length).toBe(2),
    );

    expect(ids(container, 'data-overview-recent-key')).toEqual(['queue:jobs', 'user:1']);
    expect(attr(container, '[data-overview-recent-key="user:1"]', 'data-overview-db-index')).toBe(
      '0',
    );
    expect(attr(container, '[data-overview-recent-key="user:1"]', 'data-overview-key-type')).toBe(
      'string',
    );
    expect(
      attr(container, '[data-overview-recent-key="queue:jobs"]', 'data-overview-key-type'),
    ).toBe('unknown');
    expect(
      attr(container, '[data-overview-recent-time="user:1"]', 'data-overview-relative-unit'),
    ).toBe('redis.overview.timeAgo.hours');
    expect(
      attr(container, '[data-overview-recent-time="user:1"]', 'data-overview-relative-value'),
    ).toBe('3');
    expect(
      attr(container, '[data-overview-recent-time="queue:jobs"]', 'data-overview-relative-unit'),
    ).toBe('redis.overview.timeAgo.minutes');
    expect(container.querySelector('[data-overview-recent-key="secret"]')).toBeNull();
  });

  it('writes history only for a jump that actually landed', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    // 无桥接 ⇒ 不谎报到达，也不写历史 (click an unwired big key)
    const bigKeyEl = container.querySelector('[data-overview-bigkey]');
    if (bigKeyEl) {
      fireEvent.click(bigKeyEl);
      expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBeNull();
    }

    cleanup();
    const onOpenTarget = vi.fn();
    const wired = render(<RedisOverviewHome {...baseProps({ onOpenTarget })} />);
    await waitFor(() =>
      expect(wired.container.querySelectorAll('[data-overview-bigkey]').length).toBe(3),
    );
    fireEvent.click(wired.container.querySelector('[data-overview-bigkey="2"]') as Element);

    // BUG-001 + BUG-003: the big-key jump forwards the type `memory_sample`
    // already resolved for that row (blob:b is a hash), so the host can
    // pre-colour 屏 B and the browse-history entry keeps its type.
    expect(onOpenTarget).toHaveBeenCalledWith({
      kind: 'key',
      dbIndex: 0,
      key: 'blob:b',
      keyType: 'hash',
    });
    const bucket = JSON.parse(
      globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY) ?? '{}',
    ) as Record<string, Array<{ key: string; dbIndex: number; keyType: string | null }>>;
    expect(bucket[CONNECTION]?.map((entry) => [entry.key, entry.dbIndex, entry.keyType])).toEqual([
      ['blob:b', 0, 'hash'],
    ]);
    await waitFor(() =>
      expect(ids(wired.container, 'data-overview-recent-key')).toEqual(['blob:b']),
    );
  });

  it('clears the list back to its named empty state', async () => {
    pushBrowseEntry(CONNECTION, { key: 'user:1', dbIndex: 0 }, undefined, 1_700_000_000_000);
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-recent-key]').length).toBe(1),
    );

    fireEvent.click(container.querySelector('[data-overview-recent-clear]') as Element);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-recent-key]').length).toBe(0),
    );
    expect(container.querySelector('[data-overview-recent-clear]')).toBeNull();
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBeNull();
  });
});

describe('屏 A → 屏 B 跳转的诚实降级', () => {
  it('marks every affordance unwired and shows the named guidance instead of a dead click', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    const states = new Set(ids(container, 'data-overview-jump'));
    expect(states.size).toBe(1);
    expect([...states][0]).toBe('unwired');

    // Click an unwired big key to trigger the guidance hint
    const bigKeyEl = container.querySelector('[data-overview-bigkey]');
    if (bigKeyEl) {
      fireEvent.click(bigKeyEl);
    } else {
      // fallback: click a quick action
      fireEvent.click(container.querySelector('[data-overview-action="browseDb"]') as Element);
    }
    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    expect(container.querySelector('[role="status"]')).not.toBeNull();
  });

  it('names the panel entry for console / pubsub / import-export targets', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() =>
      expect(container.querySelectorAll('[data-overview-action]').length).toBe(5),
    );

    fireEvent.click(container.querySelector('[data-overview-action="pubsub"]') as Element);
    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    expect(attr(container, '[data-overview-jump-hint]', 'data-overview-jump-hint')).toBe(
      'redis.overview.jump.pendingPanel',
    );
  });

  it('lets the user dismiss the guidance', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    // Click an unwired action to trigger the hint
    fireEvent.click(container.querySelector('[data-overview-action="browseDb"]') as Element);
    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    fireEvent.click(container.querySelector('[data-overview-jump-hint-dismiss]') as Element);
    await waitFor(() => expect(container.querySelector('[data-overview-jump-hint]')).toBeNull());
  });

  it('degrades a throwing host bridge into a named failure instead of a blank screen', async () => {
    const onOpenTarget = vi.fn(() => {
      throw new Error('bridge exploded');
    });
    const { container } = render(<RedisOverviewHome {...baseProps({ onOpenTarget })} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    // Click a wired action to trigger the throwing bridge
    fireEvent.click(container.querySelector('[data-overview-action="browseDb"]') as Element);
    await waitFor(() =>
      expect(container.querySelector('[data-overview-jump-hint]')).not.toBeNull(),
    );
    expect(attr(container, '[data-overview-jump-hint]', 'data-overview-jump-hint')).toBe(
      'redis.overview.jump.failed',
    );
    expect(cardState(container, 'server')).toBe('ready');
  });

  it('stays silent when the bridge accepts the target', async () => {
    const onOpenTarget = vi.fn();
    const { container } = render(<RedisOverviewHome {...baseProps({ onOpenTarget })} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    fireEvent.click(container.querySelector('[data-overview-action="console"]') as Element);
    await waitFor(() => expect(onOpenTarget).toHaveBeenCalled());
    expect(container.querySelector('[data-overview-jump-hint]')).toBeNull();
  });
});

describe('失败与重试', () => {
  it('names the failed state per card and retries the whole set', async () => {
    stub({ info: new Error('connection reset by peer') });
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('failed'));

    expect(container.querySelector('[data-overview-failed="server"]')).not.toBeNull();
    expect(container.querySelector('[data-overview-card-action="server"]')).toBeNull();
    // INFO 挂了不影响 Performance card
    expect(cardState(container, 'performance')).toBe('ready');

    const before = redisInvoke.mock.calls.length;
    fireEvent.click(container.querySelector('[data-overview-retry="server"]') as Element);
    await waitFor(() => expect(redisInvoke.mock.calls.length).toBeGreaterThanOrEqual(before + 4));
    expect(issuedCommands()).toEqual(['db_sizes', 'info', 'memory_sample', 'slowlog_get']);
  });

  it('offers a refresh action in the banner while a card is healthy', async () => {
    const { container } = render(<RedisOverviewHome {...baseProps()} />);
    await waitFor(() => expect(cardState(container, 'server')).toBe('ready'));

    const before = redisInvoke.mock.calls.length;
    fireEvent.click(container.querySelector('[data-overview-banner-refresh]') as Element);
    await waitFor(() => expect(redisInvoke.mock.calls.length).toBeGreaterThanOrEqual(before + 4));
  });
});
