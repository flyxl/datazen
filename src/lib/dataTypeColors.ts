/**
 * SQL-ish column type families for themed UI (--dt-* tokens in themes.css).
 * Shared by DataTable CellRenderer, StructureView, DetailPanel, etc.
 *
 * The families are a **projection** of the normalised cell type
 * (`src/lib/cellTypes.ts`), not a second classification: the host has one
 * type judgement, and the colour is derived from it. The signature of
 * {@link classifyDataType} did not change — only where the answer comes from.
 */
import {
  cellTypeToDataTypeFamily,
  resolveCellType,
  type CellType,
  type CellTypeHints,
} from './cellTypes';

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

/**
 * Classify a driver-reported column type string (case-insensitive).
 *
 * `hints` is the **driver's own** type declaration — the same one the
 * editor reads (`DatabaseTypeMeta.cellTypes`). It is optional, and a caller
 * that does not pass it gets exactly the behaviour it had before: the
 * shared table alone decides (00-contracts R4).
 *
 * Passing it is what keeps the colour and the editor from giving two
 * answers: a MySQL `tinyint(1)` is a boolean for the editor, so it has to be
 * a boolean on screen too (05 §3.4).
 */
export function classifyDataType(
  dataType: string | undefined,
  hints?: CellTypeHints,
): DataTypeFamily {
  return cellTypeToDataTypeFamily(resolveCellType(dataType, hints));
}

/** Tailwind class for a type label (StructureView, ExportDialog, …). */
export function dataTypeTextClass(dataType: string | undefined, hints?: CellTypeHints): string {
  return FAMILY_CLASS[classifyDataType(dataType, hints)];
}

/** Tailwind class for a cell value given its column type (DataTable). */
export function cellValueTextClass(
  dataType: string | undefined,
  value: unknown,
  hints?: CellTypeHints,
): string {
  if (value === null || value === undefined) return FAMILY_CLASS.null;
  return FAMILY_CLASS[classifyDataType(dataType, hints)];
}

/**
 * Tailwind class for a cell value judged by the **normalised** cell type.
 *
 * The grid already knows the normalised type (the owner computed it once,
 * with the driver's hints), so the colour reads it instead of re-deriving
 * it from the type name: the cell and its editor then cannot disagree.
 */
export function cellValueTextClassFor(cellType: CellType, value: unknown): string {
  if (value === null || value === undefined) return FAMILY_CLASS.null;
  return FAMILY_CLASS[cellTypeToDataTypeFamily(cellType)];
}
