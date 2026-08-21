import type { ColumnInfo, QueryResult, Value } from '../types';

const DEFAULT_PROCESS_COLUMNS: ColumnInfo[] = [
  { name: 'pid', dataType: 'integer', nullable: false },
  { name: 'user', dataType: 'string', nullable: true },
  { name: 'database', dataType: 'string', nullable: true },
  { name: 'state', dataType: 'string', nullable: true },
  { name: 'query', dataType: 'string', nullable: true },
  { name: 'durationMs', dataType: 'integer', nullable: true },
];

function isQueryResultShape(data: unknown): data is QueryResult {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return false;
  const record = data as Record<string, unknown>;
  return Array.isArray(record.columns) && Array.isArray(record.rows);
}

function processRecordToRow(record: Record<string, unknown>): (Value | null)[] {
  return DEFAULT_PROCESS_COLUMNS.map((col) => {
    const raw = record[col.name];
    if (raw == null) return null;
    if (typeof raw === 'string' || typeof raw === 'number' || typeof raw === 'boolean') {
      return raw;
    }
    return String(raw);
  });
}

/** Normalize list_processes command output into a QueryResult for DataTable. */
export function commandResultRows(data: unknown): QueryResult {
  if (isQueryResultShape(data)) {
    return data;
  }
  if (data && typeof data === 'object' && !Array.isArray(data)) {
    const record = data as Record<string, unknown>;
    const processes = record.processes;
    if (Array.isArray(processes)) {
      const rows = processes
        .filter((item): item is Record<string, unknown> => !!item && typeof item === 'object')
        .map(processRecordToRow);
      return {
        columns: DEFAULT_PROCESS_COLUMNS,
        rows,
        executionTimeMs: 0,
      };
    }
  }
  return { columns: DEFAULT_PROCESS_COLUMNS, rows: [], executionTimeMs: 0 };
}

export function commandResultColumns(
  fallback: ColumnInfo[] = DEFAULT_PROCESS_COLUMNS,
): ColumnInfo[] {
  return fallback.length > 0 ? fallback : DEFAULT_PROCESS_COLUMNS;
}
