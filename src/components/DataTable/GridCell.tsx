import { memo } from 'react';
import type { ColumnDef } from './TableHeader';
import { CellRenderer } from './CellRenderer';
import { cn } from '../../lib/cn';

/**
 * One grid cell — the single owner of the cell-level DOM contract.
 *
 * Everything that needs a `data-dt-*` attribute on a cell (DB-01 selection, DB-05 editing,
 * DB-06 dirty state, DB-07 foreign keys) goes through this component, so the contract has
 * exactly one place to live (00-contracts §4.4).
 *
 * The attributes are **added** to the existing ones; `data-dt-row` / `data-dt-col` /
 * `data-testid="data-table-cell"` stay on the outermost element, because
 * `resolveDataTableCellFromEvent` and the existing E2E specs look them up there.
 */
export interface GridCellProps {
  /** Row index inside the current page. Render-only, never used to build SQL. */
  rowIndex: number;
  /** Column this cell belongs to. */
  column: ColumnDef;
  /** Cell value as rendered by the grid (positional array). */
  value: unknown;
  /** Rendered width in pixels. */
  width: number;
  /** The cell is inside the current cell selection. */
  selected: boolean;
  /** The cell is the focus corner of the selection (pointer/keyboard cursor). */
  active?: boolean;
  /** The cell is being edited right now. */
  editing: boolean;
  /**
   * Normalised type name rendered as `data-dt-cell-type`. The value is computed by the
   * parent (DB-01 uses `classifyDataType`, DB-05 swaps the source for `resolveCellType`),
   * this component only renders it — one attribute, one producer.
   */
  cellType?: string;
  /** DOM id, referenced by the grid's `aria-activedescendant`. */
  id?: string;
  /**
   * Grid semantics are on: the cell renders `role="gridcell"` and `aria-selected`.
   * Off for grids that are not an ARIA grid (a grid without a `role="grid"` parent would
   * be invalid ARIA, so the roles are added as one set or not at all).
   */
  gridSemantics?: boolean;
  /**
   * The cell has an uncommitted change (DB-06).
   *
   * The attribute is the **only** contract the grid needs: `data-dt-dirty="true"`
   * when the cell is dirty, no attribute at all when it is clean (00-contracts
   * §4.4 — `"true"` / absent, never `"false"`).
   */
  dirty?: boolean;
  /** The tooltip of a dirty cell: the original value and the value it becomes. */
  dirtyTooltip?: string;
  /** The screen-reader label of a dirty cell. */
  dirtyAriaLabel?: string;
  onCellDoubleClick: (rowIndex: number, columnName: string) => void;
  onCellEdit: (rowIndex: number, columnName: string, value: unknown) => void;
  onCellEditCancel: () => void;
}

export const GridCell = memo(function GridCell({
  rowIndex,
  column,
  value,
  width,
  selected,
  active = false,
  editing,
  cellType,
  id,
  gridSemantics = false,
  dirty = false,
  dirtyTooltip,
  dirtyAriaLabel,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
}: GridCellProps) {
  return (
    <div
      id={id}
      role={gridSemantics ? 'gridcell' : undefined}
      data-testid="data-table-cell"
      data-dt-row={rowIndex}
      data-dt-col={column.name}
      {...(selected ? { 'data-dt-selected': 'true' } : {})}
      {...(active ? { 'data-dt-active': 'true' } : {})}
      {...(cellType ? { 'data-dt-cell-type': cellType } : {})}
      {...(dirty ? { 'data-dt-dirty': 'true' } : {})}
      aria-selected={gridSemantics && selected ? 'true' : undefined}
      aria-label={dirty ? dirtyAriaLabel : undefined}
      title={dirty ? dirtyTooltip : undefined}
      className={cn(
        'flex shrink-0 items-center overflow-hidden border-r border-edge/30 px-2',
        // The dirty mark is a *state* channel, kept apart from the value's
        // type colour: it drives the border and the background only.
        dirty && 'relative border-l-2 border-l-dt-dirty-border',
        // A selected cell keeps the selection background — the dirty channel
        // is the border and the corner mark there (06 §11 D-8).
        dirty && !selected && 'bg-dt-dirty-bg',
      )}
      style={{ width }}
      onDoubleClick={(e) => {
        e.stopPropagation();
        onCellDoubleClick(rowIndex, column.name);
      }}
    >
      <div className="min-w-0 flex-1">
        <CellRenderer
          columnName={column.name}
          dataType={column.type}
          value={value}
          isEditing={editing}
          onCommit={(v) => onCellEdit(rowIndex, column.name, v)}
          onCancel={onCellEditCancel}
        />
      </div>
      {dirty && (
        // The corner mark: the second shape of the dirty channel. It is a
        // rotated square clipped by the cell's own `overflow-hidden`, so the
        // shape does not depend on the theme.
        <div
          aria-hidden="true"
          className="pointer-events-none absolute right-0 top-0 h-1 w-1 rotate-45 bg-dt-dirty-mark"
        />
      )}
    </div>
  );
});
