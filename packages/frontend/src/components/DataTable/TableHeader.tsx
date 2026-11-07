import { useState } from 'react';
import { ArrowDown, ArrowUp, ChevronDown, Filter } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
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
  /**
   * Remove one column from the sort list — the third state of the header's
   * click cycle (DBR-SORT, Navicat-aligned).
   *
   * Omitted ⇒ the cycle stays two-state (asc ⇄ desc), exactly as grids that
   * cannot remove a sort rendered before; the header menu greys the entry out
   * rather than offering an action it cannot take.
   */
  onRemoveSort?: (column: string) => void;
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
  /**
   * Reorder by dragging a header onto another one (DB-12): the callback gets
   * the displayed indexes, `from` → `to`. Omitted ⇒ headers are not
   * draggable, exactly as before.
   */
  onReorderColumns?: (from: number, to: number) => void;
  /**
   * How many leading columns are frozen (DB-12). `0`/omitted freezes
   * nothing — the header renders byte for byte as before.
   */
  frozenColumns?: number;
  /**
   * The row-number gutter (DB-17). `true`/omitted renders the `#` slot,
   * exactly as before; `false` drops it and the frozen columns pin from the
   * grid's left edge instead of behind the gutter.
   */
  showRowNumbers?: boolean;
}

export function TableHeader({
  columns,
  sorts,
  onSort,
  onRemoveSort,
  columnWidths,
  onResizeStart,
  sortable = true,
  onFilterColumn,
  cellTypeHints,
  gridSemantics = false,
  onReorderColumns,
  frozenColumns = 0,
  showRowNumbers = true,
}: TableHeaderProps) {
  const { t } = useI18n();
  /** Where a drag came from and where it currently hovers (DB-12). */
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);

  /**
   * The header's own sort menu (DBR-SORT): three single-column entries, opened
   * from the chevron every sortable column carries. The entries are **replace**
   * semantics — the Navicat behaviour the click cycle also implements — while
   * the right-click menu keeps its multi-column builder; the two never mix.
   */
  const openSortMenu = (event: React.MouseEvent<HTMLButtonElement>, column: string, sorted: 'asc' | 'desc' | 'none') => {
    event.stopPropagation();
    const rect = event.currentTarget.getBoundingClientRect();
    void showNativeContextMenu(
      [
        {
          kind: 'item',
          id: 'sort-ascending',
          label: t('dataTable.sortColumnAscending'),
          action: () => onSort({ column, descending: false }),
        },
        {
          kind: 'item',
          id: 'sort-descending',
          label: t('dataTable.sortColumnDescending'),
          action: () => onSort({ column, descending: true }),
        },
        {
          kind: 'item',
          id: 'remove-sort',
          label: t('dataTable.removeSort'),
          // A column that is not part of the sort has nothing to remove — the
          // entry answers "how do I undo this", so it is greyed, not hidden.
          enabled: onRemoveSort != null && sorted !== 'none',
          action: () => onRemoveSort?.(column),
        },
      ],
      { x: rect.left, y: rect.bottom + 2 },
    );
  };

  // The left offsets of the frozen headers (DB-12): whatever pins first
  // (the `#` gutter, or the first column itself when the gutter is off,
  // DB-17) pins at 0 and each frozen column pins right after the one before it.
  const frozenLefts: number[] = [];
  if (frozenColumns > 0) {
    let left = showRowNumbers ? 40 : 0; // the `#` gutter is `w-10`
    for (let i = 0; i < columns.length; i += 1) {
      frozenLefts[i] = left;
      if (i < frozenColumns) left += columnWidths?.[i] ?? 160;
    }
  }

  return (
    <div
      role={gridSemantics ? 'row' : undefined}
      className="sticky top-0 z-10 flex min-w-max border-b border-edge bg-surface-alt"
    >
      {showRowNumbers && (
        <div
          role={gridSemantics ? 'columnheader' : undefined}
          className={cn(
            'flex h-10 w-10 shrink-0 items-center justify-center border-r border-edge text-xs text-fg-muted',
            frozenColumns > 0 && 'sticky left-0 z-20 bg-surface-alt',
          )}
        >
          #
        </div>
      )}
      {columns.map((col, colIdx) => {
        // DB-11: every column carries its own sort, not just the first one —
        // the list order is the ORDER BY order, so the badge is the
        // column's position in it.
        const sortPosition = sorts.findIndex((sort) => sort.column === col.name);
        const mine = sortPosition >= 0 ? sorts[sortPosition] : undefined;
        const sorted = mine ? (mine.descending ? 'desc' : 'asc') : ('none' as const);
        const width = columnWidths?.[colIdx] ?? 160;
        const frozen = frozenColumns > colIdx;
        const lastFrozen = frozen && colIdx === frozenColumns - 1;

        return (
          <div
            key={col.id}
            role={gridSemantics ? 'columnheader' : undefined}
            data-col-header={col.name}
            aria-sort={
              gridSemantics
                ? sorted === 'asc'
                  ? 'ascending'
                  : sorted === 'desc'
                    ? 'descending'
                    : undefined
                : undefined
            }
            draggable={onReorderColumns != null}
            onDragStart={(e) => {
              if (onReorderColumns == null) return;
              setDragIndex(colIdx);
              setDropIndex(null);
              // `dataTransfer` can be null (a synthetic event in a test, an
              // engine without a drag clipboard): the drag itself still moves
              // the column, only the drag image keeps the default.
              if (e.dataTransfer) {
                e.dataTransfer.effectAllowed = 'move';
                // Without a payload some engines cancel the drag immediately.
                e.dataTransfer.setData('text/plain', col.name);
              }
            }}
            onDragOver={(e) => {
              if (onReorderColumns == null || dragIndex == null) return;
              e.preventDefault();
              if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
              setDropIndex(colIdx);
            }}
            onDragLeave={() => {
              if (dropIndex === colIdx) setDropIndex(null);
            }}
            onDrop={(e) => {
              if (onReorderColumns == null || dragIndex == null) return;
              e.preventDefault();
              if (dragIndex !== colIdx) onReorderColumns(dragIndex, colIdx);
              setDragIndex(null);
              setDropIndex(null);
            }}
            onDragEnd={() => {
              setDragIndex(null);
              setDropIndex(null);
            }}
            className={cn(
              'group relative flex h-10 shrink-0 select-none items-center justify-between gap-2 border-r border-edge px-3',
              // The frozen edge (DB-12): the last pinned column needs a
              // visible seam so the scroll boundary reads as intentional.
              lastFrozen && 'border-r-2 border-r-accent/40',
              // The drop target of a header drag shows where the column
              // will land (DB-12).
              dropIndex === colIdx && dragIndex != null && dragIndex !== colIdx
                ? 'border-l-2 border-l-accent'
                : undefined,
            )}
            style={{
              width,
              ...(frozen ? { position: 'sticky', left: frozenLefts[colIdx], zIndex: 20 } : {}),
            }}
            onClick={() => {
              if (!sortable) return;
              // The Navicat cycle (DBR-SORT): unsorted → ascending →
              // descending → removed. Removing falls back to the default
              // order — an explicit sort the user made is what cycles, the
              // table's implicit order is not theirs to lose here. A grid
              // without a remove channel wraps back to ascending instead.
              if (sorted === 'none') {
                onSort({ column: col.name, descending: false });
              } else if (sorted === 'asc') {
                onSort({ column: col.name, descending: true });
              } else if (onRemoveSort) {
                onRemoveSort(col.name);
              } else {
                onSort({ column: col.name, descending: false });
              }
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
              <span
                data-sort-indicator
                aria-hidden
                className="inline-flex h-4 w-4 shrink-0 items-center justify-center text-accent"
              >
                {sorted === 'asc' ? (
                  <ArrowUp className="h-3.5 w-3.5" />
                ) : (
                  <ArrowDown className="h-3.5 w-3.5" />
                )}
              </span>
            )}
            {sortable && (
              <button
                type="button"
                data-sort-menu
                className={cn(
                  'inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-fg-muted opacity-0 transition-opacity hover:bg-surface hover:text-accent',
                  'focus-visible:opacity-100 group-hover:opacity-100',
                )}
                onClick={(e) => openSortMenu(e, col.name, sorted)}
                aria-label={t('dataTable.sortMenuLabel')}
                title={t('dataTable.sortMenuLabel')}
              >
                <ChevronDown className="h-3.5 w-3.5" />
              </button>
            )}
            {onResizeStart && (
              <div
                className="absolute right-0 top-0 z-20 h-full w-[5px] cursor-col-resize hover:bg-accent/40 active:bg-accent/60"
                onPointerDown={(e) => {
                  e.preventDefault();
                  onResizeStart(colIdx, e.clientX);
                }}
                // A resize drag starts with a pointer down on this strip; the
                // click that follows must not sort the column the strip belongs
                // to (DBR-SORT made the whole cell click-sensitive).
                onClick={(e) => e.stopPropagation()}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
