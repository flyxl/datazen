/**
 * Client-side Redis command danger classification for the Console (PRD §4 I-7).
 *
 * FAIL-CLOSED by contract: the four level sets below are the *entire* known
 * command vocabulary. Anything the classifier does not recognise is treated as
 * `ultra-danger` (blocked) with `unknown: true`, so a typo, a module command
 * (`JSON.SET`, `FT.SEARCH`, `BF.ADD`) or a command added to a newer Redis can
 * never be waved through by an implicit default. The historical
 * `return 'safe'` fallback was exactly that hole and must not come back.
 *
 * Levels are kept backwards compatible (`safe` | `write` | `danger` |
 * 'ultra-danger'); only membership and the default branch changed:
 *   - `ultra-danger`  → the Console refuses to run it at all (PRD I-7:
 *     "默认阻断，不可仅弹确认放行"). KEYS / FLUSHALL / FLUSHDB / CONFIG / EVAL /
 *     EVALSHA / SCRIPT / DEBUG / SHUTDOWN are the PRD/task-book named core; the
 *     remaining members were already `ultra-danger` before this change.
 *   - `danger`        → runs after one confirmation (unchanged semantics).
 *   - `write`         → no confirmation, but Safe Mode blocks it (unchanged).
 *   - `safe`          → read / no dataset mutation (must be listed explicitly).
 *
 * Pure module: no React, no i18n, no IO, so it is table-testable.
 */

export type DangerLevel = 'safe' | 'write' | 'danger' | 'ultra-danger';

/** Strictness order used to fold a multi-command batch into one level. */
const LEVEL_RANK: Record<DangerLevel, number> = {
  safe: 0,
  write: 1,
  danger: 2,
  'ultra-danger': 3,
};

/**
 * Blocked (PRD §4 I-7). Destructive, server-wide or arbitrary-code commands:
 * a confirmation dialog is not an adequate brake, so the Console refuses them.
 * The workbench has purpose-built panels for the legitimate use cases.
 */
const ULTRA_DANGER = new Set([
  // PRD I-7 named core
  'KEYS',
  'FLUSHALL',
  'FLUSHDB',
  'CONFIG',
  'EVAL',
  'EVALSHA',
  // Task-book additions (server-side Lua loading / debugger / shutdown)
  'SCRIPT',
  'DEBUG',
  'SHUTDOWN',
  // Already ultra-danger before I-7; kept so the tier stays "server-wide admin"
  'ACL',
  'MODULE',
  'CLUSTER',
  'REPLICAOF',
  'SLAVEOF',
]);

/**
 * Confirm tier — deletes / expires / renames / connection-wide commands.
 * Membership is unchanged by I-7 (no silent upgrade to blocked).
 */
const DANGER = new Set([
  'DEL',
  'UNLINK',
  'RENAME',
  'RENAMENX',
  'EXPIRE',
  'PEXPIRE',
  'EXPIREAT',
  'PEXPIREAT',
  'PERSIST',
  'MOVE',
  'SORT',
  'OBJECT',
  'CLIENT',
  'WAIT',
  'SWAPDB',
  'SUBSCRIBE',
  'PSUBSCRIBE',
  'UNSUBSCRIBE',
  'PUNSUBSCRIBE',
  'DISCARD',
  'RESET',
]);

/** Write tier — dataset mutations (Safe Mode blocks these, no dialog). */
const WRITE = new Set([
  // Strings
  'SET',
  'MSET',
  'MSETNX',
  'SETEX',
  'PSETEX',
  'SETNX',
  'SETXX',
  'APPEND',
  'INCR',
  'DECR',
  'INCRBY',
  'DECRBY',
  'INCRBYFLOAT',
  'GETSET',
  'SETRANGE',
  'SETBIT',
  'BITFIELD',
  'BITOP',
  'GETDEL',
  'GETEX',
  // Lists
  'LPUSH',
  'LPUSHX',
  'RPUSH',
  'RPUSHX',
  'LPOP',
  'RPOP',
  'LSET',
  'LREM',
  'LTRIM',
  'LINSERT',
  'RPOPLPUSH',
  'LMOVE',
  'LMPOP',
  'BLMPOP',
  // Sets
  'SADD',
  'SREM',
  'SPOP',
  'SMOVE',
  'SINTERSTORE',
  'SUNIONSTORE',
  'SDIFFSTORE',
  'SMISMEMBER',
  // Sorted sets
  'ZADD',
  'ZREM',
  'ZINCRBY',
  'ZPOPMIN',
  'ZPOPMAX',
  'ZDIFFSTORE',
  'ZINTERSTORE',
  'ZUNIONSTORE',
  'ZREMRANGEBYRANK',
  'ZREMRANGEBYSCORE',
  'ZREMRANGEBYLEX',
  'GEORADIUS',
  'GEORADIUSBYMEMBER',
  // Hashes
  'HSET',
  'HMSET',
  'HDEL',
  'HSETNX',
  'HINCRBY',
  'HINCRBYFLOAT',
  // Streams
  'XADD',
  'XACK',
  'XDEL',
  'XTRIM',
  'XSETID',
  'XGROUP',
  'XCLAIM',
  'XAUTOCLAIM',
  // PubSub / transactions
  'PUBLISH',
  'EXEC',
  'MULTI',
  // Keyspace / server mutations
  'COPY',
  'MIGRATE',
  'RESTORE',
  'LINK',
  'TOUCH',
  'GEOADD',
  'GEOSEARCHSTORE',
]);

/**
 * Explicitly known read-only / non-mutating commands. This list exists only
 * because the default is now "blocked": a command missing here is *not*
 * treated as read-only, it is treated as unknown.
 */
const SAFE = new Set([
  // Core reads
  'GET',
  'MGET',
  'GETBIT',
  'GETRANGE',
  'STRLEN',
  'SUBSTR',
  'EXISTS',
  'TTL',
  'PTTL',
  'EXPIRETIME',
  'PEXPIRETIME',
  'TYPE',
  'DUMP',
  'RANDOMKEY',
  'SCAN',
  'HSCAN',
  'SSCAN',
  'ZSCAN',
  // Hash / list / set / zset reads
  'HEXISTS',
  'HGET',
  'HGETALL',
  'HKEYS',
  'HLEN',
  'HMGET',
  'HSTRLEN',
  'HVALS',
  'LINDEX',
  'LLEN',
  'LPOS',
  'LRANGE',
  'SCARD',
  'SDIFF',
  'SINTER',
  'SINTERCARD',
  'SISMEMBER',
  'SMEMBERS',
  'SRANDMEMBER',
  'SUNION',
  'ZCARD',
  'ZCOUNT',
  'ZDIFF',
  'ZINTER',
  'ZLEXCOUNT',
  'ZRANGE',
  'ZRANGEBYLEX',
  'ZRANGEBYSCORE',
  'ZRANK',
  'ZREVRANGE',
  'ZREVRANGEBYLEX',
  'ZREVRANGEBYSCORE',
  'ZREVRANK',
  'ZSCORE',
  'ZUNION',
  // Bitmap / Geo reads
  'BITCOUNT',
  'BITPOS',
  'GEODIST',
  'GEOHASH',
  'GEOPOS',
  'GEORADIUS_RO',
  'GEORADIUSBYMEMBER_RO',
  'GEOSEARCH',
  // Stream reads
  'XINFO',
  'XLEN',
  'XPENDING',
  'XRANGE',
  'XREAD',
  'XREADGROUP',
  'XREVRANGE',
  // Server introspection (no dataset mutation)
  'PING',
  'ECHO',
  'INFO',
  'DBSIZE',
  'LASTSAVE',
  'TIME',
  'MEMORY',
  'MONITOR',
  'SLOWLOG',
  'COMMAND',
  'LOLWUT',
  // Connection / transaction bookkeeping
  'AUTH',
  'HELLO',
  'SELECT',
  'WATCH',
  'UNWATCH',
  // Durability triggers — operator-visible, but they never touch the dataset
  'SAVE',
  'BGSAVE',
  'BGREWRITEAOF',
]);

/** Commands the driver additionally gates on its own `allowFlush` setting. */
const FLUSH_COMMANDS = new Set(['FLUSHDB', 'FLUSHALL']);

/** A single classified command. */
export interface CommandAssessment {
  /** The command text as typed (whitespace normalised for one logical command). */
  readonly raw: string;
  /** Upper-cased command name token, `''` when the input is blank. */
  readonly name: string;
  readonly level: DangerLevel;
  /**
   * `true` when `name` is not part of the known vocabulary. Such a command is
   * reported as `ultra-danger` but must never share the "destructive command"
   * copy — the user needs to know the Console did not recognise it.
   */
  readonly unknown: boolean;
}

/**
 * Extract the command name token. Tolerant by design (the parser must never
 * throw and never grades on argument content):
 *  - leading/trailing whitespace and `\n` are ignored;
 *  - a quoted command token (`"GET" k`) is unwrapped;
 *  - a SQL habit trailing `;` is dropped;
 *  - everything after the first token — including an `EVAL` script body, which
 *    may mention `KEYS`/`DEL` — is *not* inspected.
 */
export function commandNameOf(command: string): string {
  const trimmed = command.trim();
  if (!trimmed) return '';
  const quote = trimmed[0];
  let token: string;
  if (quote === '"' || quote === "'") {
    const end = trimmed.indexOf(quote, 1);
    token = end === -1 ? trimmed.slice(1) : trimmed.slice(1, end);
  } else {
    token = trimmed.split(/\s+/)[0] ?? '';
  }
  return token.trim().replace(/;+$/, '').toUpperCase();
}

/** Classify one logical command (never a batch). */
export function assessCommand(command: string): CommandAssessment {
  const raw = command.replace(/\s+/g, ' ').trim();
  const name = commandNameOf(raw);
  if (!raw || !name) return { raw, name: '', level: 'safe', unknown: false };
  if (ULTRA_DANGER.has(name)) return { raw, name, level: 'ultra-danger', unknown: false };
  if (DANGER.has(name)) return { raw, name, level: 'danger', unknown: false };
  if (WRITE.has(name)) return { raw, name, level: 'write', unknown: false };
  if (SAFE.has(name)) return { raw, name, level: 'safe', unknown: false };
  // fail-closed: unknown commands are blocked, not waved through (PRD §4 I-7).
  // Reverting this line to `{ level: 'safe' }` must turn
  // `redisConsoleSafetyCounterproof.test.ts` red — that suite is the gate.
  return { raw, name, level: 'ultra-danger', unknown: true };
}

/**
 * Level-only view kept for existing consumers (completion popup, result tabs).
 * Blank input reports `safe` because the Console refuses to run blank input
 * before it ever classifies it; a *non-blank* unknown command is blocked.
 */
export function classifyDangerLevel(command: string): DangerLevel {
  return assessCommand(command).level;
}

/** Whether a level is the blocking tier (refuse to run, no confirmation path). */
export function isBlockedLevel(level: DangerLevel): boolean {
  return level === 'ultra-danger';
}

/** Whether this danger level requires user confirmation before execution. */
export function requiresConfirmation(level: DangerLevel): boolean {
  return level === 'danger' || level === 'ultra-danger';
}

/** Fold a level list into its strictest member. */
export function worstLevel(levels: readonly DangerLevel[]): DangerLevel {
  let worst: DangerLevel = 'safe';
  for (const level of levels) {
    if (LEVEL_RANK[level] > LEVEL_RANK[worst]) worst = level;
  }
  return worst;
}

/** Whether `level` is at least as strict as `min`. */
export function isAtLeast(level: DangerLevel, min: DangerLevel): boolean {
  return LEVEL_RANK[level] >= LEVEL_RANK[min];
}

/** Numeric strictness of a level (0 = safe … 3 = blocked); exported for folds. */
export function dangerRank(level: DangerLevel): number {
  return LEVEL_RANK[level];
}

/** Whether the Console's own `allowFlush` opt-in applies to this command name. */
export function isFlushCommand(name: string): boolean {
  return FLUSH_COMMANDS.has(name);
}

/**
 * Return a color class for the danger badge. Unknown commands get their own
 * treatment so "not recognised" never looks like "known destructive".
 */
export function dangerBadgeColor(level: DangerLevel, unknown = false): string {
  if (level === 'ultra-danger' && unknown) return 'bg-red-900 text-white ring-1 ring-red-400';
  switch (level) {
    case 'ultra-danger':
      return 'bg-red-600 text-white';
    case 'danger':
      return 'bg-orange-500 text-white';
    case 'write':
      return 'bg-yellow-500 text-black';
    default:
      return 'bg-green-600 text-white';
  }
}
