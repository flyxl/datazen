import type { DataSyncTableResult } from '../../commands/sync';
import {
  displayTableName,
  rowDiffCounts,
  summarizeCompare,
  type CompareSummaryStats,
} from './mappingView';

export function buildCompareReportText(
  rows: DataSyncTableResult[],
  stats?: CompareSummaryStats,
): string {
  const summary = stats ?? summarizeCompare(rows);
  const lines: string[] = [
    'Data Sync Compare Report',
    `Inserts: ${summary.inserts}`,
    `Updates: ${summary.updates}`,
    `Deletes: ${summary.deletes}`,
    `Unchanged tables: ${summary.unchangedTables}`,
    `Incompatible tables: ${summary.incompatible}`,
    '',
    'Tables:',
  ];

  for (const row of rows) {
    const name = displayTableName(row);
    if (row.status === 'INCOMPATIBLE') {
      lines.push(
        `- ${name}: INCOMPATIBLE${row.incompatibleReason ? ` — ${row.incompatibleReason}` : ''}`,
      );
      continue;
    }
    if (row.status !== 'MATCHED') {
      lines.push(`- ${name}: ${row.status}`);
      continue;
    }
    const counts = rowDiffCounts(row);
    lines.push(
      `- ${name}: +${counts.inserts} / ~${counts.updates} / −${counts.deletes} (${counts.unchanged} unchanged rows)`,
    );
  }

  return lines.join('\n');
}

export function buildCompareReportJson(
  rows: DataSyncTableResult[],
  stats?: CompareSummaryStats,
): string {
  const summary = stats ?? summarizeCompare(rows);
  const payload = {
    summary,
    tables: rows.map((row) => {
      if (row.status === 'INCOMPATIBLE') {
        return {
          name: displayTableName(row),
          status: row.status,
          incompatibleReason: row.incompatibleReason ?? null,
        };
      }
      if (row.status !== 'MATCHED') {
        return { name: displayTableName(row), status: row.status };
      }
      const counts = rowDiffCounts(row);
      return {
        name: displayTableName(row),
        status: row.status,
        inserts: counts.inserts,
        updates: counts.updates,
        deletes: counts.deletes,
        unchangedRows: counts.unchanged,
      };
    }),
  };
  return JSON.stringify(payload, null, 2);
}
