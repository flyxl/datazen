import { extractQualifiedToken } from './sqlPathPrefix';
import { namespaceBranchChildNames, namespaceHasChild, type SqlNamespace } from './sqlNamespace';

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

/**
 * Convert a selector path rooted at the displayed connection database into
 * the catalog/schema path expected after the driver database id.
 *
 * Tree validation disambiguates the case where a database and its catalog
 * share the same name: `['hive', 'snap']` remains a catalog/schema path, while
 * `['hive', 'hive', 'snap']` loses only the first (database) segment.
 */
export function pathHierarchyRelativeNamespacePath(
  databases: readonly string[],
  namespaceTree: SqlNamespace,
  namespacePath: readonly string[],
): string[] {
  const first = namespacePath[0];
  if (!first || !databases.includes(first)) return [...namespacePath];

  const isRootedTreePath = namespaceHasChild(namespaceTree, [...namespacePath]);
  const repeatsDatabaseAsCatalog = namespacePath[1] === first;
  return isRootedTreePath || repeatsDatabaseAsCatalog ? namespacePath.slice(1) : [...namespacePath];
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

/**
 * Connection-level database for path-hierarchy pin building.
 * Resolves namespace display names through path aliases instead of treating
 * them as connection-level databases (for example `hive` → `558`).
 */
export function pathHierarchyConnectionRoot(
  databases: readonly string[],
  panelDatabase: string | undefined | null,
  currentDatabase: string | null,
  pathAliases: Readonly<Record<string, string>> = {},
  namespaceRoots: readonly string[] = [],
): string | null {
  const namespaceRootSet = new Set(namespaceRoots);
  const hasAlias = (value: string): boolean =>
    Object.prototype.hasOwnProperty.call(pathAliases, value);

  const knownDatabaseFor = (root: string): string | null => {
    const trimmed = root.trim();
    if (!trimmed) return null;
    return (
      databases.find((database) => database === trimmed || database.split(':', 1)[0] === trimmed) ??
      null
    );
  };

  const resolveCandidate = (value: string | undefined | null): string | null => {
    const trimmed = value?.trim();
    if (!trimmed) return null;

    // currentDatabase may already be a complete fetch path. Only its first
    // component is the connection root; the remaining components are the
    // namespace path and must not be prepended again.
    const { root } = splitPathHierarchyDatabasePin(trimmed);
    const alias = pathAliases[root]?.trim();
    if (namespaceRootSet.has(root) || hasAlias(root)) {
      if (alias) return knownDatabaseFor(alias) ?? alias;
      return knownDatabaseFor(root);
    }

    const known = knownDatabaseFor(root);
    if (known) return known;
    if (/^\d+$/.test(root)) return root;
    // An id:name root is valid while the database list is still loading.
    if (databases.length === 0 && root.includes(':')) return root;
    return null;
  };

  const panelRoot = resolveCandidate(panelDatabase);
  if (panelRoot) return panelRoot;

  const currentRoot = resolveCandidate(currentDatabase);
  if (currentRoot) return currentRoot;

  for (const database of databases) {
    const root = resolveCandidate(database);
    if (root) return root;
  }
  return null;
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
    if (children.length === 0) {
      const nextIndex = path.length;
      const alreadyHasNext = segments.some(
        (segment) => segment.kind === 'select' && segment.levelIndex === nextIndex,
      );
      const showLoadingPlaceholder =
        !alreadyHasNext && path.length > 0 && path.length < 2 && nextIndex <= contextPath.length;
      if (showLoadingPlaceholder) {
        segments.push({
          kind: 'select',
          levelIndex: nextIndex,
          options: [],
          value: contextPath[nextIndex] ?? '',
        });
      }
      break;
    }
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
