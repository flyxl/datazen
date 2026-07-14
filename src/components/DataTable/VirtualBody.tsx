import { useMemo } from 'react';
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

export function VirtualBody({
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

  const colNames = useMemo(() => columns.map((c) => c.name), [columns]);

  return (
    <div className="min-w-max" style={{ height: totalHeight, position: 'relative' }}>
      {virtualRows.map((vRow) => {
        const row = rows[vRow.index] ?? [];
        const selected = selectedRows.has(vRow.index);
        const nextSelected = selectedRows.has(vRow.index + 1);
        const highlighted = highlightedRow === vRow.index;
        return (
          <div
            key={vRow.key}
            tabIndex={0}
            className={cn(
              'absolute left-0 flex w-full cursor-pointer outline-none',
              selected && nextSelected ? 'border-b border-blue-500/10' : 'border-b border-edge/30',
              vRow.index % 2 === 1 ? 'bg-surface-raised/50' : 'bg-surface',
              selected
                ? 'bg-blue-500/15 dark:bg-blue-500/20'
                : highlighted
                  ? 'bg-accent/8 dark:bg-accent/12'
                  : 'hover:bg-surface-raised/50',
            )}
            style={{ top: vRow.start, height: rowHeight }}
            onClick={(e) => {
              onRowSelect(vRow.index, {
                multi: e.metaKey || e.ctrlKey,
                range: e.shiftKey,
              });
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onRowSelect(vRow.index);
              }
            }}
            onDoubleClick={() => {
              const first = colNames[0];
              if (first) onCellDoubleClick(vRow.index, first);
            }}
          >
            <button
              type="button"
              className={cn(
                'flex w-10 shrink-0 items-center justify-center border-r border-edge/30 text-xs text-fg-muted',
                selected && 'border-l-2 border-l-blue-500 text-blue-400 dark:text-blue-300',
              )}
              onClick={(e) => {
                e.stopPropagation();
                onRowSelect(vRow.index, {
                  multi: e.metaKey || e.ctrlKey,
                  range: e.shiftKey,
                });
              }}
              title={t('dataTable.selectRow')}
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
      })}
    </div>
  );
}
