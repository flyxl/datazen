import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). */
export type DataTableContextMenuLabels = {
  copy: string;
  copyRow: string;
  copyAsJson: string;
  copyAsSqlInsert: string;
  copyAsUpdate: string;
  copyAsCsv: string;
  copyColumnName: string;
  setNull: string;
  filterByValue: string;
  copySelectedRows: string;
  export: string;
};

export type DataTableContextMenuHandlers = {
  onCopy?: () => void;
  onCopyRow?: () => void;
  onCopyAsJson?: () => void;
  onCopyAsSqlInsert?: () => void;
  onCopyAsUpdate?: () => void;
  onCopyAsCsv?: () => void;
  onCopyColumnName?: () => void;
  onSetNull?: () => void;
  onFilterByValue?: () => void;
  onCopySelectedRows?: () => void;
  onExport?: () => void;
};

export type BuildDataTableContextMenuArgs = {
  labels: DataTableContextMenuLabels;
  handlers: DataTableContextMenuHandlers;
  /** Right-clicked a data cell (row+column resolved). */
  hasCellContext?: boolean;
  /** One or more rows are selected. */
  hasSelectedRows?: boolean;
  /** Export action available. */
  exportEnabled?: boolean;
  /** Filter-by-value available (needs cell context + filter handler). */
  canFilterByValue?: boolean;
  /** Set NULL available (needs cell context + edit handler). */
  canSetNull?: boolean;
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

/** Build a JSON object row from column names + cell values. */
export function rowToNamedRecord(columnNames: string[], row: unknown[]): Record<string, unknown> {
  const obj: Record<string, unknown> = {};
  for (let i = 0; i < columnNames.length; i++) {
    const name = columnNames[i];
    if (name) obj[name] = row[i] ?? null;
  }
  return obj;
}

function escapeSqlValue(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return `'${str.replaceAll("'", "''")}'`;
}

function quoteIdent(name: string): string {
  return `"${name.replaceAll('"', '""')}"`;
}

/** Single-row SQL INSERT (identifiers quoted with double quotes). */
export function formatRowAsSqlInsert(
  tableName: string,
  columnNames: string[],
  row: unknown[],
): string {
  const cols = columnNames.map((c) => quoteIdent(c)).join(', ');
  const values = columnNames.map((_, i) => escapeSqlValue(row[i])).join(', ');
  const table = quoteIdent(tableName);
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
    return `${quoteIdent(col)} = ${escapeSqlValue(row[idx])}`;
  });
  const where = whereKeys
    .map((col) => {
      const idx = columnNames.indexOf(col);
      return `${quoteIdent(col)} = ${escapeSqlValue(row[idx])}`;
    })
    .join(' AND ');

  return `UPDATE ${quoteIdent(tableName)} SET ${setClauses.join(', ')} WHERE ${where};`;
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
 * TablePlus-style DataTable context menu.
 * With cell context: always a multi-item menu (copy / copy row / copy as… / set null / filter / export).
 * Never emit a lonely single “Export” item when cell context exists.
 */
export function buildDataTableContextMenuItems(
  args: BuildDataTableContextMenuArgs,
): NativeMenuItemDef[] {
  const {
    labels,
    handlers,
    hasCellContext = false,
    hasSelectedRows = false,
    exportEnabled = false,
    canFilterByValue = false,
    canSetNull = false,
  } = args;

  if (hasCellContext) {
    const cellBlock = push(
      item('copy', labels.copy, handlers.onCopy),
      item('copy-row', labels.copyRow, handlers.onCopyRow),
      item('copy-as-json', labels.copyAsJson, handlers.onCopyAsJson),
      item('copy-as-sql-insert', labels.copyAsSqlInsert, handlers.onCopyAsSqlInsert),
      item('copy-as-update', labels.copyAsUpdate, handlers.onCopyAsUpdate),
      item('copy-as-csv', labels.copyAsCsv, handlers.onCopyAsCsv),
      item('copy-column-name', labels.copyColumnName, handlers.onCopyColumnName),
      canSetNull ? item('set-null', labels.setNull, handlers.onSetNull) : null,
      canFilterByValue
        ? item('filter-by-value', labels.filterByValue, handlers.onFilterByValue)
        : null,
    );

    const tail = push(
      hasSelectedRows
        ? item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows)
        : null,
      exportEnabled ? item('export', labels.export, handlers.onExport) : null,
    );

    if (cellBlock.length === 0) return tail;
    if (tail.length === 0) return cellBlock;
    return [...cellBlock, { kind: 'separator' }, ...tail];
  }

  // No cell hit — still avoid a single lonely export when possible.
  return push(
    hasSelectedRows
      ? item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows)
      : null,
    hasSelectedRows ? item('copy-as-csv', labels.copyAsCsv, handlers.onCopyAsCsv) : null,
    exportEnabled ? item('export', labels.export, handlers.onExport) : null,
  );
}
