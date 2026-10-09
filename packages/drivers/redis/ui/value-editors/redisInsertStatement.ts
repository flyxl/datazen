/**
 * "Copy insert statement" (PRD §3.3 key header row, absorbed from dbx).
 *
 * Builds a single redis-cli-compatible command that re-creates the key from
 * the detail payload `get_key` returns (`key_value_json` shapes, read-only
 * mirror of packages/drivers/redis/src/driver/session.rs):
 *
 * - string → `{"value": "<s>"}`      ⇒ `SET key value`
 * - hash   → `{field: value, …}`     ⇒ `HSET key f v [f v …]`
 * - list   → `["a", "b"]`            ⇒ `RPUSH key a b`
 * - set    → `["m1", "m2"]`          ⇒ `SADD key m1 m2`
 * - zset   → `[{member, score}, …]`  ⇒ `ZADD key score member …`
 *
 * Stream / JSON / module payloads (server-side IDs, nested docs) cannot be
 * reproduced faithfully by a plain insert command, so those return `null` and
 * the header hides the action — refuse rather than copy a statement that lies.
 * The payload is server data, never translated.
 */
import type { KeyDetail } from '../shared/types';
import { unwrapStringKeyValue } from './stringKeyValue';

/** Tokens that need no quoting in redis-cli (readable pass-through). */
const PLAIN_TOKEN = /^[A-Za-z0-9_.:\-/]+$/;

/** Quote one redis-cli argument: plain when safe, else double-quoted + escaped. */
export function quoteArg(s: string): string {
  if (s.length > 0 && PLAIN_TOKEN.test(s)) return s;
  return `"${s.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function stringArray(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null;
  return value.every((v): v is string => typeof v === 'string') ? value : null;
}

type InsertDetail = Pick<KeyDetail, 'key' | 'keyType' | 'value'>;

export function buildRedisInsertStatement(detail: InsertDetail): string | null {
  const key = quoteArg(detail.key);
  switch (detail.keyType) {
    case 'string':
      return `SET ${key} ${quoteArg(unwrapStringKeyValue(detail.value))}`;
    case 'hash': {
      const v = detail.value;
      if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
      const entries = Object.entries(v as Record<string, unknown>);
      if (entries.length === 0) return null;
      const args = entries
        .map(([field, val]) => `${quoteArg(field)} ${quoteArg(String(val))}`)
        .join(' ');
      return `HSET ${key} ${args}`;
    }
    case 'list': {
      const items = stringArray(detail.value);
      if (items === null || items.length === 0) return null;
      return `RPUSH ${key} ${items.map(quoteArg).join(' ')}`;
    }
    case 'set': {
      const items = stringArray(detail.value);
      if (items === null || items.length === 0) return null;
      return `SADD ${key} ${items.map(quoteArg).join(' ')}`;
    }
    case 'zset': {
      const v = detail.value;
      if (!Array.isArray(v) || v.length === 0) return null;
      const args: string[] = [];
      for (const item of v) {
        if (typeof item !== 'object' || item === null) return null;
        const { member, score } = item as { member?: unknown; score?: unknown };
        if (typeof member !== 'string' || typeof score !== 'number') return null;
        args.push(`${String(score)} ${quoteArg(member)}`);
      }
      return `ZADD ${key} ${args.join(' ')}`;
    }
    default:
      return null;
  }
}
