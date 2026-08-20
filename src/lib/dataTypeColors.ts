/**
 * SQL-ish column type families for themed UI (--dt-* tokens in themes.css).
 * Shared by DataTable CellRenderer, StructureView, DetailPanel, etc.
 */

export type DataTypeFamily = 'null' | 'bool' | 'number' | 'datetime' | 'json' | 'binary' | 'text';

const FAMILY_CLASS: Record<DataTypeFamily, string> = {
  null: 'text-dt-null',
  bool: 'text-dt-bool',
  number: 'text-dt-number',
  datetime: 'text-dt-datetime',
  json: 'text-dt-json',
  binary: 'text-dt-binary',
  text: 'text-dt-text',
};

/** Classify a driver-reported column type string (case-insensitive). */
export function classifyDataType(dataType: string | undefined): DataTypeFamily {
  const type = (dataType ?? '').toLowerCase();
  if (!type) return 'text';
  if (type.includes('bool')) return 'bool';
  if (
    type.includes('int') ||
    type.includes('serial') ||
    type.includes('double') ||
    type.includes('numeric') ||
    type.includes('decimal') ||
    type.includes('real') ||
    type.includes('float')
  ) {
    return 'number';
  }
  if (
    type.includes('timestamp') ||
    type.includes('date') ||
    type.includes('time') ||
    type.includes('interval')
  ) {
    return 'datetime';
  }
  if (type.includes('json')) return 'json';
  if (type.includes('bytea') || type.includes('blob') || type.includes('binary')) return 'binary';
  return 'text';
}

/** Tailwind class for a type label (StructureView, ExportDialog, …). */
export function dataTypeTextClass(dataType: string | undefined): string {
  return FAMILY_CLASS[classifyDataType(dataType)];
}

/** Tailwind class for a cell value given its column type (DataTable). */
export function cellValueTextClass(dataType: string | undefined, value: unknown): string {
  if (value === null || value === undefined) return FAMILY_CLASS.null;
  return FAMILY_CLASS[classifyDataType(dataType)];
}
