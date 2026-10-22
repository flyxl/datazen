/**
 * SQL-ish column type families for themed UI (--dt-* tokens in themes.css).
 * Shared by DataTable CellRenderer, StructureView, DetailPanel, etc.
 *
 * The families are a **projection** of the normalised cell type
 * (`src/lib/cellTypes.ts`), not a second classification: the host has one
 * type judgement, and the colour is derived from it. The signature of
 * {@link classifyDataType} did not change — only where the answer comes from.
 */
import { cellTypeToDataTypeFamily, resolveCellType } from './cellTypes';

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
  return cellTypeToDataTypeFamily(resolveCellType(dataType));
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
