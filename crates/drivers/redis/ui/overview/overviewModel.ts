/**
 * 屏 A (Redis connection home) — pure view model.
 *
 * Everything the overview cards render is derived here so the components stay
 * thin (PRD §5 "组件薄 + 纯逻辑模块厚") and every rule is unit-testable without
 * React: the ten mode-aware Server rows (7 common + 3 mode-specific for
 * standalone/cluster/sentinel), the memory gauge, the 16-cell key-space grid,
 * the Top-5 big keys, the Top-5 slowlog rows and the error classification behind
 * the named "unauthorized" empty states (PRD I-11).
 *
 * HARD INVARIANT (PRD §3.0 / §3.1): 屏 A reads **only** `info`, `db_sizes`,
 * `slowlog_get` and `memory_sample`. No `scan_keys`, no `list_children`, no
 * `scan_values`, no per-key round trip. Adding one breaks the premise that lets
 * 屏 A be the default landing screen, so this module deliberately accepts no
 * key-detail input either.
 *
 * Copy-free by construction: rows carry **i18n keys** (`labelKey` / `unitKey`)
 * plus raw server values; only components call `t()`.
 */
import { parseInfoSections, type InfoSection } from '../observe/infoParse';

/** 碎片率超过该值视为异常（PRD §3.1 卡 1/卡 2 的 warning 判定位）。 */
export const FRAGMENTATION_WARN_RATIO = 1.5;

/** 卡 2 / 卡 4 的 Top-N 预算（概览屏限制为3行以适配一屏布局）。 */
export const BIG_KEY_LIMIT = 3;
export const SLOWLOG_LIMIT = 3;

/** 命令摘要截断长度，避免一行慢查询命令撑破卡片。 */
export const SLOWLOG_COMMAND_SUMMARY_MAX = 96;

// ---------------------------------------------------------------------------
// Command payload shapes (structural twins of `MonitorPanel`'s private types;
// duplicated on purpose so this module never imports a React component).
// ---------------------------------------------------------------------------

export interface OverviewMemorySample {
  key: string;
  bytes: number;
  /** Redis `TYPE`; null/absent when unreadable or the key vanished after SCAN. */
  type?: string | null;
  /** `PTTL` ms: -1 no expiry, -2 gone, >0 remaining; null when unreadable. */
  ttlMs?: number | null;
  /** The key expired / was deleted between `SCAN` and the field read. */
  missing?: boolean;
}

export interface OverviewMemorySampleResult {
  samples: OverviewMemorySample[];
  truncated?: boolean;
}

export interface OverviewSlowlogEntry {
  id: number;
  timestamp: number;
  durationUs: number;
  command: string[];
  clientAddr?: string | null;
  clientName?: string | null;
}

// ---------------------------------------------------------------------------
// INFO field lookup
// ---------------------------------------------------------------------------

/**
 * Flat `field → value` map over every INFO section. First occurrence wins, which
 * matches Redis' own field uniqueness within a `INFO` reply and lets callers look
 * up `redis_version` / `mode` without hard-coding section names.
 */
export function flattenInfoFields(sections: InfoSection[]): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const section of sections) {
    for (const entry of section.entries) {
      if (!(entry.key in fields)) fields[entry.key] = entry.value;
    }
  }
  return fields;
}

/** `INFO` 原文 → 扁平字段表（一次解析，屏 A 的所有派生值都从它来）。 */
export function parseInfoFieldsFromRaw(raw: string): Record<string, string> {
  return flattenInfoFields(parseInfoSections(raw));
}

function rawField(fields: Record<string, string>, key: string): string | null {
  const value = fields[key];
  if (value === undefined) return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function numField(fields: Record<string, string>, key: string): number | null {
  const raw = rawField(fields, key);
  if (raw === null) return null;
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : null;
}

// ---------------------------------------------------------------------------
// Card 1 — Server overview
// ---------------------------------------------------------------------------

export type ServerRowId =
  | 'version'
  | 'mode'
  | 'arch'
  | 'uptime'
  | 'connectedClients'
  | 'blockedClients'
  | 'opsPerSec'
  | 'totalCommands'
  | 'evictedKeys'
  | 'expiredKeys'
  // Cluster 模式特有
  | 'clusterSlotsOk'
  | 'clusterKnownNodes'
  | 'clusterSize'
  // Sentinel 模式特有
  | 'sentinelMasters'
  | 'sentinelSlaves'
  | 'sentinelSentinels';

export interface ServerOverviewRow {
  id: ServerRowId;
  /** `redis.overview.server.<id>` — rendered label. */
  labelKey: string;
  /** Raw server value (`null` ⇒ INFO did not report it). */
  value: string | null;
  /** Optional unit template key; when set the component renders `t(unitKey, { value })`. */
  unitKey: string | null;
  /** `true` ⇒ {@link value} 本身就是一个 i18n key（`mode` 的已知服务端取值）。 */
  valueIsKey: boolean;
  /** Warning tint (PRD: `evicted_keys > 0`). */
  warn: boolean;
}

const SERVER_ROW_LABEL: Record<ServerRowId, string> = {
  version: 'redis.overview.server.version',
  mode: 'redis.overview.server.mode',
  arch: 'redis.overview.server.arch',
  uptime: 'redis.overview.server.uptime',
  connectedClients: 'redis.overview.server.connectedClients',
  blockedClients: 'redis.overview.server.blockedClients',
  opsPerSec: 'redis.overview.server.opsPerSec',
  totalCommands: 'redis.overview.server.totalCommands',
  evictedKeys: 'redis.overview.server.evictedKeys',
  expiredKeys: 'redis.overview.server.expiredKeys',
  // Cluster
  clusterSlotsOk: 'redis.overview.server.clusterSlotsOk',
  clusterKnownNodes: 'redis.overview.server.clusterKnownNodes',
  clusterSize: 'redis.overview.server.clusterSize',
  // Sentinel
  sentinelMasters: 'redis.overview.server.sentinelMasters',
  sentinelSlaves: 'redis.overview.server.sentinelSlaves',
  sentinelSentinels: 'redis.overview.server.sentinelSentinels',
};

/** `mode` is a server token, not copy: known values map to i18n keys, the rest stays raw. */
export function modeValueKey(mode: string | null): string | null {
  if (mode === null) return null;
  const normalized = mode.trim().toLowerCase();
  if (normalized === 'standalone') return 'redis.overview.mode.standalone';
  if (normalized === 'cluster') return 'redis.overview.mode.cluster';
  if (normalized === 'sentinel') return 'redis.overview.mode.sentinel';
  return null;
}

function serverRow(
  id: ServerRowId,
  value: string | null,
  options: { unitKey?: string; valueIsKey?: boolean; warn?: boolean } = {},
): ServerOverviewRow {
  return {
    id,
    labelKey: SERVER_ROW_LABEL[id],
    value,
    unitKey: options.unitKey ?? null,
    valueIsKey: options.valueIsKey === true,
    warn: options.warn === true,
  };
}

/**
 * 模式感知的 Server 行集：恰好 10 行（双列 2×5 网格）。
 *
 * - Standalone: 通用运维指标（clients / ops / evicted / expired）
 * - Cluster:    集群拓扑健康（slots_ok / known_nodes / size）
 * - Sentinel:   哨兵高可用拓扑（masters / replicas / sentinels）
 *
 * Cluster/Sentinel 下 blocked_clients / total_commands / expired_keys 移除：
 * 单节点值在多节点拓扑下易误导，概览页应展示拓扑健康指标。
 */
export function buildServerRows(fields: Record<string, string>): ServerOverviewRow[] {
  const mode = (rawField(fields, 'mode') ?? 'standalone').toLowerCase();
  const modeKey = modeValueKey(rawField(fields, 'mode'));
  const arch = numField(fields, 'arch_bits');
  const uptime = numField(fields, 'uptime_in_days');
  const connected = numField(fields, 'connected_clients');
  const ops = numField(fields, 'instantaneous_ops_per_sec');
  const evicted = numField(fields, 'evicted_keys');

  const versionRow = serverRow('version', rawField(fields, 'redis_version'));
  const modeRow_ = modeKey
    ? serverRow('mode', modeKey, { valueIsKey: true })
    : serverRow('mode', rawField(fields, 'mode'));
  const archRow = serverRow('arch', arch === null ? null : String(arch), {
    unitKey: 'redis.overview.unit.bits',
  });
  const uptimeRow = serverRow('uptime', uptime === null ? null : String(uptime), {
    unitKey: 'redis.overview.unit.days',
  });
  const connectedRow = serverRow(
    'connectedClients',
    connected === null ? null : String(connected),
    {
      unitKey: 'redis.overview.unit.clients',
    },
  );
  const opsRow = serverRow('opsPerSec', ops === null ? null : String(ops), {
    unitKey: 'redis.overview.unit.opsPerSec',
  });
  // PRD §3.1: 淘汰过的键必须显眼 —— evicted_keys > 0 自动 warning 色。
  const evictedRow = serverRow('evictedKeys', evicted === null ? null : String(evicted), {
    warn: (evicted ?? 0) > 0,
  });

  if (mode === 'cluster') {
    return [
      versionRow,
      modeRow_,
      archRow,
      uptimeRow,
      connectedRow,
      serverRow('clusterSlotsOk', rawField(fields, 'cluster_slots_ok')),
      serverRow('clusterKnownNodes', rawField(fields, 'cluster_known_nodes')),
      serverRow('clusterSize', rawField(fields, 'cluster_size')),
      opsRow,
      evictedRow,
    ];
  }

  if (mode === 'sentinel') {
    return [
      versionRow,
      modeRow_,
      archRow,
      uptimeRow,
      connectedRow,
      serverRow('sentinelMasters', rawField(fields, 'sentinel_masters')),
      serverRow('sentinelSlaves', rawField(fields, 'sentinel_slaves')),
      serverRow('sentinelSentinels', rawField(fields, 'sentinel_sentinels')),
      opsRow,
      evictedRow,
    ];
  }

  // Standalone（默认）— 保持原始 PRD 顺序
  const blocked = numField(fields, 'blocked_clients');
  const total = numField(fields, 'total_commands_processed');
  const expired = numField(fields, 'expired_keys');
  return [
    versionRow,
    modeRow_,
    archRow,
    uptimeRow,
    connectedRow,
    serverRow('blockedClients', blocked === null ? null : String(blocked), {
      unitKey: 'redis.overview.unit.clients',
    }),
    opsRow,
    serverRow('totalCommands', total === null ? null : String(total)),
    evictedRow,
    serverRow('expiredKeys', expired === null ? null : String(expired)),
  ];
}

// ---------------------------------------------------------------------------
// Card 2 — Memory
// ---------------------------------------------------------------------------

export interface MemoryModel {
  usedBytes: number | null;
  usedHuman: string | null;
  maxBytes: number | null;
  maxHuman: string | null;
  /** `maxmemory 0` / absent ⇒ 服务端未设上限。 */
  unlimited: boolean;
  /** 0–100，`null` ⇒ 无法计算（缺 used 或无上限）。 */
  usedPercent: number | null;
  fragRatio: number | null;
  fragWarn: boolean;
  policy: string | null;
}

/**
 * `mem_fragmentation_ratio` is Redis ≤7 的真名；Redis 8 拆成了
 * `mem_fragmentation_ratio_old` / `_new`，所以按顺序取第一个存在的。
 */
function fragRatioOf(fields: Record<string, string>): number | null {
  return (
    numField(fields, 'mem_fragmentation_ratio') ??
    numField(fields, 'mem_fragmentation_ratio_old') ??
    numField(fields, 'mem_fragmentation_ratio_new')
  );
}

export function buildMemoryModel(fields: Record<string, string>): MemoryModel {
  const usedBytes = numField(fields, 'used_memory');
  const maxBytes = numField(fields, 'maxmemory');
  const unlimited = maxBytes === null || maxBytes <= 0;
  let usedPercent: number | null = null;
  if (usedBytes !== null && !unlimited && maxBytes > 0) {
    usedPercent = Math.min(100, Math.max(0, (usedBytes / maxBytes) * 100));
  }
  const fragRatio = fragRatioOf(fields);
  return {
    usedBytes,
    usedHuman: rawField(fields, 'used_memory_human'),
    maxBytes: unlimited ? null : maxBytes,
    maxHuman: unlimited ? null : rawField(fields, 'maxmemory_human'),
    unlimited,
    usedPercent,
    fragRatio,
    fragWarn: fragRatio !== null && fragRatio > FRAGMENTATION_WARN_RATIO,
    policy: rawField(fields, 'maxmemory_policy'),
  };
}

// ---------------------------------------------------------------------------
// Card 2 (big keys) / Card 4 (slowlog)
// ---------------------------------------------------------------------------

export interface BigKeyRow {
  rank: number;
  key: string;
  bytes: number;
  /** Redis `TYPE` from the same `memory_sample` call; null when unreadable/gone. */
  keyType: string | null;
  /** `PTTL` ms: -1 no expiry, -2 gone, >0 remaining; null when unreadable. */
  ttlMs: number | null;
  /** Key vanished between `SCAN` and the field read — an empty, labelled state. */
  missing: boolean;
}

/**
 * `memory_sample` resolves `MEMORY USAGE` + `TYPE` + `PTTL` for the whole sample
 * in one batch (see `ops_workbench::fetch_memory_sample_fields`), so 键名 / 类型 /
 * 字节 / TTL all come from the same single 屏 A command — the zero-键级-往返
 * invariant holds because there is no extra per-key round trip to break it.
 */
export function buildBigKeyRows(
  result: OverviewMemorySampleResult | null | undefined,
  limit: number = BIG_KEY_LIMIT,
): BigKeyRow[] {
  const samples = Array.isArray(result?.samples) ? result.samples : [];
  return [...samples]
    .filter((sample) => sample && typeof sample.key === 'string' && sample.key.length > 0)
    .sort((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0))
    .slice(0, Math.max(0, limit))
    .map((sample, offset) => ({
      rank: offset + 1,
      key: sample.key,
      bytes: Number.isFinite(sample.bytes) ? sample.bytes : 0,
      keyType: typeof sample.type === 'string' && sample.type.length > 0 ? sample.type : null,
      ttlMs: Number.isFinite(sample.ttlMs) ? (sample.ttlMs as number) : null,
      missing: sample.missing === true,
    }));
}

export interface SlowlogRow {
  rank: number;
  id: number;
  durationUs: number;
  commandSummary: string;
  /** `null` ⇒ SLOWLOG 未带客户端信息，组件渲染具名"无客户端"态。 */
  client: string | null;
  timestamp: number;
}

/** `SLOWLOG GET` 的 args 数组折成一行摘要；首元素本身可能含空格。 */
export function summariseSlowlogCommand(command: string[] | null | undefined): string {
  const parts = (command ?? []).filter((part) => typeof part === 'string' && part.length > 0);
  const joined = parts.join(' ').trim();
  if (joined.length <= SLOWLOG_COMMAND_SUMMARY_MAX) return joined;
  return `${joined.slice(0, SLOWLOG_COMMAND_SUMMARY_MAX - 1)}…`;
}

export function buildSlowlogRows(
  entries: OverviewSlowlogEntry[] | null | undefined,
  limit: number = SLOWLOG_LIMIT,
): SlowlogRow[] {
  const list = Array.isArray(entries) ? entries : [];
  return (
    [...list]
      // Redis 已按发生顺序回，但托管代理偶尔乱序 —— 显式按 id 降序保证 Top5 稳定。
      .sort((a, b) => Number(b.id) - Number(a.id))
      .slice(0, Math.max(0, limit))
      .map((entry, offset) => {
        const addr =
          typeof entry.clientAddr === 'string' && entry.clientAddr.length > 0
            ? entry.clientAddr
            : null;
        const name =
          typeof entry.clientName === 'string' && entry.clientName.length > 0
            ? entry.clientName
            : null;
        return {
          rank: offset + 1,
          id: Number(entry.id),
          durationUs: Number.isFinite(entry.durationUs) ? entry.durationUs : 0,
          commandSummary: summariseSlowlogCommand(entry.command),
          client: [addr, name].filter(Boolean).join(' · ') || null,
          timestamp: Number(entry.timestamp),
        };
      })
  );
}

// ---------------------------------------------------------------------------
// Banner pills
// ---------------------------------------------------------------------------

export interface BannerPill {
  id: 'version' | 'mode' | 'usedMemory';
  value: string | null;
  /** 目标监控子页（PRD: 点击 → 打开监控对应子页）。 */
  monitorTarget: 'info' | 'memory';
}

export function buildBannerPills(
  fields: Record<string, string>,
  memory: MemoryModel,
): BannerPill[] {
  const used = memory.usedHuman ?? (memory.usedBytes === null ? null : String(memory.usedBytes));
  return [
    { id: 'version', value: rawField(fields, 'redis_version'), monitorTarget: 'info' },
    { id: 'mode', value: rawField(fields, 'mode'), monitorTarget: 'info' },
    { id: 'usedMemory', value: used, monitorTarget: 'memory' },
  ];
}

// ---------------------------------------------------------------------------
// Error classification (PRD I-11 named "未授权" empty state, 不报错)
// ---------------------------------------------------------------------------

export type OverviewIssue = 'unauthorized' | 'failed';

const UNAUTHORIZED_PATTERNS: RegExp[] = [
  /\bnoperm\b/i,
  /no permissions/i,
  /not authorized/i,
  /\bpermission denied\b/i,
  /\bforbidden\b/i,
  /\bunknown command\b/i,
  /is not allowed/i,
  /\bdisabled\b/i,
];

/**
 * 屏 A 的可选数据源各带独立权限位（`redis:allow-memory-sample` /
 * `redis:allow-slowlog-get`），而这些权限位只是**声明式元数据**（宿主不做
 * 运行时门闸），所以前端唯一可靠的判定来源是命令自身的错误形状：
 * Redis ACL 拒绝回 `NOPERM …`，托管端常见 `unknown command`。命中即渲染
 * 具名"未授权"空态而不是错误条。
 */
export function classifyOverviewError(error: unknown): OverviewIssue {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === 'string'
        ? error
        : error === undefined || error === null
          ? ''
          : String(error);
  if (!message) return 'failed';
  return UNAUTHORIZED_PATTERNS.some((pattern) => pattern.test(message)) ? 'unauthorized' : 'failed';
}
