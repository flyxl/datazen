import { ArrowDown, ArrowUp, ArrowUpDown } from 'lucide-react';
import type { SortCondition } from '../../types';
import { cn } from '../../lib/cn';

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
}

export function TableHeader({ columns, sorts, onSort, columnWidths, onResizeStart }: TableHeaderProps) {
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
            className="relative flex h-10 shrink-0 items-center justify-between gap-2 border-r border-edge px-3"
            style={{ width }}
          >
            <div className="min-w-0 truncate text-left">
              <div className="truncate text-xs font-medium text-fg-secondary" title={col.name}>
                {col.name}
              </div>
              {col.type ? (
                <div className="truncate font-mono text-[11px] text-fg-muted" title={col.type}>
                  {col.type}
                </div>
              ) : null}
            </div>
            <button
              type="button"
              className={cn(
                'inline-flex h-7 w-7 items-center justify-center rounded-md text-fg-muted hover:bg-surface hover:text-fg',
                sorted !== 'none' && 'text-blue-400',
              )}
              title="排序"
              onClick={() => {
                if (sorted === 'none') onSort({ column: col.name, descending: false });
                else if (sorted === 'asc') onSort({ column: col.name, descending: true });
                else onSort({ column: col.name, descending: false });
              }}
            >
              {sorted === 'none' ? (
                <ArrowUpDown className="h-4 w-4" />
              ) : sorted === 'asc' ? (
                <ArrowUp className="h-4 w-4" />
              ) : (
                <ArrowDown className="h-4 w-4" />
              )}
            </button>
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
