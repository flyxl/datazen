import { ArrowDown, ArrowUp, Filter } from 'lucide-react';
import type { SortCondition } from '../../types';
import { cn } from '../../lib/cn';
import { dataTypeTextClass } from '../../lib/dataTypeColors';

export interface ColumnDef {
  id: string;
  name: string;
  type?: string;
}

export interface TableHeaderProps {
  columns: ColumnDef[];
  sorts: SortCondition[];
  onSort: (sort: SortCondition) => void;
  columnWidths?: number[];
  onResizeStart?: (colIndex: number, startX: number) => void;
  sortable?: boolean;
  onFilterColumn?: (column: string) => void;
}

export function TableHeader({
  columns,
  sorts,
  onSort,
  columnWidths,
  onResizeStart,
  sortable = true,
  onFilterColumn,
}: TableHeaderProps) {
  const active = sorts[0];

  return (
    <div className="sticky top-0 z-10 flex min-w-max border-b border-edge bg-surface-alt">
      <div className="flex h-10 w-10 shrink-0 items-center justify-center border-r border-edge text-xs text-fg-muted">
        #
      </div>
      {columns.map((col, colIdx) => {
        const sorted =
          active?.column === col.name ? (active.descending ? 'desc' : 'asc') : ('none' as const);
        const width = columnWidths?.[colIdx] ?? 160;

        return (
          <div
            key={col.id}
            data-col-header={col.name}
            className="relative flex h-10 shrink-0 items-center justify-between gap-2 border-r border-edge px-3"
            style={{ width }}
            onDoubleClick={() => {
              if (!sortable) return;
              if (sorted === 'none') onSort({ column: col.name, descending: false });
              else if (sorted === 'asc') onSort({ column: col.name, descending: true });
              else onSort({ column: col.name, descending: false });
            }}
          >
            <div data-col-label className="min-w-0 flex-1 truncate text-left">
              <div className="truncate text-xs font-medium text-fg-secondary" title={col.name}>
                {col.name}
              </div>
              {col.type ? (
                <div
                  className={cn('truncate font-mono text-[11px]', dataTypeTextClass(col.type))}
                  title={col.type}
                >
                  {col.type}
                </div>
              ) : null}
            </div>
            {onFilterColumn && (
              <button
                type="button"
                className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-fg-muted hover:bg-surface hover:text-accent"
                onClick={(e) => {
                  e.stopPropagation();
                  onFilterColumn(col.name);
                }}
                aria-label="Filter"
              >
                <Filter className="h-3.5 w-3.5" />
              </button>
            )}
            {sortable && sorted !== 'none' && (
              <button
                type="button"
                data-sort-icon
                className={cn(
                  'inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-accent hover:bg-surface hover:text-accent/80',
                )}
                onClick={(e) => {
                  e.stopPropagation();
                  onSort({ column: col.name, descending: sorted === 'asc' });
                }}
              >
                {sorted === 'asc' ? (
                  <ArrowUp className="h-4 w-4" />
                ) : (
                  <ArrowDown className="h-4 w-4" />
                )}
              </button>
            )}
            {onResizeStart && (
              <div
                className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                onPointerDown={(e) => {
                  e.preventDefault();
                  onResizeStart(colIdx, e.clientX);
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
