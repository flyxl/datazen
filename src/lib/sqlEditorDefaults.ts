import type { TableInfo } from '../types';
import { isSchemaGroupingSchema } from './sqlNamespace';

const FROM_TABLE =
  /\b(?:from|join)\s+(?:only\s+)?(?:"([^"]+)"|([A-Za-z_][\w$]*))(?:\s*\.\s*(?:"([^"]+)"|([A-Za-z_][\w$]*)))?/gi;

/**
 * Last FROM/JOIN relation in `sql`. For `schema.table`, returns the table
 * segment — CodeMirror resolves it via `defaultSchema` + aliases.
 */
export function inferDefaultTable(sql: string): string | undefined {
  let table: string | undefined;
  FROM_TABLE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = FROM_TABLE.exec(sql)) !== null) {
    table = match[3] || match[4] || match[1] || match[2];
  }
  return table || undefined;
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
