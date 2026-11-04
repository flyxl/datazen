/**
 * Client side of the `key_object_info` driver command (PRD §6, ruling 8-3 = M).
 *
 * The command answers the key-attribute sidebar in **one** pipeline
 * (`MEMORY USAGE` / `OBJECT ENCODING` / `OBJECT IDLETIME` / `OBJECT FREQ` /
 * `PTTL` / `TYPE`) and is deliberately partial: every field except `missing`
 * and `ttlMs` can come back `null` when the server rejected just that one
 * command (`OBJECT FREQ` on a non-LFU policy is the everyday case, `MEMORY
 * USAGE` on Redis < 4.0 another). A `null` therefore means "unknown", never
 * "zero" — the renderers must not fall back to `0`.
 *
 * A key that expired or was deleted between selection and read is the
 * command's `missing: true` success case, not an error; 8-3 chose the sidebar
 * precisely so this state has somewhere to be shown.
 *
 * `maxmemory_policy` is server-wide, not per key, and `key_object_info` does
 * not carry it (PRD §6: "从 `info` 缓存取"), so it comes from `info_filtered`
 * instead. Failures there degrade to `null` — the eviction row disappears
 * rather than the sidebar going red.
 */
import type { InfoSection } from '../observe/infoParse';
import { redisCommandInvoke, type RedisInvokeFn } from '../shared/redisInvoke';

/** Wire shape of the `key_object_info` reply (camelCase, `serde` renamed). */
export interface KeyObjectInfo {
  /** Key vanished (expired / deleted) — a success case, not an error. */
  missing: boolean;
  /** Redis `TYPE`; `null` when that one reply errored. */
  type: string | null;
  /** `MEMORY USAGE` in bytes; `null` when unsupported or rejected. */
  memoryBytes: number | null;
  /** `OBJECT ENCODING`; `null` for module types and on error. */
  encoding: string | null;
  /** `OBJECT IDLETIME` in seconds. */
  idleSeconds: number | null;
  /** `OBJECT FREQ`; `null` unless `maxmemory-policy` is LFU. */
  freq: number | null;
  /** `PTTL` in milliseconds: `-1` no expiry, `-2` missing, `>0` remaining. */
  ttlMs: number;
}
export const TTL_MISSING_MS = -2;

/** The three states a `PTTL` reply can mean, separated for the renderers. */
export type TtlState =
  | { kind: 'missing' }
  | { kind: 'no-expiry' }
  | { kind: 'remaining'; ms: number };

/** Interpret a `PTTL` millisecond reply: `-2` gone, any other negative = no expiry. */
export function describeTtl(ttlMs: number): TtlState {
  if (ttlMs === TTL_MISSING_MS) return { kind: 'missing' };
  if (ttlMs < 0) return { kind: 'no-expiry' };
  return { kind: 'remaining', ms: ttlMs };
}

/**
 * Coarse duration for the TTL / idle read-outs (`12s`, `3m 04s`, `2h 5m`, `9d 3h`).
 *
 * Deliberately unit-free of any translated copy: these are server measurements,
 * so the string is composed of measurement suffixes rather than locale words —
 * the same call the existing `redis.expireAt` column makes. Two significant
 * units keeps the pills narrow without lying about precision.
 */
export function formatDurationMs(totalMs: number): string {
  if (!Number.isFinite(totalMs) || totalMs < 0) return '—';
  const totalSeconds = Math.floor(totalMs / 1000);
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${String(seconds).padStart(2, '0')}s`;
  if (totalMs < 1000) return `${totalMs}ms`;
  return `${seconds}s`;
}

/** `OBJECT IDLETIME` is already seconds; reuse the millisecond formatter. */
export function formatIdle(idleSeconds: number | null): string {
  if (idleSeconds === null || !Number.isFinite(idleSeconds) || idleSeconds < 0) return '—';
  return formatDurationMs(idleSeconds * 1000);
}

/**
 * Branch both KV slots render, derived from the relay selection and the last
 * read. Exposed as a state name rather than as JSX because the tests (and the
 * host's E2E) must target a state marker, not translated copy (PRD §7-6).
 */
export type AttributeViewState =
  | 'no-key'
  | 'loading'
  | 'failed'
  | 'missing'
  | 'ready'
  | 'unavailable';

export function attributeViewState(
  selectedKey: string | null,
  loading: boolean,
  failed: boolean,
  info: KeyObjectInfo | null,
): AttributeViewState {
  if (!selectedKey) return 'no-key';
  if (loading) return 'loading';
  if (failed) return 'failed';
  if (info?.missing) return 'missing';
  if (info) return 'ready';
  // A key is selected but nothing could be read (e.g. no resolved db index).
  return 'unavailable';
}

/** Read one key's attributes. Throws on a transport / server-level failure. */
export async function invokeKeyObjectInfo(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<KeyObjectInfo> {
  return (await invoke('redis', 'key_object_info', {
    dbSessionId,
    dbIndex,
    key,
  })) as KeyObjectInfo;
}

/**
 * `maxmemory_policy` of the server behind `dbSessionId`, read from the `memory`
 * INFO section. "Server-wide" here means *across that server's keys* — the value
 * is still scoped to the session it was read through, which is what the caller's
 * state has to be keyed by as well (redis-kvbar-ui-BUG-005).
 *
 * `null` on any failure (no permission for `CONFIG`-adjacent INFO fields, older
 * server layout, offline node): the eviction row is simply not rendered, which
 * is the §3.4 rule for unavailable data — "不渲染", not "渲染 0".
 */
export async function invokeMaxmemoryPolicy(
  dbSessionId: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<string | null> {
  try {
    const result = await invoke('redis', 'info_filtered', {
      dbSessionId,
      section: 'memory',
    });
    if (!result || typeof result !== 'object' || !('sections' in result)) return null;
    const raw = (result as { sections: unknown }).sections;
    const sections = Array.isArray(raw) ? (raw as InfoSection[]) : [];
    for (const section of sections) {
      const hit = section.entries.find((entry) => entry.key === 'maxmemory_policy');
      if (hit) return hit.value;
    }
    return null;
  } catch {
    return null;
  }
}
