import { useCallback } from 'react';
import type { DatabaseType, FilterCondition } from '../../types';
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
import type { DirtyRowEntry } from '../../stores/tableData/dirtyIndex';

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
  /** Opens the export dialog. */
  onExport?: () => void;
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
    dirtyByRow,
    onRestoreRow,
    onExport,
    t,
  } = options;

  const onContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const hit = resolveDataTableCellFromEvent(e.target);
      const hitHeaderCol = !hit ? resolveDataTableHeaderColFromEvent(e.target) : null;
      const effectiveColName = hit ? hit.columnName : hitHeaderCol;
      const effectiveColIdx = effectiveColName ? columnNames.indexOf(effectiveColName) : -1;
      const hitRow = hit ? rows[hit.rowIndex] : undefined;
      const hitColIdx = hit ? columnNames.indexOf(hit.columnName) : -1;
      const hitCellValue = hitRow && hitColIdx >= 0 ? (hitRow[hitColIdx] ?? null) : undefined;

      const fromProp = getContextCellText?.() ?? '';
      const fromSelection = window.getSelection()?.toString() ?? '';
      const cellTextForCopy =
        hitCellValue !== undefined
          ? hitCellValue == null
            ? ''
            : String(hitCellValue)
          : fromProp || fromSelection;

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

      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
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
          },
          hasCellContext,
          hasHeaderContext: !!hitHeaderCol,
          hasSelectedRows,
          multiRowSelection,
          exportEnabled,
          canFilterByValue,
          canSetNull,
          canDelete,
          canRestoreRow,
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
    ],
  );

  return { onContextMenu };
}
