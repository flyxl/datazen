import { useCallback } from 'react';
import type { DatabaseType, FilterCondition, SortCondition } from '../../types';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  formatRowsAsSqlInsert,
  formatRowsAsSqlUpdate,
  resolveDataTableCellFromEvent,
  resolveDataTableHeaderColFromEvent,
  rowToNamedRecord,
  serializeDataTableColumnValues,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsJson,
  serializeDataTableRowsAsTsv,
} from '../../lib/dataTableContextMenu';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { copyToClipboard } from '../../lib/fetchRelationDdl';
import type { PasteProblem } from '../../lib/gridErrors';
import type { SerializeCellsOptions } from '../../lib/gridClipboard';
import type { DirtyRowEntry } from '../../stores/tableData/dirtyIndex';
import type { SetValueActionId, SetValueState } from '../../lib/setValueActions';

/**
 * The grid's right-click menu.
 *
 * The whole menu construction lives here so `DataTable.tsx` stays a layout file: the menu
 * itself is the biggest block in the grid, and DB-02/03/05/06 all add to it. The behaviour
 * is the one that shipped before the extraction — item set, conditions, order, labels and
 * click effects are unchanged; only the location moved.
 */
export interface UseDataTableContextMenuOptions {
  /** Displayed column names, in render order. */
  columnNames: string[];
  /** Rendered rows (positional arrays, one per grid row). */
  rows: unknown[][];
  /** Currently selected row indices. */
  selectedRows: Set<number>;
  /** The grid is loading — "filter by value" is unavailable while loading. */
  loading?: boolean;
  /** Table name used for the export dialog and for the generated SQL. */
  exportTableName?: string;
  /** Export is possible at all (button visibility). */
  exportEnabled: boolean;
  /** Database dialect for the SQL rendering. */
  databaseType?: string;
  /** Primary-key column names; the SQL builders fall back to the first column for copy. */
  primaryKeyColumns?: string[];
  /** Whether "Set NULL" may be offered (defaults to "the grid can edit cells"). */
  enableSetNull?: boolean;
  /** Optional cell text source for the native context menu "copy" item. */
  getContextCellText?: () => string | null | undefined;
  onAddFilter?: (filter: FilterCondition) => void;
  onCellEdit?: (row: number, col: string, value: unknown) => void;
  onDeleteRows?: (rowIndices: number[]) => void;
  /** The dirty state of the current page, keyed by row index (DB-06). */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
  /** Undo the delete mark of a row (DB-06). */
  onRestoreRow?: (rowIndex: number) => void;
  /**
   * Open the tabs of the rows that reference the right-clicked row (DB-20).
   * Omitted ⇒ the row menu keeps its pre-DB-20 shape — result grids and
   * driver grids are unaffected.
   */
  onShowReferencingRows?: (rowIndex: number) => void;
  /** Opens the export dialog. */
  onExport?: () => void;
  /**
   * What the "Set Value" menu may offer for the cell under the pointer
   * (DB-05). Omitted → the menu keeps its shape from before DB-05.
   */
  setValueState?: (rowIndex: number, columnName: string) => SetValueState | null;
  /** Write the cell without opening an editor (DB-05). */
  onSetValue?: (rowIndex: number, columnName: string, action: SetValueActionId) => void;
  /** Ask for the candidate values of the right-clicked column (DB-05). */
  onLoadCandidates?: (rowIndex: number, columnName: string) => void;
  /* ── Selected-cell clipboard (DB-03). Omitted ⇒ the menu keeps its pre-DB-03 shape. ── */
  /** A cell region is selected, so the selected-cell entries are offered. */
  hasCellRange?: boolean;
  /** Copying the selection is possible (read-only connection ⇒ `false`, still visible). */
  canCopySelection?: boolean;
  /** Pasting into the selection is possible (no primary key ⇒ `false`, still visible). */
  canPasteSelection?: boolean;
  /** Non-contiguous extra regions the rectangular copy ignores; shown in the label. */
  selectionIgnoredRanges?: number;
  onCopySelectedCells?: () => void;
  onCopySelectedCellsAs?: (
    format: SerializeCellsOptions['format'],
    options?: { includeHeader?: boolean },
  ) => void;
  onPasteSelectedCells?: () => void;
  /** Row-level paste — DB-04 fills this in. */
  onPasteRows?: () => void;
  /** A clipboard write failed (the menu copy has no synchronous gesture to rely on). */
  onCopyProblem?: (problem: PasteProblem) => void;
  /* ── Multi-column sort (DB-11). Omitted ⇒ the header menu keeps its
     pre-DB-11 shape — the result grid and driver grids are unaffected. ── */
  /** The sorts in force, in `ORDER BY` order. */
  sorts?: SortCondition[];
  /** Add (or re-aim) one column of the sort list. */
  onAddSort?: (sort: SortCondition) => void;
  /** Remove one column from the sort list. */
  onRemoveSort?: (column: string) => void;
  /** Drop the whole sort list. */
  onClearSorts?: () => void;
  /* ── Frozen columns (DB-12). Omitted ⇒ no freeze entries in the header
     menu — driver grids are unaffected. ── */
  /** How many leading columns are currently frozen; `0` = none. */
  frozenColumns?: number;
  /** Freeze through the right-clicked column (that many leading columns). */
  onFreezeThrough?: (throughIndex: number) => void;
  /** Unfreeze every column. */
  onUnfreezeColumns?: () => void;
  /* ── Column statistics (DB-12). Omitted ⇒ no stats entry in the header
     menu — driver grids are unaffected. ── */
  /** Ask for the server-side aggregates of the right-clicked column. */
  onColumnStats?: (column: string) => void;
  /** i18n translate function. */
  t: (key: string, params?: Record<string, string | number>) => string;
}

export interface UseDataTableContextMenuResult {
  /** Spread onto the grid surface: `onContextMenu={onContextMenu}`. */
  onContextMenu: (event: React.MouseEvent) => void;
}

export function useDataTableContextMenu(
  options: UseDataTableContextMenuOptions,
): UseDataTableContextMenuResult {
  const {
    columnNames,
    rows,
    selectedRows,
    loading,
    exportTableName,
    exportEnabled,
    databaseType,
    primaryKeyColumns,
    enableSetNull,
    getContextCellText,
    onAddFilter,
    onCellEdit,
    onDeleteRows,
    onShowReferencingRows,
    dirtyByRow,
    onRestoreRow,
    onExport,
    setValueState,
    onSetValue,
    onLoadCandidates,
    hasCellRange = false,
    canCopySelection = false,
    canPasteSelection = false,
    selectionIgnoredRanges = 0,
    onCopySelectedCells,
    onCopySelectedCellsAs,
    onPasteSelectedCells,
    onPasteRows,
    onCopyProblem,
    sorts,
    onAddSort,
    onRemoveSort,
    onClearSorts,
    frozenColumns,
    onFreezeThrough,
    onUnfreezeColumns,
    onColumnStats,
    t,
  } = options;

  const onContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const hit = resolveDataTableCellFromEvent(e.target);
      const hitHeaderCol = !hit ? resolveDataTableHeaderColFromEvent(e.target) : null;
      // The multi-column sort block (DB-11) is bound to the right-clicked
      // header column; a click on a cell never offers it.
      const headerSort = hitHeaderCol && sorts ? { column: hitHeaderCol, sorts } : undefined;
      // The freeze block (DB-12) needs the right-clicked column's position
      // among the displayed columns and the freeze state in force.
      const headerFreeze =
        hitHeaderCol && (frozenColumns != null || onFreezeThrough || onUnfreezeColumns)
          ? { index: columnNames.indexOf(hitHeaderCol), frozen: frozenColumns ?? 0 }
          : undefined;
      // The stats entry (DB-12) is bound to the right-clicked header column;
      // a click on a cell never offers it.
      const headerStats = hitHeaderCol && onColumnStats ? { column: hitHeaderCol } : undefined;
      const effectiveColName = hit ? hit.columnName : hitHeaderCol;
      const effectiveColIdx = effectiveColName ? columnNames.indexOf(effectiveColName) : -1;
      const hitRow = hit ? rows[hit.rowIndex] : undefined;
      const hitColIdx = hit ? columnNames.indexOf(hit.columnName) : -1;
      const hitCellValue = hitRow && hitColIdx >= 0 ? (hitRow[hitColIdx] ?? null) : undefined;

      const fromProp = getContextCellText?.() ?? '';
      /**
       * The cell text for "copy" comes from the **row data**, never from the DOM
       * (DB-03 §3.8 R-B). `window.getSelection()` used to be the last fallback and it read the
       * *rendered* text — which `CellRenderer` truncates to 120 characters. Copying that
       * would silently cut long values and JSON; a selection that is not a cell falls back to
       * what the caller explicitly provides, and to nothing at all.
       */
      const cellTextForCopy =
        hitCellValue !== undefined
          ? hitCellValue == null
            ? ''
            : typeof hitCellValue === 'object'
              ? JSON.stringify(hitCellValue)
              : String(hitCellValue)
          : fromProp;

      const selectedIndices = Array.from(selectedRows).sort((a, b) => a - b);
      const selectedDataRows = selectedIndices
        .map((i) => rows[i])
        .filter((r): r is unknown[] => Array.isArray(r));
      const hasSelectedRows = selectedDataRows.length > 0;
      const hasCellContext = hit != null && Array.isArray(hitRow);
      /**
       * The right-click landed inside a selection of more than one row: the menu
       * must act on the whole selection, not just the clicked row, so every
       * single-row / single-cell action is dropped below.
       */
      const multiRowSelection =
        hasCellContext && hit != null && selectedRows.size > 1 && selectedRows.has(hit.rowIndex);
      /** Row the cell-scoped actions operate on; null in selection mode. */
      const actionRow = hasCellContext && hitRow && !multiRowSelection ? hitRow : null;
      /** Rows the selection-scoped actions operate on. */
      const selectionRowActions = multiRowSelection || !hasCellContext ? selectedDataRows : [];
      const canFilterByValue = !loading && hasCellContext && !!onAddFilter && !!hit;
      const setNullAllowed = enableSetNull ?? !!onCellEdit;
      const canSetNull = hasCellContext && setNullAllowed && !!onCellEdit && !!hit;
      const deleteIndices = hasSelectedRows ? selectedIndices : hit != null ? [hit.rowIndex] : [];
      const canDelete =
        !!onDeleteRows && (primaryKeyColumns?.length ?? 0) > 0 && deleteIndices.length > 0;
      /**
       * "Restore This Row" is offered on a row that is marked for deletion
       * (DB-06). The row state comes from the dirty index, never from the
       * selection: the deletion mark is a fact of the staged change.
       */
      const canRestoreRow =
        !!hit && !!onRestoreRow && dirtyByRow?.get(hit.rowIndex)?.state === 'deleted';

      const csvRows = actionRow ? [actionRow] : selectedDataRows;
      const canCopyCsv = csvRows.length > 0;

      /**
       * What the "Set Value" menu may offer for the right-clicked cell
       * (DB-05). It is asked for at the moment of the click, because the
       * answer depends on the column (nullable?) and on the row (is there
       * anything to revert?).
       */
      const setValue = hit
        ? (setValueState?.(hit.rowIndex, hit.columnName) ?? undefined)
        : undefined;

      /**
       * The menu callback runs after an async IPC round trip, so the synchronous user
       * gesture `navigator.clipboard.writeText` needs is gone. `copyToClipboard` carries the
       * three-step chain (async API → `write_clipboard` → `execCommand`) and reports failure
       * instead of dropping it, so "I clicked Copy and nothing happened" cannot happen (G5).
       */
      const copyText = (text: string) => {
        void copyToClipboard(text).then((ok) => {
          if (!ok) onCopyProblem?.({ code: 'grid.clipboard.writeFailed', row: null, column: null });
        });
      };

      void showNativeContextMenu(
        buildDataTableContextMenuItems({
          labels: {
            copy: t('common.copy'),
            copyRow: t('dataTable.copyRow'),
            moreActions: t('dataTable.moreActions'),
            copyAsJson: t('dataTable.copyAsJson'),
            copyAsSqlInsert: t('dataTable.copyAsSqlInsert'),
            copyAsUpdate: t('dataTable.copyAsUpdate'),
            copyAsCsv: t('dataTable.copyAsCsv'),
            copyColumnName: t('dataTable.copyColumnName'),
            copyColumnData: t('dataTable.copyColumnData'),
            setNull: t('dataTable.setNull'),
            filterByValue: t('dataTable.filterByValue'),
            copySelectedRows:
              selectedRows.size > 1
                ? `${t('common.copy')} ${t('export.selectedRows')} (${selectedRows.size})`
                : `${t('common.copy')} ${t('export.selectedRows')}`,
            deleteRow:
              deleteIndices.length > 1
                ? `${t('dataTable.deleteRow')} (${deleteIndices.length})`
                : t('dataTable.deleteRow'),
            export:
              selectedRows.size > 0
                ? `${t('export.export')} (${t('export.selectedRows')} ${selectedRows.size})`
                : t('export.export'),
            restoreRow: t('tableData.restoreRow'),
            referencedBy: t('dataTable.referencedBy'),
            setValue: t('cellEditor.setValue'),
            setEmpty: t('cellEditor.setEmpty'),
            setDefault: t('cellEditor.setDefault'),
            revertCell: t('cellEditor.revertCell'),
            loadCandidates: t('cellEditor.loadCandidates'),
            copySelectedCells: t('dataTable.copySelectedCells'),
            copySelectedCellsAs: t('dataTable.copySelectedCellsAs'),
            copySelectedCellsAsTsv: t('dataTable.copySelectedCellsAsTsv'),
            copySelectedCellsAsTsvWithHeader: t('dataTable.copySelectedCellsAsTsvWithHeader'),
            copySelectedCellsAsCsv: t('dataTable.copySelectedCellsAsCsv'),
            copySelectedCellsAsJson: t('dataTable.copySelectedCellsAsJson'),
            // The DB-10 formats: plain text, a text/html table, a pipe table.
            copySelectedCellsAsPlain: t('dataTable.copySelectedCellsAsPlain'),
            copySelectedCellsAsHtml: t('dataTable.copySelectedCellsAsHtml'),
            copySelectedCellsAsMarkdown: t('dataTable.copySelectedCellsAsMarkdown'),
            pasteSelectedCells: t('dataTable.pasteSelectedCells'),
            pasteRows: t('dataTable.pasteRows'),
            sortMulti: t('dataTable.sortMulti'),
            sortAscending: t('dataTable.sortAscending'),
            sortDescending: t('dataTable.sortDescending'),
            removeSortColumn: t('dataTable.removeSortColumn'),
            clearSorts: t('dataTable.clearSorts'),
            freezeThrough: t('dataTable.freezeThrough'),
            unfreezeColumns: t('dataTable.unfreezeColumns'),
            columnStats: t('dataTable.columnStats'),
          },
          handlers: {
            onCopy:
              hasCellContext && !multiRowSelection
                ? () => {
                    copyText(cellTextForCopy);
                  }
                : undefined,
            onCopyRow: actionRow
              ? () => {
                  copyText(serializeDataTableRowsAsTsv([actionRow]));
                }
              : undefined,
            onCopyAsJson: actionRow
              ? () => {
                  copyText(JSON.stringify(rowToNamedRecord(columnNames, actionRow), null, 2));
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(serializeDataTableRowsAsJson(columnNames, selectionRowActions));
                  }
                : undefined,
            onCopyAsSqlInsert: actionRow
              ? () => {
                  copyText(
                    formatRowAsSqlInsert(
                      exportTableName || 'table',
                      columnNames,
                      actionRow,
                      databaseType as DatabaseType,
                    ),
                  );
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(
                      formatRowsAsSqlInsert(
                        exportTableName || 'table',
                        columnNames,
                        selectionRowActions,
                        databaseType as DatabaseType,
                      ),
                    );
                  }
                : undefined,
            onCopyAsUpdate: actionRow
              ? () => {
                  copyText(
                    formatRowAsSqlUpdate(
                      exportTableName || 'table',
                      columnNames,
                      actionRow,
                      primaryKeyColumns,
                      databaseType as DatabaseType,
                    ),
                  );
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(
                      formatRowsAsSqlUpdate(
                        exportTableName || 'table',
                        columnNames,
                        selectionRowActions,
                        primaryKeyColumns,
                        databaseType as DatabaseType,
                      ),
                    );
                  }
                : undefined,
            onCopyAsCsv: canCopyCsv
              ? () => {
                  copyText(serializeDataTableRowsAsCsv(columnNames, csvRows));
                }
              : undefined,
            onCopyColumnName: effectiveColName
              ? () => {
                  copyText(effectiveColName);
                }
              : undefined,
            onCopyColumnData:
              effectiveColIdx >= 0
                ? () => {
                    copyText(serializeDataTableColumnValues(effectiveColIdx, rows));
                  }
                : undefined,
            onSetNull:
              canSetNull && !multiRowSelection && hit
                ? () => {
                    onCellEdit?.(hit.rowIndex, hit.columnName, null);
                  }
                : undefined,
            onFilterByValue:
              canFilterByValue && hit
                ? () => {
                    onAddFilter?.({
                      column: hit.columnName,
                      operator: 'eq',
                      value: hitCellValue == null ? '' : String(hitCellValue),
                    });
                  }
                : undefined,
            onCopySelectedRows: hasSelectedRows
              ? () => {
                  copyText(serializeDataTableRowsAsTsv(selectedDataRows));
                }
              : undefined,
            onDeleteRow: canDelete
              ? () => {
                  onDeleteRows?.(deleteIndices);
                }
              : undefined,
            onExport: exportEnabled
              ? () => {
                  onExport?.();
                }
              : undefined,
            onRestoreRow:
              canRestoreRow && hit
                ? () => {
                    onRestoreRow?.(hit.rowIndex);
                  }
                : undefined,
            // The reverse jump (DB-20) is bound to the row under the pointer:
            // the referencing rows are the ones pointing at **this** row.
            onShowReferencingRows:
              hit && onShowReferencingRows
                ? () => {
                    onShowReferencingRows(hit.rowIndex);
                  }
                : undefined,
            // The write entries (DB-05). They need the cell under the
            // pointer, so they are bound to it here.
            onSetValue:
              setValue && hit && onSetValue
                ? (id: SetValueActionId) => {
                    onSetValue(hit.rowIndex, hit.columnName, id);
                  }
                : undefined,
            onLoadCandidates:
              hit && onLoadCandidates
                ? () => {
                    onLoadCandidates(hit.rowIndex, hit.columnName);
                  }
                : undefined,
            onSortAscending: onAddSort
              ? (column: string) => {
                  onAddSort({ column, descending: false });
                }
              : undefined,
            onSortDescending: onAddSort
              ? (column: string) => {
                  onAddSort({ column, descending: true });
                }
              : undefined,
            onRemoveSort,
            onClearSorts,
            onFreezeThrough,
            onUnfreezeColumns,
            onColumnStats,
          },
          hasCellContext,
          hasHeaderContext: !!hitHeaderCol,
          headerSort,
          headerFreeze,
          headerStats,
          hasSelectedRows,
          multiRowSelection,
          exportEnabled,
          canFilterByValue,
          canSetNull,
          canDelete,
          canRestoreRow,
          setValue,
          hasCellRange,
          canCopySelection,
          canPasteSelection,
          selectionIgnoredRanges,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      columnNames,
      exportEnabled,
      exportTableName,
      getContextCellText,
      onAddFilter,
      onCellEdit,
      enableSetNull,
      onDeleteRows,
      primaryKeyColumns,
      rows,
      selectedRows,
      t,
      loading,
      onExport,
      dirtyByRow,
      onRestoreRow,
      setValueState,
      onSetValue,
      onLoadCandidates,
      hasCellRange,
      canCopySelection,
      canPasteSelection,
      selectionIgnoredRanges,
      onCopySelectedCells,
      onCopySelectedCellsAs,
      onPasteSelectedCells,
      onPasteRows,
      onCopyProblem,
      sorts,
      onAddSort,
      onRemoveSort,
      onClearSorts,
      frozenColumns,
      onFreezeThrough,
      onUnfreezeColumns,
      onColumnStats,
    ],
  );

  return { onContextMenu };
}
