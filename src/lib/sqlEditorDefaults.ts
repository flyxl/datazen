import type { TableInfo } from '../types';
import { isSchemaGroupingSchema } from './sqlNamespace';

const FROM_RELATION =
  /\b(?:from|join)\s+(?:only\s+)?((?:"[^"]+"|`[^`]+`|[A-Za-z_][\w$]*)(?:\s*\.\s*(?:"[^"]+"|`[^`]+`|[A-Za-z_][\w$]*))*)/gi;

function unquoteIdent(raw: string): string {
  const trimmed = raw.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith('`') && trimmed.endsWith('`')) ||
    (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function lastRelationSegment(qualified: string): string | undefined {
  const parts = qualified.split('.').map(unquoteIdent).filter(Boolean);
  return parts[parts.length - 1];
}

/** Table names referenced by FROM/JOIN (last segment of each qualified path). */
export function tablesReferencedInSql(sql: string): string[] {
  const names: string[] = [];
  const seen = new Set<string>();
  FROM_RELATION.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = FROM_RELATION.exec(sql)) !== null) {
    const table = lastRelationSegment(match[1] ?? '');
    if (table && !seen.has(table)) {
      seen.add(table);
      names.push(table);
    }
  }
  return names;
}

/**
 * Last FROM/JOIN relation in `sql`. For `schema.table` / `catalog.schema.table`,
 * returns the table segment — CodeMirror resolves it via `defaultSchema` + aliases.
 */
export function inferDefaultTable(sql: string): string | undefined {
  const names = tablesReferencedInSql(sql);
  return names[names.length - 1];
}

/** Prefer `public`, otherwise the most common real schema on loaded tables. */
export function inferDefaultSchema(
  tables: TableInfo[],
  views: TableInfo[] = [],
): string | undefined {
  const counts = new Map<string, number>();
  for (const item of [...tables, ...views]) {
    if (!isSchemaGroupingSchema(item.schema)) continue;
    counts.set(item.schema!, (counts.get(item.schema!) ?? 0) + 1);
  }
  if (counts.size === 0) return undefined;
  if (counts.has('public')) return 'public';
  return [...counts.entries()].sort((a, b) => b[1] - a[1])[0]![0];
}
