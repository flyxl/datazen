export type SqlNamespace = { [name: string]: SqlNamespace } | readonly string[];

export type NamespaceMergeKind = 'branch' | 'tables';

export function pathKey(segments: string[]): string {
  return segments.join('/');
}

function isLeaf(node: SqlNamespace): node is readonly string[] {
  return Array.isArray(node);
}

function asBranch(node: SqlNamespace | undefined): Record<string, SqlNamespace> {
  if (!node || isLeaf(node)) return {};
  return { ...node };
}

export function mergeNamespacePath(
  tree: SqlNamespace,
  segments: string[],
  kind: NamespaceMergeKind,
  names: string[],
): SqlNamespace {
  const root = asBranch(tree);

  const setAt = (
    node: Record<string, SqlNamespace>,
    segs: string[],
  ): Record<string, SqlNamespace> => {
    if (segs.length === 0) {
      const next = { ...node };
      for (const name of names) {
        if (kind === 'tables') {
          next[name] = Array.isArray(next[name]) ? next[name] : [];
        } else if (!(name in next) || Array.isArray(next[name])) {
          next[name] = asBranch(next[name]);
        }
      }
      return next;
    }
    const [head, ...rest] = segs;
    const child = asBranch(node[head]);
    return { ...node, [head]: setAt(child, rest) };
  };

  return setAt(root, segments);
}

export function namespaceHasChild(tree: SqlNamespace, segments: string[]): boolean {
  if (segments.length === 0) return true;
  let node: SqlNamespace = tree;
  for (const seg of segments) {
    if (isLeaf(node)) return false;
    if (!(seg in node)) return false;
    node = node[seg];
  }
  return true;
}

/** Deep-clone tree and replace table leaves whose names appear in columnMap. */
export function overlayColumnMap(
  tree: SqlNamespace,
  columnMap: Record<string, string[]>,
): SqlNamespace {
  if (isLeaf(tree)) return tree;
  const out: Record<string, SqlNamespace> = {};
  for (const [key, child] of Object.entries(tree)) {
    if (isLeaf(child)) {
      out[key] = columnMap[key] ?? [...child];
    } else {
      out[key] = overlayColumnMap(child, columnMap);
    }
  }
  return out;
}
