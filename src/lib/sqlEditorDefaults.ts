import type { TableInfo } from '../types';
import { isSchemaGroupingSchema } from './sqlNamespace';
import { scanSql } from '../components/sql-editor/semantic/scanner';
import { buildStatementRanges } from '../components/sql-editor/semantic/statementRanges';
import { buildScopesForStatement } from '../components/sql-editor/semantic/scope/builder';
import { getDialectAdapter } from '../components/sql-editor/semantic/dialectAdapter';

/**
 * Extract physical table/view names referenced across all scopes and statements
 * in the provided SQL text, using lexical and semantic scope analysis.
 *
 * Filters out CTE and subquery aliases, preserving appearance order.
 */
export function tablesReferencedInSql(sql: string, dialectId?: string): string[] {
  if (!sql.trim()) return [];

  const adapter = getDialectAdapter(dialectId ?? 'standard');
  const { tokens } = scanSql(sql);
  const ranges = buildStatementRanges(sql);
  const effectiveRanges =
    ranges.length > 0
      ? ranges
      : [
          {
            contentFrom: 0,
            contentTo: sql.length,
            index: 0,
            from: 0,
            to: sql.length,
            delimiterFrom: null,
            delimiterTo: null,
            firstExecutableLine: 0,
            confidence: 'degraded' as const,
          },
        ];

  const names: string[] = [];
  const seen = new Set<string>();

  for (const range of effectiveRanges) {
    const stmtRange = { from: range.contentFrom, to: range.contentTo };
    let scopes: ReturnType<typeof buildScopesForStatement> = [];
    try {
      scopes = buildScopesForStatement(tokens, stmtRange, adapter, sql);
    } catch {
      continue;
    }

    const allRelations: { name: string; from: number }[] = [];
    for (const scope of scopes) {
      for (const rel of scope.relations) {
        if (rel.sourceKind === 'table' || rel.sourceKind === 'view') {
          const name = rel.relation.name.name;
          if (name) {
            allRelations.push({ name, from: rel.sourceRange.from });
          }
        }
      }
    }
    allRelations.sort((a, b) => a.from - b.from);

    for (const item of allRelations) {
      if (!seen.has(item.name)) {
        seen.add(item.name);
        names.push(item.name);
      }
    }
  }

  return names;
}

/**
 * Last FROM/JOIN relation in `sql`. For `schema.table` / `catalog.schema.table`,
 * returns the table segment — CodeMirror resolves it via `defaultSchema` + aliases.
 */
export function inferDefaultTable(sql: string, dialectId?: string): string | undefined {
  const names = tablesReferencedInSql(sql, dialectId);
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
