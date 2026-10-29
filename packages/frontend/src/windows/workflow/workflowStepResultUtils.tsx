import type { ColumnInfo, StatementResult, StepExecutionResult, Value } from '../../types';

export function StepStatusIcon({ status }: { status: string }) {
  if (status === 'success') return <span className="text-green-500">✓</span>;
  if (status === 'skipped') return <span className="text-yellow-500">⏭</span>;
  if (status === 'timed_out') return <span className="text-yellow-500">⏱</span>;
  return <span className="text-red-400">✗</span>;
}

export function extractStepColumnNames(
  stepResult: Record<string, unknown> | undefined,
): { name: string; dataType?: string }[] {
  if (!stepResult) return [];
  const cols = stepResult.columns as { name: string; dataType?: string }[] | undefined;
  if (Array.isArray(cols) && cols.length > 0) return cols;
  const rows = stepResult.rows as Record<string, unknown>[] | undefined;
  if (
    Array.isArray(rows) &&
    rows.length > 0 &&
    typeof rows[0] === 'object' &&
    rows[0] !== null &&
    !Array.isArray(rows[0])
  ) {
    return Object.keys(rows[0]).map((k) => ({ name: k }));
  }
  return [];
}

export function stepToStatementResult(step: StepExecutionResult): StatementResult | null {
  const r = step.result;
  if (!r?.rows) return null;
  const cols = extractStepColumnNames(r);
  if (cols.length === 0) return null;
  const columnInfos: ColumnInfo[] = cols.map((c) => ({
    name: c.name,
    dataType: c.dataType || 'text',
    nullable: true,
  }));
  const rawRows = r.rows as unknown[];
  let rows: (Value | null)[][];
  if (
    rawRows.length > 0 &&
    typeof rawRows[0] === 'object' &&
    rawRows[0] !== null &&
    !Array.isArray(rawRows[0])
  ) {
    rows = (rawRows as Record<string, unknown>[]).map((obj) =>
      cols.map((c) => (obj[c.name] ?? null) as Value | null),
    );
  } else {
    rows = rawRows as (Value | null)[][];
  }
  return {
    sql: step.sqlExecuted ?? '',
    columns: columnInfos,
    rows,
    executionTimeMs: (r.execution_time_ms as number) ?? step.executionTimeMs,
  };
}
