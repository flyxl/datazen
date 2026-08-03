import type { ColumnInfo, StatementResult, Value } from '../../types';
import type { ChartField, InferredFieldType } from '../../types/chart';

export function inferFieldType(column: ColumnInfo, sampleValues: (Value | null)[]): InferredFieldType {
  const dt = column.dataType.toLowerCase();

  if (/bool/.test(dt)) return 'boolean';
  if (/int|serial|double|numeric|decimal|real|float|money|bigint|smallint/.test(dt)) return 'numeric';
  if (/timestamp|date|time/.test(dt)) return 'datetime';

  const nonNull = sampleValues.filter((v) => v != null);
  if (nonNull.length === 0) return 'unknown';

  if (nonNull.every((v) => typeof v === 'number')) return 'numeric';
  if (nonNull.every((v) => typeof v === 'boolean')) return 'boolean';
  if (nonNull.every((v) => typeof v === 'string' && isDateLike(v as string))) return 'datetime';

  return 'categorical';
}

function isDateLike(s: string): boolean {
  return /^\d{4}-\d{2}-\d{2}/.test(s) || (!isNaN(Date.parse(s)) && s.length > 4);
}

export function inferAllFields(result: StatementResult): ChartField[] {
  const sampleSize = Math.min(result.rows.length, 50);
  return result.columns.map((col, colIdx) => {
    const samples = result.rows.slice(0, sampleSize).map((row) => row[colIdx]);
    const distinctValues = new Set(samples.filter((v) => v != null).map(String));
    return {
      name: col.name,
      dataType: col.dataType,
      inferredType: inferFieldType(col, samples),
      distinctCount: distinctValues.size,
      sampleValues: Array.from(distinctValues).slice(0, 5),
    };
  });
}

export function isChartableResult(result: StatementResult): boolean {
  if (result.rows.length === 0 || result.columns.length === 0) return false;
  const fields = inferAllFields(result);
  return fields.some((f) => f.inferredType === 'numeric');
}
