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

/** Driver `use_database` pin for path-hierarchy namespaces (e.g. Superset `558/hive/snap`). */
export function buildPathHierarchyDatabasePin(
  rootDatabase: string,
  namespacePath: readonly string[],
): string {
  const root = rootDatabase.trim();
  if (!root) return namespacePath.join('/');
  if (namespacePath.length === 0) return root;
  return [root, ...namespacePath].join('/');
}

/** Inverse of {@link buildPathHierarchyDatabasePin} for table-open / panel bootstrap. */
export function splitPathHierarchyDatabasePin(pin: string): {
  root: string;
  namespacePath: string[];
} {
  const trimmed = pin.trim();
  if (!trimmed) return { root: '', namespacePath: [] };
  const slash = trimmed.indexOf('/');
  if (slash < 0) return { root: trimmed, namespacePath: [] };
  return {
    root: trimmed.slice(0, slash),
    namespacePath: trimmed
      .slice(slash + 1)
      .split('/')
      .filter(Boolean),
  };
}

export type PathHierarchySelectorSegment =
  | { kind: 'label'; name: string }
  | { kind: 'select'; levelIndex: number; options: string[]; value: string };

/**
 * Extend `contextPath` through single-option namespace levels (auto-pick).
 * Returns `null` when no change is needed.
 */
export function autoCompletePathHierarchyPath(
  namespaceTree: SqlNamespace,
  pathAliases: Record<string, string>,
  databases: readonly string[],
  contextPath: readonly string[],
): string[] | null {
  const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
  if (roots.length === 0) return null;

  let extended = [...contextPath];
  if (extended.length === 0 && roots.length === 1) {
    extended = [roots[0]!];
  }

  while (true) {
    const children = namespaceBranchChildNames(namespaceTree, extended);
    if (children.length !== 1) break;
    extended = [...extended, children[0]!];
  }

  if (extended.length === contextPath.length && pathsEqual(extended, contextPath)) {
    return null;
  }
  return extended;
}

/** UI segments: single-option levels become labels; multi-option levels stay as selects. */
export function buildPathHierarchySelectorSegments(
  namespaceTree: SqlNamespace,
  pathAliases: Record<string, string>,
  databases: readonly string[],
  contextPath: readonly string[],
): PathHierarchySelectorSegment[] {
  const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
  if (roots.length === 0) return [];

  const segments: PathHierarchySelectorSegment[] = [];
  const path: string[] = [];
  let options = roots;

  while (options.length > 0) {
    const depth = path.length;
    if (options.length === 1) {
      const only = options[0]!;
      segments.push({ kind: 'label', name: only });
      path.push(only);
    } else {
      const value = contextPath[depth] ?? '';
      segments.push({ kind: 'select', levelIndex: depth, options: [...options], value });
      if (!value) break;
      path.push(value);
    }

    const children = namespaceBranchChildNames(namespaceTree, path);
    if (children.length === 0) break;
    options = children;
  }

  return segments;
}

/** Placeholder UI while namespace tree is loading or has no roots yet. */
export const PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS: PathHierarchySelectorSegment[] = [
  { kind: 'select', levelIndex: 0, options: [], value: '' },
  { kind: 'select', levelIndex: 1, options: [], value: '' },
];

export function pathHierarchySelectorSegmentsForUi(
  namespaceTree: SqlNamespace,
  pathAliases: Record<string, string>,
  databases: readonly string[],
  contextPath: readonly string[],
): PathHierarchySelectorSegment[] {
  const segments = buildPathHierarchySelectorSegments(
    namespaceTree,
    pathAliases,
    databases,
    contextPath,
  );
  return segments.length > 0 ? segments : PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS;
}
