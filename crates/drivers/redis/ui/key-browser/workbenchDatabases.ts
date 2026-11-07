/**
 * Redis logical-database list helpers (pure, no React).
 *
 * Redis always exposes `db0 … db15`; the schema store may additionally report
 * non-numeric database names on unusual servers, which get appended so the
 * sidebar never loses an entry the server knows about.
 *
 * Extracted from `RedisWorkbench.tsx` in the 键树 track D-0 split — behaviour is
 * deliberately identical, the point is that the workbench stops being a JSX wall
 * (PRD §7-5 「先拆再改」).
 */

export const REDIS_DB_COUNT = 16;

/** `db0 … db{REDIS_DB_COUNT - 1}`. */
export function allRedisDbs(): string[] {
  return Array.from({ length: REDIS_DB_COUNT }, (_, i) => `db${i}`);
}

/** Standard dbs first, then any extra name the server reported. */
export function mergeDatabases(fromServer: string[]): string[] {
  const extras = fromServer.filter((db) => !/^db(\d+)$/.test(db));
  return [...allRedisDbs(), ...extras];
}

/** Numeric SCAN target of a `db{n}` label; unknown labels fall back to 0. */
export function dbIndexOfName(db: string): number {
  const parsed = Number.parseInt(db.replace('db', ''), 10);
  return Number.isFinite(parsed) ? parsed : 0;
}
