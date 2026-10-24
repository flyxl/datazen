import { ArrowDown, ArrowUp, Filter } from 'lucide-react';
import type { SortCondition } from '../../types';
import { cn } from '../../lib/cn';
import { dataTypeTextClass } from '../../lib/dataTypeColors';
import type { CellTypeHints } from '../../lib/cellTypes';

export interface ColumnDef {
  id: string;
  name: string;
  type?: string;
  /**
   * The column accepts SQL `NULL`.
   *
   * It is the one piece of column metadata the editor cannot derive from
   * the type name: without it, a grid that has no schema behind it would
   * have to *guess* — and the guess "nullable" turns a `NOT NULL` column
   * into one that silently accepts an empty input as a `NULL` (05 §7 #3).
   */
  nullable?: boolean;
}

export interface TableHeaderProps {
  columns: ColumnDef[];
  sorts: SortCondition[];
  onSort: (sort: SortCondition) => void;
  columnWidths?: number[];
  onResizeStart?: (colIndex: number, startX: number) => void;
  sortable?: boolean;
  onFilterColumn?: (column: string) => void;
  /**
   * The driver's own column-type rules (`DatabaseTypeMeta.cellTypes`).
   *
   * The **same** declaration the cells read: the type label of a header and
   * the value of a cell must not give two different answers for the same
   * column (05 §3.1). Omitted → the shared table alone decides, exactly as
   * before.
   */
  cellTypeHints?: CellTypeHints;
  /**
   * The header belongs to an ARIA grid: the header row becomes `role="row"` and its cells
   * `role="columnheader"`. A `row` may only contain cell children, so the roles are added
   * together with the grid itself (00-contracts §4.4).
   */
  gridSemantics?: boolean;
}

export function TableHeader({
  columns,
  sorts,
  onSort,
  columnWidths,
  onResizeStart,
  sortable = true,
  onFilterColumn,
  cellTypeHints,
  gridSemantics = false,
}: TableHeaderProps) {
  return (
    <div
      role={gridSemantics ? 'row' : undefined}
      className="sticky top-0 z-10 flex min-w-max border-b border-edge bg-surface-alt"
    >
      <div
        role={gridSemantics ? 'columnheader' : undefined}
        className="flex h-10 w-10 shrink-0 items-center justify-center border-r border-edge text-xs text-fg-muted"
      >
        #
      </div>
      {columns.map((col, colIdx) => {
        // DB-11: every column carries its own sort, not just the first one —
        // the list order is the ORDER BY order, so the badge is the
        // column's position in it.
        const sortPosition = sorts.findIndex((sort) => sort.column === col.name);
        const mine = sortPosition >= 0 ? sorts[sortPosition] : undefined;
        const sorted = mine ? (mine.descending ? 'desc' : 'asc') : ('none' as const);
        const width = columnWidths?.[colIdx] ?? 160;

        return (
          <div
            key={col.id}
            role={gridSemantics ? 'columnheader' : undefined}
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
                  className={cn(
                    'truncate font-mono text-[11px]',
                    dataTypeTextClass(col.type, cellTypeHints),
                  )}
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
            {sortable && sorted !== 'none' && sorts.length > 1 && (
              <span
                data-sort-badge
                title={`#${sortPosition + 1}`}
                className="inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-sm bg-accent/15 px-1 font-mono text-[10px] leading-none text-accent"
              >
                {sortPosition + 1}
              </span>
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
