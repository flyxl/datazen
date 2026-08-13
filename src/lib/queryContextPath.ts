import { extractQualifiedToken } from './sqlPathPrefix';
import { namespaceBranchChildNames, type SqlNamespace } from './sqlNamespace';

const RELATION =
  /\b(?:from|join|update|into)\s+(?:only\s+)?((?:"[^"]+"|`[^`]+`|[A-Za-z_][\w$]*)(?:\s*\.\s*(?:"[^"]+"|`[^`]+`|[A-Za-z_][\w$]*))*)/gi;

function unquoteIdent(seg: string): string {
  const trimmed = seg.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith('`') && trimmed.endsWith('`'))
  ) {
    return trimmed.slice(1, -1).replace(/""/g, '"').replace(/``/g, '`');
  }
  return trimmed;
}

/**
 * Last FROM/JOIN/UPDATE/INTO relation, split into identifier segments.
 * `FROM trading_dev.t_order` → `['trading_dev', 't_order']`.
 */
export function inferSqlRelationPath(sql: string): string[] {
  let last: string[] = [];
  RELATION.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = RELATION.exec(sql)) !== null) {
    last = match[1]!
      .split(/\s*\.\s*/)
      .map(unquoteIdent)
      .filter(Boolean);
  }
  return last;
}

export interface QueryContextResolveOpts {
  databases: readonly string[];
  namespaceRoots: readonly string[];
}

/**
 * Context path for query toolbar selectors (database / catalog / schema).
 *
 * Handles `db.` immediately (trailing dot) as well as a completed `db.table`.
 * Returns `null` when the SQL is unqualified or the first segment is not a
 * known root (e.g. PG `public.users`).
 */
export function resolveQueryContextPath(
  sql: string,
  opts: QueryContextResolveOpts,
): string[] | null {
  const roots = new Set([...opts.databases, ...opts.namespaceRoots]);
  const fromToken = contextFromQualifiedToken(sql, roots);
  if (fromToken) return fromToken;

  const path = inferSqlRelationPath(sql);
  if (path.length < 2) return null;
  const parents = path.slice(0, -1);
  const first = parents[0];
  if (!first || !roots.has(first)) return null;
  return parents;
}

function contextFromQualifiedToken(sql: string, roots: Set<string>): string[] | null {
  const token = extractQualifiedToken(sql, sql.length);
  if (!token) return null;
  const segs = token.value.split('.').filter(Boolean);
  const parents = token.endsWithDot ? segs : segs.slice(0, -1);
  const first = parents[0];
  if (!first || !roots.has(first)) return null;
  return parents;
}

export function namespaceRootsFrom(
  namespaceTree: SqlNamespace,
  pathAliases: Record<string, string>,
  databases: readonly string[] = [],
): string[] {
  const names = new Set<string>([...databases, ...Object.keys(pathAliases)]);
  for (const name of namespaceBranchChildNames(namespaceTree, [])) {
    names.add(name);
  }
  return [...names];
}

export function pathsEqual(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((seg, i) => seg === b[i]);
}
