import { describe, expect, it } from 'vitest';
import {
  BIG_KEY_LIMIT,
  FRAGMENTATION_WARN_RATIO,
  buildBannerPills,
  buildBigKeyRows,
  buildMemoryModel,
  buildServerRows,
  buildSlowlogRows,
  classifyOverviewError,
  flattenInfoFields,
  modeValueKey,
  parseInfoFieldsFromRaw,
  summariseSlowlogCommand,
  SLOWLOG_COMMAND_SUMMARY_MAX,
  type ServerRowId,
} from '../overview/overviewModel';
import { parseInfoSections } from '../observe/infoParse';

/** A realistic `INFO` reply trimmed to the fields 屏 A renders. */
const INFO_FIXTURE = [
  '# Server',
  'redis_version:7.2.4',
  'arch_bits:64',
  'uptime_in_days:12',
  'os:Linux 6.1 x86_64',
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

function fields(raw = INFO_FIXTURE): Record<string, string> {
  return parseInfoFieldsFromRaw(raw);
}

describe('flattenInfoFields', () => {
  it('maps every section entry to a flat lookup and keeps the first occurrence', () => {
    const sections = parseInfoSections(INFO_FIXTURE);
    const flat = flattenInfoFields(sections);
    expect(flat.redis_version).toBe('7.2.4');
    // `mode` lives under # Replication — the flat map lets callers ignore sections.
    expect(flat.mode).toBe('standalone');
    expect(flat.used_memory).toBe('1048576');
  });

  it('ignores blank values so a missing field never renders an empty hole', () => {
    const flat = flattenInfoFields(
      parseInfoSections('# Server\r\nredis_version:\r\nmaxmemory:0\r\n'),
    );
    const rows = buildServerRows(flat);
    expect(rows.find((row) => row.id === 'version')?.value).toBeNull();
  });
});

describe('buildServerRows — PRD 卡 1', () => {
  it('emits exactly the ten rows in PRD order, two columns of five', () => {
    const ids = buildServerRows(fields()).map((row) => row.id);
    expect(ids).toEqual<ServerRowId[]>([
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
  });

  it('carries i18n keys instead of copy for labels and units', () => {
    const rows = buildServerRows(fields());
    const uptime = rows.find((row) => row.id === 'uptime');
    expect(uptime?.labelKey).toBe('redis.overview.server.uptime');
    expect(uptime?.unitKey).toBe('redis.overview.unit.days');
    expect(uptime?.value).toBe('12');
    expect(uptime?.valueIsKey).toBe(false);
  });

  it('flags evicted_keys > 0 as a warning row and keeps 0 clean', () => {
    const clean = buildServerRows(fields()).find((row) => row.id === 'evictedKeys');
    expect(clean?.warn).toBe(false);
    const dirty = buildServerRows(
      fields(INFO_FIXTURE.replace('evicted_keys:0', 'evicted_keys:17')),
    );
    expect(dirty.find((row) => row.id === 'evictedKeys')?.warn).toBe(true);
  });

  it('maps known mode tokens to i18n keys and leaves unknown ones raw', () => {
    expect(modeValueKey('standalone')).toBe('redis.overview.mode.standalone');
    expect(modeValueKey('CLUSTER')).toBe('redis.overview.mode.cluster');
    expect(modeValueKey('sentinel')).toBe('redis.overview.mode.sentinel');
    expect(modeValueKey('something-new')).toBeNull();
    expect(modeValueKey(null)).toBeNull();

    const rows = buildServerRows(
      fields(INFO_FIXTURE.replace('mode:standalone', 'mode:something-new')),
    );
    const mode = rows.find((row) => row.id === 'mode');
    expect(mode?.value).toBe('something-new');
    expect(mode?.valueIsKey).toBe(false);
  });

  it('degrades to null values (never a crash) when INFO is empty', () => {
    const rows = buildServerRows({});
    expect(rows).toHaveLength(10);
    expect(rows.every((row) => row.value === null)).toBe(true);
    expect(rows.every((row) => row.warn === false)).toBe(true);
  });
});

describe('buildServerRows — 模式感知行集', () => {
  it('standalone: 输出 10 行含 blockedClients / totalCommands / expiredKeys', () => {
    const rows = buildServerRows(fields('# Replication\r\nmode:standalone'));
    const ids = rows.map((r) => r.id);
    expect(ids).toContain('blockedClients');
    expect(ids).toContain('totalCommands');
    expect(ids).toContain('expiredKeys');
    expect(ids).not.toContain('clusterSlotsOk');
    expect(ids).not.toContain('sentinelMasters');
    expect(rows).toHaveLength(10);
  });

  it('cluster: 输出 10 行含 clusterSlotsOk / clusterKnownNodes / clusterSize', () => {
    const rows = buildServerRows(fields('# Replication\r\nmode:cluster'));
    const ids = rows.map((r) => r.id);
    expect(ids).toContain('clusterSlotsOk');
    expect(ids).toContain('clusterKnownNodes');
    expect(ids).toContain('clusterSize');
    expect(ids).not.toContain('blockedClients');
    expect(ids).not.toContain('expiredKeys');
    expect(rows).toHaveLength(10);
  });

  it('sentinel: 输出 10 行含 sentinelMasters / sentinelSlaves / sentinelSentinels', () => {
    const rows = buildServerRows(fields('# Replication\r\nmode:sentinel'));
    const ids = rows.map((r) => r.id);
    expect(ids).toContain('sentinelMasters');
    expect(ids).toContain('sentinelSlaves');
    expect(ids).toContain('sentinelSentinels');
    expect(ids).not.toContain('blockedClients');
    expect(ids).not.toContain('expiredKeys');
    expect(rows).toHaveLength(10);
  });

  it('cluster: 读取 cluster_slots_ok 等字段值', () => {
    const rows = buildServerRows(
      fields(
        '# Replication\r\nmode:cluster\r\n# Cluster\r\ncluster_slots_ok:16384\r\ncluster_known_nodes:6\r\ncluster_size:3',
      ),
    );
    expect(rows.find((r) => r.id === 'clusterSlotsOk')?.value).toBe('16384');
    expect(rows.find((r) => r.id === 'clusterKnownNodes')?.value).toBe('6');
    expect(rows.find((r) => r.id === 'clusterSize')?.value).toBe('3');
  });

  it('sentinel: 读取 sentinel_masters 等字段值', () => {
    const rows = buildServerRows(
      fields(
        '# Replication\r\nmode:sentinel\r\n# Sentinel\r\nsentinel_masters:2\r\nsentinel_slaves:4\r\nsentinel_sentinels:3',
      ),
    );
    expect(rows.find((r) => r.id === 'sentinelMasters')?.value).toBe('2');
    expect(rows.find((r) => r.id === 'sentinelSlaves')?.value).toBe('4');
    expect(rows.find((r) => r.id === 'sentinelSentinels')?.value).toBe('3');
  });

  it('未知 mode 退化为 standalone 行集', () => {
    const rows = buildServerRows(fields('# Replication\r\nmode:unknown'));
    expect(rows).toHaveLength(10);
    expect(rows.map((r) => r.id)).toContain('blockedClients');
  });

  it('三种模式都恰好 10 行，适配双列 2×5 网格', () => {
    expect(buildServerRows(fields('# Replication\r\nmode:standalone'))).toHaveLength(10);
    expect(buildServerRows(fields('# Replication\r\nmode:cluster'))).toHaveLength(10);
    expect(buildServerRows(fields('# Replication\r\nmode:sentinel'))).toHaveLength(10);
  });
});

describe('buildMemoryModel — PRD 卡 2 gauge', () => {
  it('computes the used/max percentage from the raw byte fields', () => {
    const model = buildMemoryModel(fields());
    expect(model.usedBytes).toBe(1048576);
    expect(model.maxBytes).toBe(4194304);
    expect(model.unlimited).toBe(false);
    expect(model.usedPercent).toBeCloseTo(25, 5);
  });

  it('treats maxmemory 0 as unlimited and refuses to invent a percentage', () => {
    const model = buildMemoryModel(
      fields(INFO_FIXTURE.replace('maxmemory:4194304', 'maxmemory:0')),
    );
    expect(model.unlimited).toBe(true);
    expect(model.maxBytes).toBeNull();
    expect(model.maxHuman).toBeNull();
    expect(model.usedPercent).toBeNull();
  });

  it('clamps a reported used_memory above maxmemory to 100%', () => {
    const model = buildMemoryModel(
      fields(INFO_FIXTURE.replace('used_memory:1048576', 'used_memory:9000000')),
    );
    expect(model.usedPercent).toBe(100);
  });

  it('warns strictly above the fragmentation threshold', () => {
    expect(FRAGMENTATION_WARN_RATIO).toBe(1.5);
    expect(buildMemoryModel(fields()).fragWarn).toBe(false);
    const high = buildMemoryModel(
      fields(INFO_FIXTURE.replace('mem_fragmentation_ratio:1.42', 'mem_fragmentation_ratio:2.8')),
    );
    expect(high.fragRatio).toBe(2.8);
    expect(high.fragWarn).toBe(true);
    const exact = buildMemoryModel(
      fields(INFO_FIXTURE.replace('mem_fragmentation_ratio:1.42', 'mem_fragmentation_ratio:1.5')),
    );
    expect(exact.fragWarn).toBe(false);
  });

  it('reads the Redis 8 fragmentation field names too', () => {
    const redis8 = INFO_FIXTURE.replace(
      'mem_fragmentation_ratio:1.42',
      'mem_fragmentation_ratio_new:1.10\r\nmem_fragmentation_ratio_old:1.90',
    );
    const model = buildMemoryModel(fields(redis8));
    expect(model.fragRatio).toBe(1.9);
    expect(model.fragWarn).toBe(true);
  });

  it('reports an unknown ratio as null rather than 0', () => {
    const model = buildMemoryModel(fields('# Memory\r\nused_memory:10\r\n'));
    expect(model.fragRatio).toBeNull();
    expect(model.fragWarn).toBe(false);
    expect(model.policy).toBeNull();
  });
});

describe('buildBigKeyRows — PRD 卡 2 Top3', () => {
  it('sorts by bytes descending and caps at three rows', () => {
    const rows = buildBigKeyRows({
      samples: [
        { key: 'small', bytes: 10 },
        { key: 'huge', bytes: 5000 },
        { key: 'mid', bytes: 300 },
        { key: 'a4', bytes: 40 },
        { key: 'a5', bytes: 50 },
        { key: 'a6', bytes: 60 },
      ],
    });
    expect(rows.map((row) => row.key)).toEqual(['huge', 'mid', 'a6']);
    expect(rows.map((row) => row.rank)).toEqual([1, 2, 3]);
    expect(rows[0]?.bytes).toBe(5000);
  });

  it('honours the shared Top-N budget', () => {
    expect(BIG_KEY_LIMIT).toBe(3);
    expect(buildBigKeyRows({ samples: [{ key: 'a', bytes: 1 }] }, 1)).toHaveLength(1);
  });

  it('drops nameless samples and tolerates an empty/absent result', () => {
    expect(buildBigKeyRows({ samples: [{ key: '', bytes: 9 }] })).toEqual([]);
    expect(buildBigKeyRows({ samples: [] })).toEqual([]);
    expect(buildBigKeyRows(null)).toEqual([]);
    expect(buildBigKeyRows(undefined)).toEqual([]);
  });

  it('carries the type / TTL / gone state resolved by the same memory_sample call (BUG-003)', () => {
    const [live, gone, unknown] = buildBigKeyRows({
      samples: [
        { key: 'live', bytes: 30, type: 'hash', ttlMs: 8_000, missing: false },
        { key: 'gone', bytes: 20, type: null, ttlMs: -2, missing: true },
        { key: 'unknown', bytes: 10 },
      ],
    });
    expect(live).toMatchObject({ key: 'live', keyType: 'hash', ttlMs: 8_000, missing: false });
    // A key deleted between SCAN and read keeps its own distinguishable state.
    expect(gone).toMatchObject({ key: 'gone', keyType: null, ttlMs: -2, missing: true });
    // A legacy `{ key, bytes }`-only sample degrades to null/absent, never a lie.
    expect(unknown).toMatchObject({ key: 'unknown', keyType: null, ttlMs: null, missing: false });
  });

  it('normalises malformed type / ttl / missing payloads to the unreadable state', () => {
    const [row] = buildBigKeyRows({
      samples: [
        {
          key: 'junk',
          bytes: 5,
          type: '',
          ttlMs: Number.NaN,
          // Deliberately off-contract: the normaliser must not treat it as true.
          missing: 'yes' as unknown as boolean,
        },
      ],
    });
    // Empty/non-string types, non-finite TTLs and non-true flags never fabricate.
    expect(row).toMatchObject({ keyType: null, ttlMs: null, missing: false });
  });
});

describe('buildSlowlogRows — PRD 卡 4', () => {
  it('numbers rows, keeps µs exact and joins the client address with the name', () => {
    const rows = buildSlowlogRows([
      {
        id: 3,
        timestamp: 1700000000,
        durationUs: 41234,
        command: ['KEYS', '*'],
        clientAddr: '127.0.0.1:52134',
        clientName: 'worker',
      },
      {
        id: 2,
        timestamp: 1700000001,
        durationUs: 900,
        command: ['GET big:key'],
        clientAddr: null,
        clientName: null,
      },
    ]);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ rank: 1, id: 3, durationUs: 41234, commandSummary: 'KEYS *' });
    expect(rows[0]?.client).toBe('127.0.0.1:52134 · worker');
    expect(rows[1]?.client).toBeNull();
  });

  it('orders by slowlog id descending so the Top3 is stable across proxies', () => {
    const rows = buildSlowlogRows([
      { id: 1, timestamp: 1, durationUs: 10, command: ['A'] },
      { id: 7, timestamp: 2, durationUs: 20, command: ['B'] },
      { id: 4, timestamp: 3, durationUs: 30, command: ['C'] },
    ]);
    expect(rows.map((row) => row.id)).toEqual([7, 4, 1]);
  });

  it('truncates a monster command into a summary without cutting the whole row', () => {
    const summary = summariseSlowlogCommand(['SET', 'k', 'x'.repeat(500)]);
    expect(summary.length).toBe(SLOWLOG_COMMAND_SUMMARY_MAX);
    expect(summary.endsWith('…')).toBe(true);
    expect(summariseSlowlogCommand([])).toBe('');
    expect(summariseSlowlogCommand(undefined)).toBe('');
  });

  it('returns no rows for an empty or malformed slowlog reply', () => {
    expect(buildSlowlogRows([])).toEqual([]);
    expect(buildSlowlogRows(null)).toEqual([]);
  });
});

describe('buildBannerPills — PRD 头部三枚 pill', () => {
  it('carries version, mode and used memory and points at the monitor sub-pages', () => {
    const flat = fields();
    const pills = buildBannerPills(flat, buildMemoryModel(flat));
    expect(pills.map((pill) => pill.id)).toEqual(['version', 'mode', 'usedMemory']);
    expect(pills[0]?.value).toBe('7.2.4');
    expect(pills[1]?.value).toBe('standalone');
    expect(pills[2]?.value).toBe('1.00M');
    expect(pills[2]?.monitorTarget).toBe('memory');
    expect(pills[0]?.monitorTarget).toBe('info');
  });

  it('falls back to raw bytes when INFO omits the human-readable field', () => {
    const flat = flattenInfoFields(parseInfoSections('# Memory\r\nused_memory:2048\r\n'));
    const pills = buildBannerPills(flat, buildMemoryModel(flat));
    expect(pills[2]?.value).toBe('2048');
  });
});

describe('classifyOverviewError — PRD I-11 未授权 vs 失败', () => {
  it('recognises Redis ACL rejections as an authorization gap', () => {
    for (const message of [
      "NOPERM this user has no permissions to run the 'memory' command",
      'ERR unknown command `MEMORY`',
      'This user has no permissions',
      'ERR client is not authorized to run this command',
      'command is disabled on this instance',
    ]) {
      expect(classifyOverviewError(new Error(message))).toBe('unauthorized');
    }
  });

  it('keeps transport and runtime errors in the failed bucket', () => {
    expect(classifyOverviewError(new Error('connection reset by peer'))).toBe('failed');
    expect(classifyOverviewError('socket hang up')).toBe('failed');
    expect(classifyOverviewError(undefined)).toBe('failed');
    expect(classifyOverviewError(null)).toBe('failed');
  });
});
