import type { NativeMenuItemDef } from './nativeContextMenu';
import type { DatabaseType } from '../types';
import { escapeIdent, getEscapeSqlValue } from './databaseTypes';

/** Caller-supplied labels (typically from `t()`). */
export type DataTableContextMenuLabels = {
  copy: string;
  copyRow: string;
  moreActions: string;
  copyAsJson: string;
  copyAsSqlInsert: string;
  copyAsUpdate: string;
  copyAsCsv: string;
  copyColumnName: string;
  copyColumnData: string;
  setNull: string;
  filterByValue: string;
  copySelectedRows: string;
  deleteRow: string;
  export: string;
  /** "Restore This Row" — offered on a row that is marked for deletion (DB-06). */
  restoreRow: string;
};

export type DataTableContextMenuHandlers = {
  onCopy?: () => void;
  onCopyRow?: () => void;
  onCopyAsJson?: () => void;
  onCopyAsSqlInsert?: () => void;
  onCopyAsUpdate?: () => void;
  onCopyAsCsv?: () => void;
  onCopyColumnName?: () => void;
  onCopyColumnData?: () => void;
  onSetNull?: () => void;
  onFilterByValue?: () => void;
  onCopySelectedRows?: () => void;
  onDeleteRow?: () => void;
  onExport?: () => void;
  /** Undo the delete mark of the right-clicked row (DB-06). */
  onRestoreRow?: () => void;
};

export type BuildDataTableContextMenuArgs = {
  labels: DataTableContextMenuLabels;
  handlers: DataTableContextMenuHandlers;
  /** Right-clicked a data cell (row+column resolved). */
  hasCellContext?: boolean;
  /** Right-clicked a column header (column resolved). */
  hasHeaderContext?: boolean;
  /** One or more rows are selected. */
  hasSelectedRows?: boolean;
  /**
   * The menu targets the whole row selection rather than the right-clicked
   * cell: several rows are selected and the click landed inside that selection.
   * Single-cell / single-row actions are then omitted, because they cannot act
   * on more than one row.
   */
  multiRowSelection?: boolean;
  /** Export action available. */
  exportEnabled?: boolean;
  /** Filter-by-value available (needs cell context + filter handler). */
  canFilterByValue?: boolean;
  /** Set NULL available (needs cell context + edit handler). */
  canSetNull?: boolean;
  /** Delete row available (needs PK + delete handler). */
  canDelete?: boolean;
  /**
   * The right-clicked row is marked for deletion and can be restored (DB-06).
   * The capability bit defaults to false, so a grid without the dirty state
   * keeps today's menu byte for byte.
   */
  canRestoreRow?: boolean;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

function submenu(id: string, label: string, items: NativeMenuItemDef[]): NativeMenuItemDef | null {
  return items.length > 0 ? { kind: 'submenu', id, label, items } : null;
}

function escapeCsvCell(value: unknown): string {
  if (value === null || value === undefined) return '';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  if (str.includes(',') || str.includes('"') || str.includes('\n') || str.includes('\r')) {
    return `"${str.replaceAll('"', '""')}"`;
  }
  return str;
}

/** Serialize table rows to TSV (tab-separated cells, newline-separated rows). */
export function serializeDataTableRowsAsTsv(rows: unknown[][]): string {
  return rows
    .map((row) => row.map((cell) => (cell == null ? '' : String(cell))).join('\t'))
    .join('\n');
}

/** Serialize rows as CSV with a header line. */
export function serializeDataTableRowsAsCsv(columnNames: string[], rows: unknown[][]): string {
  const header = columnNames.map(escapeCsvCell).join(',');
  const body = rows.map((row) => columnNames.map((_, i) => escapeCsvCell(row[i])).join(','));
  return [header, ...body].join('\n');
}

/** Serialize rows as a pretty-printed JSON array of named records. */
export function serializeDataTableRowsAsJson(columnNames: string[], rows: unknown[][]): string {
  return JSON.stringify(
    rows.map((row) => rowToNamedRecord(columnNames, row)),
    null,
    2,
  );
}

/** Build a JSON object row from column names + cell values. */
export function rowToNamedRecord(columnNames: string[], row: unknown[]): Record<string, unknown> {
  const obj: Record<string, unknown> = {};
  for (let i = 0; i < columnNames.length; i++) {
    const name = columnNames[i];
    if (name) obj[name] = row[i] ?? null;
  }
  return obj;
}

function escapeSqlValue(value: unknown, dialect?: DatabaseType): string {
  return getEscapeSqlValue(dialect)(value);
}

function quoteIdent(name: string, dialect?: DatabaseType): string {
  return escapeIdent(name, dialect);
}

/** Single-row SQL INSERT (identifiers quoted per dialect). */
export function formatRowAsSqlInsert(
  tableName: string,
  columnNames: string[],
  row: unknown[],
  dialect?: DatabaseType,
): string {
  const cols = columnNames.map((c) => quoteIdent(c, dialect)).join(', ');
  const values = columnNames.map((_, i) => escapeSqlValue(row[i], dialect)).join(', ');
  const table = quoteIdent(tableName, dialect);
  return `INSERT INTO ${table} (${cols}) VALUES (${values});`;
}

/**
 * Single-row SQL UPDATE. WHERE uses primary-key columns when provided,
 * otherwise the first column.
 */
export function formatRowAsSqlUpdate(
  tableName: string,
  columnNames: string[],
  row: unknown[],
  primaryKeyColumns?: string[],
  dialect?: DatabaseType,
): string {
  const pkNames =
    primaryKeyColumns && primaryKeyColumns.length > 0
      ? primaryKeyColumns.filter((c) => columnNames.includes(c))
      : columnNames.slice(0, 1);
  const whereKeys = pkNames.length > 0 ? pkNames : columnNames.slice(0, 1);
  const whereSet = new Set(whereKeys);
  let setCols = columnNames.filter((c) => !whereSet.has(c));
  if (setCols.length === 0) {
    setCols = columnNames.slice();
  }

  const setClauses = setCols.map((col) => {
    const idx = columnNames.indexOf(col);
    return `${quoteIdent(col, dialect)} = ${escapeSqlValue(row[idx], dialect)}`;
  });
  const where = whereKeys
    .map((col) => {
      const idx = columnNames.indexOf(col);
      return `${quoteIdent(col, dialect)} = ${escapeSqlValue(row[idx], dialect)}`;
    })
    .join(' AND ');

  return `UPDATE ${quoteIdent(tableName, dialect)} SET ${setClauses.join(', ')} WHERE ${where};`;
}

/**
 * Single-row SQL DELETE. WHERE uses primary-key columns when provided,
 * otherwise the first column. Null PK values use `IS NULL`.
 */
export function formatRowAsSqlDelete(
  tableName: string,
  columnNames: string[],
  row: unknown[],
  primaryKeyColumns?: string[],
  dialect?: DatabaseType,
): string {
  const pkNames =
    primaryKeyColumns && primaryKeyColumns.length > 0
      ? primaryKeyColumns.filter((c) => columnNames.includes(c))
      : columnNames.slice(0, 1);
  const whereKeys = pkNames.length > 0 ? pkNames : columnNames.slice(0, 1);
  const where = whereKeys
    .map((col) => {
      const idx = columnNames.indexOf(col);
      const val = row[idx];
      if (val === null || val === undefined) {
        return `${quoteIdent(col, dialect)} IS NULL`;
      }
      return `${quoteIdent(col, dialect)} = ${escapeSqlValue(val, dialect)}`;
    })
    .join(' AND ');

  return `DELETE FROM ${quoteIdent(tableName, dialect)} WHERE ${where};`;
}

/** One `INSERT` statement per row (joined by newlines). */
export function formatRowsAsSqlInsert(
  tableName: string,
  columnNames: string[],
  rows: unknown[][],
  dialect?: DatabaseType,
): string {
  return rows.map((row) => formatRowAsSqlInsert(tableName, columnNames, row, dialect)).join('\n');
}

/** One `UPDATE` statement per row (joined by newlines). */
export function formatRowsAsSqlUpdate(
  tableName: string,
  columnNames: string[],
  rows: unknown[][],
  primaryKeyColumns?: string[],
  dialect?: DatabaseType,
): string {
  return rows
    .map((row) => formatRowAsSqlUpdate(tableName, columnNames, row, primaryKeyColumns, dialect))
    .join('\n');
}

/**
 * Resolve which cell was targeted by a contextmenu event.
 * Expects VirtualBody cells to set `data-dt-row` / `data-dt-col`.
 */
export function resolveDataTableCellFromEvent(
  target: EventTarget | null,
): { rowIndex: number; columnName: string } | null {
  if (!(target instanceof Element)) return null;
  const el = target.closest('[data-dt-row][data-dt-col]');
  if (!el) return null;
  const rowRaw = el.getAttribute('data-dt-row');
  const columnName = el.getAttribute('data-dt-col');
  if (rowRaw == null || !columnName) return null;
  const rowIndex = Number(rowRaw);
  if (!Number.isInteger(rowIndex) || rowIndex < 0) return null;
  return { rowIndex, columnName };
}

/**
 * Resolve which column header was targeted by a contextmenu event.
 * Expects TableHeader cells to set `data-col-header`.
 */
export function resolveDataTableHeaderColFromEvent(target: EventTarget | null): string | null {
  if (!(target instanceof Element)) return null;
  const el = target.closest('[data-col-header]');
  return el ? el.getAttribute('data-col-header') : null;
}

/**
 * Serialize an entire column's data as newline-separated text.
 */
export function serializeDataTableColumnValues(columnIndex: number, rows: unknown[][]): string {
  if (columnIndex < 0) return '';
  return rows
    .map((row) => {
      if (!Array.isArray(row)) return '';
      const cell = row[columnIndex];
      return cell == null ? '' : typeof cell === 'object' ? JSON.stringify(cell) : String(cell);
    })
    .join('\n');
}

/**
 * TablePlus-style DataTable context menu.
 * With cell context, keep the frequent actions at the root and group the
 * lower-frequency copy/NULL actions under a submenu. Never emit a lonely
 * “Export” item when cell context exists.
 */
export function buildDataTableContextMenuItems(
  args: BuildDataTableContextMenuArgs,
): NativeMenuItemDef[] {
  const {
    labels,
    handlers,
    hasCellContext = false,
    hasHeaderContext = false,
    hasSelectedRows = false,
    multiRowSelection = false,
    exportEnabled = false,
    canFilterByValue = false,
    canSetNull = false,
    canDelete = false,
    canRestoreRow = false,
  } = args;

  if (hasHeaderContext && !hasCellContext) {
    const headerItems = push(
      item('copy-column-name', labels.copyColumnName, handlers.onCopyColumnName),
      item('copy-column-data', labels.copyColumnData, handlers.onCopyColumnData),
    );
    if (exportEnabled) {
      const exportItem = item('export', labels.export, handlers.onExport);
      if (exportItem) {
        return headerItems.length > 0
          ? [...headerItems, { kind: 'separator' }, exportItem]
          : [exportItem];
      }
    }
    return headerItems;
  }

  if (hasCellContext && !multiRowSelection) {
    const frequent = push(
      item('copy', labels.copy, handlers.onCopy),
      item('copy-row', labels.copyRow, handlers.onCopyRow),
      canFilterByValue
        ? item('filter-by-value', labels.filterByValue, handlers.onFilterByValue)
        : null,
    );

    const danger = canDelete
      ? push(item('delete-row', labels.deleteRow, handlers.onDeleteRow))
      : [];

    // "Restore This Row" belongs next to "Delete Row": both are about the
    // deletion state of the row under the pointer.
    const restore = canRestoreRow
      ? push(item('restore-row', labels.restoreRow, handlers.onRestoreRow))
      : [];

    const moreActions = push(
      item('copy-as-json', labels.copyAsJson, handlers.onCopyAsJson),
      item('copy-as-sql-insert', labels.copyAsSqlInsert, handlers.onCopyAsSqlInsert),
      item('copy-as-update', labels.copyAsUpdate, handlers.onCopyAsUpdate),
      item('copy-as-csv', labels.copyAsCsv, handlers.onCopyAsCsv),
      item('copy-column-name', labels.copyColumnName, handlers.onCopyColumnName),
      item('copy-column-data', labels.copyColumnData, handlers.onCopyColumnData),
      canSetNull ? item('set-null', labels.setNull, handlers.onSetNull) : null,
      hasSelectedRows
        ? item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows)
        : null,
    );

    let out = frequent;
    const dangerZone = [...danger, ...restore];
    if (dangerZone.length > 0) {
      out = out.length > 0 ? [...out, { kind: 'separator' }, ...dangerZone] : dangerZone;
    }
    if (exportEnabled) {
      const exportItem = item('export', labels.export, handlers.onExport);
      if (exportItem) {
        out = out.length > 0 ? [...out, { kind: 'separator' }, exportItem] : [exportItem];
      }
    }
    const more = submenu('more-actions', labels.moreActions, moreActions);
    if (more) {
      out = out.length > 0 ? [...out, { kind: 'separator' }, more] : [more];
    }
    return out;
  }

  // Selection-scoped layout: either the right-click landed inside a multi-row
  // selection, or it landed on empty space while rows are selected. Everything
  // here acts on the whole selection, so single-cell / single-row-only items
  // (copy cell, copy row, set NULL) are deliberately omitted and the primary
  // "copy" is the whole selection.
  const frequent = push(
    item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows),
    canFilterByValue
      ? item('filter-by-value', labels.filterByValue, handlers.onFilterByValue)
      : null,
  );

  const danger =
    canDelete && hasSelectedRows
      ? push(item('delete-row', labels.deleteRow, handlers.onDeleteRow))
      : [];

  const moreActions = push(
    item('copy-as-csv', labels.copyAsCsv, handlers.onCopyAsCsv),
    item('copy-as-json', labels.copyAsJson, handlers.onCopyAsJson),
    item('copy-as-sql-insert', labels.copyAsSqlInsert, handlers.onCopyAsSqlInsert),
    item('copy-as-update', labels.copyAsUpdate, handlers.onCopyAsUpdate),
    item('copy-column-name', labels.copyColumnName, handlers.onCopyColumnName),
    item('copy-column-data', labels.copyColumnData, handlers.onCopyColumnData),
  );

  let out = frequent;
  if (danger.length > 0) {
    out = out.length > 0 ? [...out, { kind: 'separator' }, ...danger] : danger;
  }
  if (exportEnabled) {
    const exportItem = item('export', labels.export, handlers.onExport);
    if (exportItem) {
      out = out.length > 0 ? [...out, { kind: 'separator' }, exportItem] : [exportItem];
    }
  }
  const more = submenu('more-actions', labels.moreActions, moreActions);
  if (more) {
    out = out.length > 0 ? [...out, { kind: 'separator' }, more] : [more];
  }
  return out;
}
