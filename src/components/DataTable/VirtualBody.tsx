import { memo, useCallback, useMemo, type Key } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useVirtualTable } from '../../hooks/useVirtualTable';
import type { ColumnDef } from './TableHeader';
import { GridCell } from './GridCell';
import { classifyDataType } from '../../lib/dataTypeColors';
import type { CellCoord } from '../../stores/tableData/gridSelection';
import { cn } from '../../lib/cn';

export interface VirtualBodyProps {
  columns: ColumnDef[];
  rows: unknown[][];
  rowHeight: number;
  editingCell: { row: number; col: string } | null;
  selectedRows: Set<number>;
  highlightedRow?: number | null;
  scrollElement: HTMLDivElement | null;
  columnWidths?: number[];
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  onRowSelect: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  /**
   * The cell-selection DOM contract is on: the ARIA grid roles and the `data-dt-*`
   * selection attributes are rendered as one set (a `gridcell` without a `grid` parent
   * would be invalid ARIA, 00-contracts §4.4).
   */
  cellSelectionEnabled?: boolean;
  /** Membership test for the current cell selection; defaults to "nothing selected". */
  isCellSelected?: (rowIndex: number, columnName: string) => boolean;
  /** Focus corner of the selection, rendered as `data-dt-active`. */
  activeCell?: CellCoord | null;
  /** Prefix of the cell DOM ids the grid's `aria-activedescendant` points at. */
  cellIdPrefix?: string;
  /**
   * Row-number slot click. Falls back to `onRowSelect` when it is not given; the grid uses
   * it to keep the "no consumer-side row path" fallback on the gutter only — a click on the
   * row background must never clear a row set the consumer owns.
   */
  onRowGutterSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  /** Row clicked (single click) — used by the parent to track the detail-panel row. */
  onRowClick?: (index: number) => void;
}

interface VirtualRowProps {
  vRow: { index: number; key: Key; start: number };
  row: unknown[];
  columns: ColumnDef[];
  colNames: string[];
  columnTypes: string[];
  rowHeight: number;
  columnWidths?: number[];
  editingCell: { row: number; col: string } | null;
  selected: boolean;
  nextSelected: boolean;
  highlighted: boolean;
  selectRowLabel: string;
  cellSelectionEnabled: boolean;
  isCellSelected: (rowIndex: number, columnName: string) => boolean;
  activeRow: number | null;
  activeColumnName: string | null;
  cellIdPrefix?: string;
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  onRowSelect: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onRowGutterSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onRowClick?: (index: number) => void;
}

const VirtualRow = memo(function VirtualRow({
  vRow,
  row,
  columns,
  colNames,
  columnTypes,
  rowHeight,
  columnWidths,
  editingCell,
  selected,
  nextSelected,
  highlighted,
  selectRowLabel,
  cellSelectionEnabled,
  isCellSelected,
  activeRow,
  activeColumnName,
  cellIdPrefix,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  onRowSelect,
  onRowGutterSelect,
  onRowClick,
}: VirtualRowProps) {
  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      const target = e.target instanceof Element ? e.target : null;
      const insideCell =
        cellSelectionEnabled &&
        target !== null &&
        target.closest('[data-dt-row][data-dt-col]') !== null;
      if (insideCell) {
        // A click inside a cell belongs to the cell-selection path: selecting the whole
        // row here would replace the cell selection with a row selection (two truths).
        // The row-level "open detail" callback still fires.
        onRowClick?.(vRow.index);
        return;
      }
      onRowSelect(vRow.index, {
        multi: e.metaKey || e.ctrlKey,
        range: e.shiftKey,
      });
    },
    [cellSelectionEnabled, onRowClick, onRowSelect, vRow.index],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onRowSelect(vRow.index);
      }
    },
    [onRowSelect, vRow.index],
  );

  const handleRowDoubleClick = useCallback(() => {
    const first = colNames[0];
    if (first) onCellDoubleClick(vRow.index, first);
  }, [colNames, onCellDoubleClick, vRow.index]);

  const handleSelectButtonClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      const opts = {
        multi: e.metaKey || e.ctrlKey,
        range: e.shiftKey,
      };
      if (onRowGutterSelect) {
        onRowGutterSelect(vRow.index, opts);
        return;
      }
      onRowSelect(vRow.index, opts);
    },
    [onRowGutterSelect, onRowSelect, vRow.index],
  );

  return (
    <div
      key={vRow.key}
      tabIndex={0}
      role={cellSelectionEnabled ? 'row' : undefined}
      data-dt-row-view={vRow.index}
      aria-selected={cellSelectionEnabled && selected ? 'true' : undefined}
      className={cn(
        'absolute left-0 flex w-full cursor-pointer outline-none',
        selected && nextSelected ? 'border-b border-accent/10' : 'border-b border-edge/30',
        vRow.index % 2 === 1 ? 'bg-surface-raised/50' : 'bg-surface',
        selected
          ? 'bg-accent/15 dark:bg-accent/20'
          : highlighted
            ? 'bg-accent/8 dark:bg-accent/12'
            : 'hover:bg-surface-raised/50',
      )}
      style={{ top: vRow.start, height: rowHeight }}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      onDoubleClick={handleRowDoubleClick}
    >
      {/* The row-number slot is the row header: a `row` may only contain gridcell /
          rowheader / columnheader children, so the bare button has to be wrapped. */}
      <div
        role={cellSelectionEnabled ? 'rowheader' : undefined}
        className="flex w-10 shrink-0 items-center border-r border-edge/30"
      >
        <button
          type="button"
          tabIndex={-1}
          className={cn(
            'flex h-full w-full items-center justify-center text-xs text-fg-muted',
            selected && 'border-l-2 border-l-accent text-accent',
          )}
          onClick={handleSelectButtonClick}
          title={selectRowLabel}
        >
          {vRow.index + 1}
        </button>
      </div>
      {columns.map((col, colIdx) => {
        const value = row[colIdx];
        const isEditing = editingCell?.row === vRow.index && editingCell.col === col.name;
        const colW = columnWidths?.[colIdx] ?? 160;
        const cellId = cellIdPrefix ? `${cellIdPrefix}-r${vRow.index}c${colIdx}` : undefined;
        return (
          <GridCell
            key={col.id}
            id={cellId}
            rowIndex={vRow.index}
            column={col}
            value={value}
            width={colW}
            selected={cellSelectionEnabled && isCellSelected(vRow.index, col.name)}
            active={
              cellSelectionEnabled &&
              activeRow === vRow.index &&
              activeColumnName === col.name
            }
            editing={isEditing}
            cellType={columnTypes[colIdx]}
            gridSemantics={cellSelectionEnabled}
            onCellDoubleClick={onCellDoubleClick}
            onCellEdit={onCellEdit}
            onCellEditCancel={onCellEditCancel}
          />
        );
      })}
    </div>
  );
});

export const VirtualBody = memo(function VirtualBody({
  columns,
  rows,
  rowHeight,
  editingCell,
  selectedRows,
  highlightedRow,
  scrollElement,
  columnWidths,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  onRowSelect,
  cellSelectionEnabled = false,
  isCellSelected,
  activeCell = null,
  cellIdPrefix,
  onRowGutterSelect,
  onRowClick,
}: VirtualBodyProps) {
  const { t } = useI18n();
  const { virtualRows, totalHeight } = useVirtualTable({
    rows,
    rowHeight,
    overscan: 12,
    scrollElement,
  });

  const colNames = columns.map((c) => c.name);
  const selectRowLabel = t('dataTable.selectRow');
  /** Normalised column type, rendered as `data-dt-cell-type` (DB-05 swaps the source). */
  const columnTypes = useMemo(() => columns.map((c) => classifyDataType(c.type)), [columns]);
  const cellSelected = useCallback(
    (rowIndex: number, columnName: string) => isCellSelected?.(rowIndex, columnName) ?? false,
    [isCellSelected],
  );

  return (
    <div
      className="min-w-max"
      style={{ height: totalHeight, position: 'relative' }}
      role={cellSelectionEnabled ? 'rowgroup' : undefined}
    >
      {virtualRows.map((vRow) => (
        <VirtualRow
          key={String(vRow.key)}
          vRow={vRow}
          row={rows[vRow.index] ?? []}
          columns={columns}
          colNames={colNames}
          columnTypes={columnTypes}
          rowHeight={rowHeight}
          columnWidths={columnWidths}
          editingCell={editingCell}
          selected={selectedRows.has(vRow.index)}
          nextSelected={selectedRows.has(vRow.index + 1)}
          highlighted={highlightedRow === vRow.index}
          selectRowLabel={selectRowLabel}
          cellSelectionEnabled={cellSelectionEnabled}
          isCellSelected={cellSelected}
          activeRow={activeCell?.rowIndex ?? null}
          activeColumnName={activeCell?.columnName ?? null}
          cellIdPrefix={cellIdPrefix}
          onCellDoubleClick={onCellDoubleClick}
          onCellEdit={onCellEdit}
          onCellEditCancel={onCellEditCancel}
          onRowSelect={onRowSelect}
          onRowGutterSelect={onRowGutterSelect}
          onRowClick={onRowClick}
        />
      ))}
    </div>
  );
});
