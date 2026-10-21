import { memo } from 'react';
import { ArrowUpRight } from 'lucide-react';
import type { ColumnDef } from './TableHeader';
import { CellRenderer } from './CellRenderer';
import { Spinner } from '../ui/Spinner';
import { fkJumpLabel, fkResolvingLabel } from './fkCellText';
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

/**
 * What the grid knows about the foreign key of one cell (DB-07).
 *
 * `navigable` is the arrow's own condition: the whole group must have a value
 * in this row, this cell must be the group's **first visible member**, and the
 * cell must not be busy with an editor. A composite key draws exactly one
 * arrow, on the first member the user can actually see.
 */
export interface FkCellAffordance {
  /** The display label of the target (`customers.id`). */
  targetLabel: string;
  /** This cell carries the jump arrow. */
  navigable: boolean;
  /** A resolution for this target is in flight. */
  resolving: boolean;
}

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
  /**
   * The foreign key of this cell, or `null` when the column is not a member of
   * one (DB-07).
   *
   * `null` is the default, and a cell without it renders exactly what it
   * rendered before the foreign keys existed.
   */
  fk?: FkCellAffordance | null;
  /**
   * Follow the foreign key of this cell (DB-07).
   *
   * The handler receives the coordinates instead of being bound per cell: an
   * inline closure per cell would give every cell a new prop identity and
   * defeat `memo`, which a virtualised grid feels immediately.
   */
  onFkActivate?: (rowIndex: number, columnName: string) => void;
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
  fk = null,
  onFkActivate,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
}: GridCellProps) {
  /**
   * The wording of the jump entry.
   *
   * A composite key names the table and the columns, a single-column key names
   * the target column: the tooltip says **where** the arrow goes, not merely
   * that it goes somewhere.
   */
  const fkTitle = fk ? (fk.resolving ? fkResolvingLabel() : fkJumpLabel(fk.targetLabel)) : null;
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
      // The cell belongs to a foreign key. The **value** of the attribute is the
      // marker; the target itself is in the tooltip (00-contracts §4.4).
      {...(fk ? { 'data-dt-fk': 'true' } : {})}
      aria-selected={gridSemantics && selected ? 'true' : undefined}
      aria-label={dirty ? dirtyAriaLabel : fkTitle ?? undefined}
      title={dirty ? dirtyTooltip : fkTitle ?? undefined}
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
      {fk?.navigable && !editing && (
        // The jump entry (DB-07). It is a real button: it can be reached with
        // the keyboard, and Enter/Space do what a click does.
        // An editor owns the cell it occupies, so the arrow steps aside.
        <button
          type="button"
          data-dt-fk-click="true"
          aria-label={fkTitle ?? undefined}
          title={fkTitle ?? undefined}
          disabled={fk.resolving}
          className="ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:bg-surface-raised hover:text-accent disabled:opacity-60"
          onClick={(e) => {
            // The click must not reach the row: a jump and a row selection are
            // two different actions, and a double click on the arrow must not
            // open the cell editor either.
            e.stopPropagation();
            onFkActivate?.(rowIndex, column.name);
          }}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {fk.resolving ? <Spinner size="xs" /> : <ArrowUpRight className="h-3 w-3" />}
        </button>
      )}
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
