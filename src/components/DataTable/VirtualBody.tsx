import { memo, useCallback, type Key } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useVirtualTable } from '../../hooks/useVirtualTable';
import type { ColumnDef } from './TableHeader';
import { CellRenderer } from './CellRenderer';
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
}

interface VirtualRowProps {
  vRow: { index: number; key: Key; start: number };
  row: unknown[];
  columns: ColumnDef[];
  colNames: string[];
  rowHeight: number;
  columnWidths?: number[];
  editingCell: { row: number; col: string } | null;
  selected: boolean;
  nextSelected: boolean;
  highlighted: boolean;
  selectRowLabel: string;
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  onRowSelect: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
}

const VirtualRow = memo(function VirtualRow({
  vRow,
  row,
  columns,
  colNames,
  rowHeight,
  columnWidths,
  editingCell,
  selected,
  nextSelected,
  highlighted,
  selectRowLabel,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  onRowSelect,
}: VirtualRowProps) {
  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      onRowSelect(vRow.index, {
        multi: e.metaKey || e.ctrlKey,
        range: e.shiftKey,
      });
    },
    [onRowSelect, vRow.index],
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
      onRowSelect(vRow.index, {
        multi: e.metaKey || e.ctrlKey,
        range: e.shiftKey,
      });
    },
    [onRowSelect, vRow.index],
  );

  return (
    <div
      key={vRow.key}
      tabIndex={0}
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
      <button
        type="button"
        className={cn(
          'flex w-10 shrink-0 items-center justify-center border-r border-edge/30 text-xs text-fg-muted',
          selected && 'border-l-2 border-l-accent text-accent',
        )}
        onClick={handleSelectButtonClick}
        title={selectRowLabel}
      >
        {vRow.index + 1}
      </button>
      {columns.map((col, colIdx) => {
        const value = row[colIdx];
        const isEditing = editingCell?.row === vRow.index && editingCell.col === col.name;
        const colW = columnWidths?.[colIdx] ?? 160;
        return (
          <div
            key={col.id}
            data-dt-row={vRow.index}
            data-dt-col={col.name}
            className="flex shrink-0 items-center overflow-hidden border-r border-edge/30 px-2"
            style={{ width: colW }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              onCellDoubleClick(vRow.index, col.name);
            }}
          >
            <div className="min-w-0 flex-1">
              <CellRenderer
                columnName={col.name}
                dataType={col.type}
                value={value}
                isEditing={isEditing}
                onCommit={(v) => onCellEdit(vRow.index, col.name, v)}
                onCancel={onCellEditCancel}
              />
            </div>
          </div>
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

  return (
    <div className="min-w-max" style={{ height: totalHeight, position: 'relative' }}>
      {virtualRows.map((vRow) => (
        <VirtualRow
          key={String(vRow.key)}
          vRow={vRow}
          row={rows[vRow.index] ?? []}
          columns={columns}
          colNames={colNames}
          rowHeight={rowHeight}
          columnWidths={columnWidths}
          editingCell={editingCell}
          selected={selectedRows.has(vRow.index)}
          nextSelected={selectedRows.has(vRow.index + 1)}
          highlighted={highlightedRow === vRow.index}
          selectRowLabel={selectRowLabel}
          onCellDoubleClick={onCellDoubleClick}
          onCellEdit={onCellEdit}
          onCellEditCancel={onCellEditCancel}
          onRowSelect={onRowSelect}
        />
      ))}
    </div>
  );
});
