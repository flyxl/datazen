import { scanSql } from './scanner';
import { buildStatementRanges } from './statementRanges';
import { buildScopesForStatement } from './scope/builder';
import { getDialectAdapter } from './dialectAdapter';

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

  const seen = new Set<string>();
  const results: string[] = [];

  for (const range of effectiveRanges) {
    const statementTokens = tokens.filter(
      (t) => t.from >= range.contentFrom && t.to <= range.contentTo,
    );
    if (statementTokens.length === 0) continue;

    const scopes = buildScopesForStatement(statementTokens, range, adapter, sql);
    for (const scope of scopes) {
      for (const rel of scope.relations) {
        if (rel.sourceKind === 'cte' || rel.sourceKind === 'subquery') continue;

        const name = rel.relation.name.name;
        if (!name) continue;

        const dedupKey = name.toLowerCase();
        if (!seen.has(dedupKey)) {
          seen.add(dedupKey);
          results.push(name);
        }
      }
    }
  }

  return results;
}
