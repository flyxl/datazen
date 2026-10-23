import type { NativeMenuItemDef } from './nativeContextMenu';
import type { DatabaseType } from '../types';
import { escapeIdent, getEscapeSqlValue } from './databaseTypes';
import { rowToNamedRecord as clipboardNamedRecord, serializeCells } from './gridClipboard';
import {
  resolveSetValueActions,
  type SetValueActionId,
  type SetValueState,
} from './setValueActions';

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
  /**
   * The write entries of the "Set Value" submenu (DB-05).
   *
   * They are optional so a caller that does not offer the submenu does not
   * have to invent labels for it — the menu keeps its shape from before
   * DB-05, with `Set NULL` inside `more-actions`.
   */
  /** "Set Value" — the title of the write submenu. */
  setValue?: string;
  /** "Set empty string". */
  setEmpty?: string;
  /** "Use column default" — a new row. */
  setDefault?: string;
  /** "Revert this cell" — an existing row with a staged change. */
  revertCell?: string;
  /** "Load candidate values". */
  loadCandidates?: string;
  /* ── Selected-cell clipboard (DB-03). All optional: a caller that does not offer the
     entries keeps the menu it had before DB-03, and no label has to be invented. ── */
  /** "Copy Selected Cells". */
  copySelectedCells?: string;
  /** "Copy Selected Cells As …" — the submenu title. */
  copySelectedCellsAs?: string;
  /** "Copy as TSV". */
  copySelectedCellsAsTsv?: string;
  /** "Copy as TSV with Header". */
  copySelectedCellsAsTsvWithHeader?: string;
  /** "Copy as CSV". */
  copySelectedCellsAsCsv?: string;
  /** "Copy as JSON (2D array)". */
  copySelectedCellsAsJson?: string;
  /** "Paste to Selected Cells". */
  pasteSelectedCells?: string;
  /** "Paste Rows" — the row-level entry DB-04 fills in; DB-03 only reserves the slot. */
  pasteRows?: string;
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
  /** Write the cell without opening an editor (DB-05). */
  onSetValue?: (id: SetValueActionId) => void;
  /** Read the candidate values of an enumeration column (DB-05). */
  onLoadCandidates?: () => void;
  /** Copy the selected region as TSV (DB-03). */
  onCopySelectedCells?: () => void;
  /** Copy the selected region in an explicit format (DB-03). */
  onCopySelectedCellsAs?: (format: 'tsv' | 'csv' | 'json', options?: { includeHeader?: boolean }) => void;
  /** Paste the system clipboard into the selected region (DB-03). */
  onPasteSelectedCells?: () => void;
  /** Paste whole rows (DB-04 owns the handler). */
  onPasteRows?: () => void;
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
  /**
   * What the "Set Value" submenu may offer for the right-clicked cell
   * (DB-05). Omitted — or offered without a handler — the menu keeps its
   * today's shape, with `Set NULL` inside `more-actions`.
   */
  setValue?: SetValueState;
  /**
   * There is a cell region to copy/paste into (DB-03). Default `false`: without it the menu
   * keeps exactly the item sequence it had before DB-03, which is the regression gate for
   * every existing consumer of this builder.
   */
  hasCellRange?: boolean;
  /** Copying the selection is possible (default `false` ⇒ the entry renders disabled). */
  canCopySelection?: boolean;
  /** Pasting into the selection is possible (default `false` ⇒ the entry renders disabled). */
  canPasteSelection?: boolean;
  /** Extra non-contiguous regions the rectangular copy deliberately ignores (DB-03 §3.7). */
  selectionIgnoredRanges?: number;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action };
}

/**
 * An item that stays **visible but disabled**.
 *
 * The difference matters and the chapter calls it out: an unavailable entry is
 * `enabled: false` **with** an action, while an irrelevant entry is no action at all (which
 * `item()` drops). Conflating the two either hides why an action is missing or offers a
 * click that does nothing.
 */
function enabledItem(
  id: string,
  label: string,
  action: (() => void) | undefined,
  enabled: boolean,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action, ...(enabled ? {} : { enabled: false }) };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

function submenu(id: string, label: string, items: NativeMenuItemDef[]): NativeMenuItemDef | null {
  return items.length > 0 ? { kind: 'submenu', id, label, items } : null;
}

/** Serialize table rows to TSV (tab-separated cells, newline-separated rows). */
export function serializeDataTableRowsAsTsv(rows: unknown[][]): string {
  // Delegates to the one serializer so copy/export share the escaping rules (DB-03 Step 9):
  // a value containing a tab used to add a phantom column here, and `[object Object]` used
  // to be written for an object value. Both are fixed in `serializeCells`.
  return serializeCells(rows, { format: 'tsv', columnNames: [] });
}

/** Serialize rows as CSV with a header line. */
export function serializeDataTableRowsAsCsv(columnNames: string[], rows: unknown[][]): string {
  return serializeCells(rows, { format: 'csv', columnNames, includeHeader: true });
}

/** Serialize rows as a pretty-printed JSON array of named records. */
export function serializeDataTableRowsAsJson(columnNames: string[], rows: unknown[][]): string {
  return serializeCells(rows, {
    format: 'json',
    columnNames,
    jsonObjects: true,
    // Kept at 2 so this function's output stays byte-identical to what it shipped before.
    jsonIndent: 2,
  });
}

/** Build a JSON object row from column names + cell values. */
export function rowToNamedRecord(columnNames: string[], row: unknown[]): Record<string, unknown> {
  return clipboardNamedRecord(columnNames, row);
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
  // One column, no header, same kernel as everything else: a value with a newline in it now
  // arrives quoted instead of silently becoming two lines.
  return serializeCells(
    rows.map((row) => [Array.isArray(row) ? row[columnIndex] : null]),
    { format: 'tsv', columnNames: [''] },
  );
}

/**
 * The label of a "Set Value" entry.
 *
 * The rules live in `components/DataTable/editors/setValueMenu.ts`; this
 * only maps an action to the text the caller supplied.
 */
function setValueActionLabel(
  id: SetValueActionId,
  labels: DataTableContextMenuLabels,
): string {
  switch (id) {
    case 'setNull':
      return labels.setNull;
    case 'setEmpty':
      return labels.setEmpty ?? '';
    case 'setDefault':
      return labels.setDefault ?? '';
    case 'revertCell':
      return labels.revertCell ?? '';
  }
}

/**
 * The "Set Value" submenu (DB-05).
 *
 * It is built only when the caller offers both the state of the cell and a
 * handler; otherwise the menu keeps its shape from before DB-05, with
 * `Set NULL` inside `more-actions` (the result grid still relies on it).
 */
function buildSetValueSubmenu(
  args: BuildDataTableContextMenuArgs,
): NativeMenuItemDef | null {
  const { labels, handlers, setValue } = args;
  if (!setValue || !handlers.onSetValue) return null;
  const items = resolveSetValueActions(setValue)
    .map((action) =>
      item(
        `set-value-${action.id}`,
        setValueActionLabel(action.id, labels),
        () => handlers.onSetValue?.(action.id),
      ),
    )
    .filter((entry): entry is NativeMenuItemDef => entry !== null);
  if (setValue.canLoadCandidates === true && handlers.onLoadCandidates) {
    const loadItem = item(
      'set-value-load-candidates',
      labels.loadCandidates ?? '',
      handlers.onLoadCandidates,
    );
    if (loadItem) items.push(loadItem);
  }
  return submenu('set-value', labels.setValue ?? '', items);
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
    hasCellRange = false,
    canCopySelection = false,
    canPasteSelection = false,
    selectionIgnoredRanges = 0,
  } = args;

  /**
   * The selected-cell clipboard block (DB-03).
   *
   * Only built when the caller reports a region: a menu unaware of the feature keeps its
   * exact previous item sequence. Unavailable entries render `enabled: false` rather than
   * disappearing, because "the connection is read-only" is information the user needs.
   */
  const clipboardBlock = (): NativeMenuItemDef[] => {
    if (!hasCellRange) return [];
    const copyAs = submenu('copy-selected-cells-as', labels.copySelectedCellsAs ?? '', [
      ...push(
        item('copy-selected-cells-as-tsv', labels.copySelectedCellsAsTsv ?? '', () =>
          handlers.onCopySelectedCellsAs?.('tsv'),
        ),
        item('copy-selected-cells-as-tsv-header', labels.copySelectedCellsAsTsvWithHeader ?? '', () =>
          handlers.onCopySelectedCellsAs?.('tsv', { includeHeader: true }),
        ),
        item('copy-selected-cells-as-csv', labels.copySelectedCellsAsCsv ?? '', () =>
          handlers.onCopySelectedCellsAs?.('csv'),
        ),
        item('copy-selected-cells-as-json', labels.copySelectedCellsAsJson ?? '', () =>
          handlers.onCopySelectedCellsAs?.('json'),
        ),
      ),
    ]);
    const copyLabel =
      selectionIgnoredRanges > 0 && labels.copySelectedCells
        ? `${labels.copySelectedCells} (${selectionIgnoredRanges}…)`
        : labels.copySelectedCells;
    return push(
      enabledItem(
        'copy-selected-cells',
        copyLabel ?? '',
        handlers.onCopySelectedCells,
        canCopySelection,
      ),
      copyAs,
      enabledItem(
        'paste-selected-cells',
        labels.pasteSelectedCells ?? '',
        handlers.onPasteSelectedCells,
        canPasteSelection,
      ),
    );
  };

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
    // The "Set Value" submenu (DB-05). It replaces the single "Set NULL"
    // entry of `more-actions`: offering both would show the same write
    // twice, under two different names.
    const setValueMenu = buildSetValueSubmenu(args);
    const usesSetValueMenu = setValueMenu !== null;

    const frequent = push(
      item('copy', labels.copy, handlers.onCopy),
      item('copy-row', labels.copyRow, handlers.onCopyRow),
      ...clipboardBlock(),
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
      canSetNull && !usesSetValueMenu
        ? item('set-null', labels.setNull, handlers.onSetNull)
        : null,
      hasSelectedRows
        ? item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows)
        : null,
    );

    let out = frequent;
    if (setValueMenu) {
      out = [...out, setValueMenu];
    }
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
    // The row-level paste slot. DB-03 reserves it; DB-04 (INSERT) fills the handler in.
    item('paste-rows', labels.pasteRows ?? '', handlers.onPasteRows),
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
