import type { ColumnInfo, QueryResult, Value } from '../types';
import type { ColumnDef } from '../components/DataTable/TableHeader';
import type { I18nKey } from '../locales';

const DEFAULT_PROCESS_COLUMNS: ColumnInfo[] = [
  { name: 'pid', dataType: 'integer', nullable: false },
  { name: 'user', dataType: 'string', nullable: true },
  { name: 'database', dataType: 'string', nullable: true },
  { name: 'state', dataType: 'string', nullable: true },
  { name: 'query', dataType: 'string', nullable: true },
  { name: 'durationMs', dataType: 'integer', nullable: true },
  { name: 'clientIp', dataType: 'string', nullable: true },
];

/** Map process-list column keys to localized header keys. */
const PROCESS_COLUMN_LABELS: Record<string, I18nKey> = {
  pid: 'processList.colPid',
  user: 'processList.colUser',
  database: 'processList.colDatabase',
  state: 'processList.colState',
  query: 'processList.colQuery',
  durationMs: 'processList.colDuration',
  clientIp: 'processList.colClientIp',
};

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

/** Map column metadata into DataTable column definitions with localized headers. */
export function commandResultColumns(
  fallback: ColumnInfo[] = DEFAULT_PROCESS_COLUMNS,
  t?: (key: I18nKey) => string,
): ColumnDef[] {
  const columns = fallback.length > 0 ? fallback : DEFAULT_PROCESS_COLUMNS;
  return columns.map((c) => {
    const labelKey = PROCESS_COLUMN_LABELS[c.name];
    return {
      id: c.name,
      name: labelKey ? (t ? t(labelKey) : c.name) : c.name,
      type: c.dataType,
    };
  });
}
