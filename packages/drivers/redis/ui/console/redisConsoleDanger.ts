/**
 * Client-side Redis command danger classification for the Console.
 */

export type DangerLevel = 'safe' | 'write' | 'danger' | 'ultra-danger';

const ULTRA_DANGER = new Set([
  'FLUSHDB',
  'FLUSHALL',
  'SHUTDOWN',
  'DEBUG',
  'KEYS',
  'CONFIG',
  'ACL',
  'MODULE',
  'CLUSTER',
  'REPLICAOF',
  'SLAVEOF',
]);

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

const WRITE = new Set([
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
  'GETDEL',
  'GETEX',
  'LPUSH',
  'LPUSHX',
  'RPUSH',
  'RPUSHX',
  'LSET',
  'LREM',
  'LTRIM',
  'LINSERT',
  'RPOPLPUSH',
  'LMOVE',
  'LMPOP',
  'BLMPOP',
  'SADD',
  'SREM',
  'SINTERSTORE',
  'SUNIONSTORE',
  'SDIFFSTORE',
  'SMISMEMBER',
  'ZADD',
  'ZREM',
  'ZINCRBY',
  'ZDIFFSTORE',
  'ZINTERSTORE',
  'ZUNIONSTORE',
  'ZREMRANGEBYRANK',
  'ZREMRANGEBYSCORE',
  'HSET',
  'HMSET',
  'HDEL',
  'HINCRBY',
  'HINCRBYFLOAT',
  'XADD',
  'XACK',
  'XDEL',
  'XTRIM',
  'XSETID',
  'PUBLISH',
  'EXEC',
  'MULTI',
  'COPY',
  'MIGRATE',
  'RESTORE',
  'LINK',
]);

/**
 * Classify a Redis command string by its danger level.
 */
export function classifyDangerLevel(command: string): DangerLevel {
  const name = command.trim().split(/\s+/)[0]?.toUpperCase() ?? '';
  if (ULTRA_DANGER.has(name)) return 'ultra-danger';
  if (DANGER.has(name)) return 'danger';
  if (WRITE.has(name)) return 'write';
  return 'safe';
}

/**
 * Whether this danger level requires user confirmation before execution.
 */
export function requiresConfirmation(level: DangerLevel): boolean {
  return level === 'danger' || level === 'ultra-danger';
}

/**
 * Return a color class for the danger badge.
 */
export function dangerBadgeColor(level: DangerLevel): string {
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
