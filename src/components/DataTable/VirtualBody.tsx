import { memo, useCallback, useMemo, type Key } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useVirtualTable } from '../../hooks/useVirtualTable';
import type { ColumnDef } from './TableHeader';
import { GridCell, type FkCellAffordance } from './GridCell';
import { CellRenderer } from './CellRenderer';
import { EditableCell } from './EditableCell';
import {
  buildColumnEditorInfoFromDefs,
  type CellEditingContext,
  type ColumnEditorInfo,
} from '../../lib/cellEditorKind';
import { dirtyCellAriaLabel, dirtyCellTooltip } from './dirtyCellText';
import { isFkCellNavigable, type FkGroupAffordance } from '../../lib/fkNavigation';
import type { CellCoord } from '../../stores/tableData/gridSelection';
import type { DirtyCell, DirtyRowEntry } from '../../stores/tableData/dirtyIndex';
import type { CellTypeHints } from '../../lib/cellTypes';
import { cn } from '../../lib/cn';
import type { Value } from '../../types';

/**
 * One new-row draft, as the grid renders it.
 *
 * `cells` holds the **written** columns only. A key that is absent means "do not
 * write this column" — the database keeps its default (auto-increment, generated
 * column, `DEFAULT now()`) — while `null` means "write SQL NULL".
 */
export interface DraftRowViewModel {
  /** UI association key; also rendered as `data-draft-id`. */
  draftId: string;
  /** Written columns only; a missing key means the column is not written. */
  cells: Record<string, Value | null>;
}

const NOOP = () => {};
const EMPTY_ROW: unknown[] = [];
const EMPTY_DRAFT_ROWS: DraftRowViewModel[] = [];

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
  /** The editing context of the grid (DB-05). */
  cellEditing?: CellEditingContext;
  /**
   * The driver's own column-type declaration.
   *
   * It is what a grid without an editing context falls back to: the cell type
   * is read from the declaration rather than from the type name alone, so a
   * MySQL `tinyint(1)` is a boolean in the cells exactly as it is in the
   * header and in the drawer.
   */
  cellTypeHints?: CellTypeHints;
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
  /**
   * New-row drafts, rendered **after** the real rows.
   *
   * They are derived rows: a draft is never part of the real row set, so it must
   * not reach identity building, de-duplication, export or sorting (04 §3.7 R-5).
   */
  draftRows?: DraftRowViewModel[];
  /** Write one cell of a draft. `undefined` clears the column (do not write it). */
  onDraftCellEdit?: (draftId: string, column: string, value: Value | null | undefined) => void;
  /** Start editing a draft cell. */
  onDraftCellEditStart?: (draftId: string, column: string) => void;
  /** Leave the draft cell editor without writing anything. */
  onDraftCellEditCancel?: () => void;
  /** Drop a draft row. */
  onDraftRowRemove?: (draftId: string) => void;
  /** The draft row the pointer focus is on (the target of duplicate / discard). */
  onDraftRowActivate?: (draftId: string) => void;
  /** Append a new draft row — the "empty area below the last row" entry. */
  onAddDraftRow?: () => void;
  /** The draft cell that is being edited right now. */
  editingDraft?: { draftId: string; column: string } | null;
  /**
   * The row that was just inserted — the one the commit located.
   *
   * It is a **separate** channel from `highlightedRow`: that one is shared
   * with the detail panel, so a highlighted row could be either. The new row
   * gets a marker **with a word on it**, which is what tells the user which
   * row the commit produced (04 §4.1).
   *
   * `null` (the default) renders no marker at all, so a grid that does not
   * use the feature keeps today's DOM byte for byte.
   */
  newRowIndex?: number | null;
  /**
   * The dirty state of the current page, keyed by row index (DB-06).
   *
   * Omitted / `null` = no dirty marks at all, which is exactly today's DOM.
   */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
  /**
   * The foreign keys of the table (DB-07).
   *
   * Omitted = the table has no foreign keys (or the metadata could not be
   * read): the grid renders exactly what it rendered before.
   *
   * One object, because the pieces belong together: the arrow of a composite
   * key is decided by the values of **all** of its members — including a
   * member the user has hidden — and only the **full** row records carry
   * those values (07 §7 #14). A grid that got the index but not the records
   * would silently lose the arrow of every composite key with a hidden
   * member, so the type makes that combination impossible to write (FKU-D1).
   */
  foreignKeys?: {
    /** The columns that take part in a foreign key, mapped to their group. */
    index: ReadonlyMap<string, FkGroupAffordance>;
    /** The **full** row records (every column of the table), by row index. */
    rows: readonly Record<string, unknown>[];
    /** The displayed column names, in render order. */
    visibleColumns: readonly string[];
    /** The cells whose resolution is in flight, keyed `${rowIndex}:${columnName}`. */
    resolving?: ReadonlySet<string>;
    /** Follow the foreign key of a cell. */
    onJump?: (rowIndex: number, columnName: string) => void;
  };
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
  cellSelectionEnabled: boolean;
  isCellSelected: (rowIndex: number, columnName: string) => boolean;
  activeRow: number | null;
  activeColumnName: string | null;
  cellIdPrefix?: string;
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  cellEditing?: CellEditingContext;
  /** The editor facts of every column (DB-05), computed once by the owner. */
  editorInfo: ReadonlyMap<string, ColumnEditorInfo>;
  onRowSelect: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onRowGutterSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onRowClick?: (index: number) => void;
  /** This row is the one the last insert produced. */
  isNewRow?: boolean;
  /** The wording of the marker; passed down so every row does not subscribe. */
  newRowLabel: string;
  /**
   * This row's dirty state (DB-06). `undefined` = clean.
   *
   * Only **this row's** entry is passed down: handing every row the whole map
   * would change every row's props on every edit and defeat `memo`.
   */
  dirtyRow?: DirtyRowEntry;
  /** The wording of the gutter dot of a row that is marked for deletion. */
  deletedRowLabel: string;
  /** The foreign keys of the table (DB-07). */
  foreignKeys?: VirtualBodyProps['foreignKeys'];
  /** The columns of this row whose cell may be followed (DB-07). */
  fkNavigable?: ReadonlySet<string>;
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
  cellSelectionEnabled,
  isCellSelected,
  activeRow,
  activeColumnName,
  cellIdPrefix,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  cellEditing,
  editorInfo,
  onRowSelect,
  onRowGutterSelect,
  onRowClick,
  isNewRow = false,
  newRowLabel,
  dirtyRow,
  deletedRowLabel,
  foreignKeys,
  fkNavigable,
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

  /**
   * The changed cells of this row, by column name.
   *
   * A deleted row carries no cell marks — the row-level strike-through says
   * the same thing once (06 §4.1, exit transition 4).
   */
  const dirtyCells = useMemo(() => {
    const map = new Map<string, DirtyCell>();
    for (const cell of dirtyRow?.cells ?? []) map.set(cell.columnName, cell);
    return map;
  }, [dirtyRow]);
  const rowDeleted = dirtyRow?.state === 'deleted';
  const rowDirty = dirtyRow?.state === 'dirty';

  /**
   * The foreign-key affordance of every column of this row (DB-07).
   *
   * The objects are built once per row and reused, so `memo(GridCell)` keeps
   * working: a new object on every render would re-render every visible cell
   * of the page on every scroll.
   */
  const fkAffordances = useMemo(() => {
    const fkIndex = foreignKeys?.index;
    if (!fkIndex || fkIndex.size === 0) return null;
    const map = new Map<string, FkCellAffordance>();
    for (const [columnName, group] of fkIndex) {
      map.set(columnName, {
        targetLabel: group.targetLabel,
        navigable: fkNavigable?.has(columnName) ?? false,
        resolving: foreignKeys.resolving?.has(`${vRow.index}:${columnName}`) ?? false,
      });
    }
    return map;
  }, [foreignKeys, fkNavigable, vRow.index]);

  return (
    <div
      key={vRow.key}
      // Roving tabindex (DB-02 §4.4): the scroll container is the only tab stop of the
      // grid, the rows are reachable with the arrow keys.
      tabIndex={-1}
      role={cellSelectionEnabled ? 'row' : undefined}
      data-dt-row-view={vRow.index}
      data-dt-row-state={dirtyRow?.state ?? undefined}
      // The row's own selection state (the cells carry `data-dt-selected` too, so a
      // locator has to name the row: `[data-dt-row-view][data-dt-selected]`).
      data-dt-selected={selected ? 'true' : undefined}
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
        // A row that is marked for deletion reads as struck through. The
        // strike-through colour is a state token; the value colour (the cell's
        // own type colour) is untouched.
        rowDeleted && 'line-through opacity-60 decoration-dt-deleted-fg',
      )}
      style={{ top: vRow.start, height: rowHeight }}
      onClick={handleClick}
      onDoubleClick={handleRowDoubleClick}
    >
      {isNewRow && (
        <span
          data-testid="new-row-badge"
          className="absolute right-1 top-1/2 z-10 -translate-y-1/2 rounded border border-accent/60 bg-surface/90 px-1 text-[10px] font-medium text-accent"
        >
          {newRowLabel}
        </span>
      )}
      {/* The row-number slot is the row header: a `row` may only contain gridcell /
          rowheader / columnheader children, so the bare button has to be wrapped. */}
      <div
        role={cellSelectionEnabled ? 'rowheader' : undefined}
        className={cn(
          'flex w-10 shrink-0 items-center border-r border-edge/30',
          (rowDirty || rowDeleted) && 'relative',
        )}
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
        {(rowDirty || rowDeleted) && (
          // The row-level dot: the gutter says *that* row has something
          // pending, the cell marks say *which* cells.
          <span
            data-testid={rowDeleted ? 'row-deleted-dot' : 'row-dirty-dot'}
            title={rowDeleted ? deletedRowLabel : undefined}
            aria-hidden={rowDeleted ? undefined : 'true'}
            className={cn(
              'pointer-events-none absolute right-0.5 top-0.5 h-1.5 w-1.5 rounded-full',
              rowDeleted ? 'bg-danger' : 'bg-dt-dirty-mark',
            )}
          />
        )}
      </div>
      {columns.map((col, colIdx) => {
        const value = row[colIdx];
        const isEditing = editingCell?.row === vRow.index && editingCell.col === col.name;
        const colW = columnWidths?.[colIdx] ?? 160;
        const cellId = cellIdPrefix ? `${cellIdPrefix}-r${vRow.index}c${colIdx}` : undefined;
        const dirtyCell = dirtyCells.get(col.name);
        const fk = fkAffordances?.get(col.name) ?? null;
        return (
          <GridCell
            key={col.id}
            id={cellId}
            rowIndex={vRow.index}
            column={col}
            value={value}
            width={colW}
            dirty={dirtyCell !== undefined}
            dirtyTooltip={
              dirtyCell ? dirtyCellTooltip(dirtyCell.original, dirtyCell.next) : undefined
            }
            dirtyAriaLabel={
              dirtyCell ? dirtyCellAriaLabel(dirtyCell.original, dirtyCell.next) : undefined
            }
            // The cell renders the arrow only when it is not occupied by an
            // editor, so the affordance itself stays a stable object.
            fk={fk}
            onFkActivate={foreignKeys?.onJump}
            selected={cellSelectionEnabled && isCellSelected(vRow.index, col.name)}
            active={
              cellSelectionEnabled &&
              activeRow === vRow.index &&
              activeColumnName === col.name
            }
            editing={isEditing}
            cellType={editorInfo.get(col.name)?.cellType}
            columnInfo={editorInfo.get(col.name)}
            editor={cellEditing}
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

interface DraftRowProps {
  vRow: { index: number; key: Key; start: number };
  draft: DraftRowViewModel;
  /** Position of the draft among the drafts; it is the `data-testid` suffix. */
  draftIndex: number;
  columns: ColumnDef[];
  rowHeight: number;
  columnWidths?: number[];
  editing: { draftId: string; column: string } | null;
  cellSelectionEnabled: boolean;
  /** The panel the draft belongs to (the editor registers with the guard). */
  panelId?: string;
  onDraftCellEdit?: (draftId: string, column: string, value: Value | null | undefined) => void;
  onDraftCellEditStart?: (draftId: string, column: string) => void;
  onDraftCellEditCancel?: () => void;
  onDraftRowRemove?: (draftId: string) => void;
  onDraftRowActivate?: (draftId: string) => void;
}

/**
 * One new-row draft.
 *
 * The row is deliberately **outside** the cell-selection contract: a draft has
 * no identity and is not a row of the table yet, so it must not answer the
 * selection queries of the grid (04 §3.7 R-6). It therefore carries its own
 * `data-row-kind="draft"` marker and its own `data-testid="draft-row-N"`.
 */
const DraftRow = memo(function DraftRow({
  vRow,
  draft,
  draftIndex,
  columns,
  rowHeight,
  columnWidths,
  editing,
  cellSelectionEnabled,
  panelId,
  onDraftCellEdit,
  onDraftCellEditStart,
  onDraftCellEditCancel,
  onDraftRowRemove,
  onDraftRowActivate,
}: DraftRowProps) {
  const { t } = useI18n();
  const badgeLabel = t('tableData.draftRowBadge');
  const defaultLabel = t('tableData.insertDefault');
  const unsavedLabel = t('tableData.draftRowUnsaved');
  const removeLabel = t('tableData.removeDraftRow');

  return (
    <div
      data-testid={`draft-row-${draftIndex}`}
      data-row-kind="draft"
      data-draft-id={draft.draftId}
      role={cellSelectionEnabled ? 'row' : undefined}
      aria-label={unsavedLabel}
      className="absolute left-0 flex w-full border-b border-dashed border-accent/40 bg-accent/5 outline-none"
      style={{ top: vRow.start, height: rowHeight }}
      onClick={() => onDraftRowActivate?.(draft.draftId)}
    >
      <div
        role={cellSelectionEnabled ? 'rowheader' : undefined}
        className="flex w-10 shrink-0 items-center justify-center gap-0.5 border-r border-edge/30"
      >
        <span data-testid="draft-row-badge" className="text-[10px] font-semibold text-accent">
          {badgeLabel}
        </span>
        <button
          type="button"
          tabIndex={-1}
          data-testid="draft-row-remove"
          aria-label={removeLabel}
          title={removeLabel}
          className="flex h-3.5 w-3.5 items-center justify-center rounded text-[10px] text-fg-muted hover:bg-surface-raised hover:text-danger"
          onClick={(e) => {
            e.stopPropagation();
            onDraftRowRemove?.(draft.draftId);
          }}
        >
          ×
        </button>
      </div>
      {columns.map((col, colIdx) => {
        // A missing key is the third state of the write intent: the column is
        // not written at all, so the cell shows the DEFAULT placeholder.
        const written = Object.prototype.hasOwnProperty.call(draft.cells, col.name);
        const value = written ? draft.cells[col.name] : undefined;
        const isEditing =
          editing !== null && editing.draftId === draft.draftId && editing.column === col.name;
        const colW = columnWidths?.[colIdx] ?? 160;
        return (
          <div
            key={col.id}
            data-testid="draft-cell"
            data-draft-col={col.name}
            data-draft-state={written ? (value === null ? 'null' : 'value') : 'default'}
            className="flex shrink-0 items-center overflow-hidden border-r border-edge/30 px-2"
            style={{ width: colW }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              onDraftCellEditStart?.(draft.draftId, col.name);
            }}
          >
            {isEditing ? (
              <EditableCell
                value={value ?? null}
                type={(col.type ?? '').toLowerCase()}
                panelId={panelId}
                onCommit={(committed) =>
                  onDraftCellEdit?.(
                    draft.draftId,
                    col.name,
                    committed === null ? undefined : (committed as Value),
                  )
                }
                onCancel={() => onDraftCellEditCancel?.()}
              />
            ) : written ? (
              <CellRenderer
                columnName={col.name}
                dataType={col.type}
                value={value ?? null}
                isEditing={false}
                onCommit={NOOP}
                onCancel={NOOP}
              />
            ) : (
              <span
                data-testid="draft-cell-default"
                className="truncate text-xs italic text-fg-muted"
              >
                {defaultLabel}
              </span>
            )}
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
  cellEditing,
  cellTypeHints,
  onRowSelect,
  cellSelectionEnabled = false,
  isCellSelected,
  activeCell = null,
  cellIdPrefix,
  onRowGutterSelect,
  onRowClick,
  draftRows = EMPTY_DRAFT_ROWS,
  onDraftCellEdit,
  onDraftCellEditStart,
  onDraftCellEditCancel,
  onDraftRowRemove,
  onDraftRowActivate,
  onAddDraftRow,
  editingDraft = null,
  newRowIndex = null,
  dirtyByRow = null,
  foreignKeys,
}: VirtualBodyProps) {
  const { t } = useI18n();
  /**
   * The drafts take part in the virtualisation, so the virtual list is one row
   * longer than the real row set. The placeholder entries only carry the
   * length; the draft content is rendered from `draftRows`.
   */
  const displayRows = useMemo(
    () => (draftRows.length === 0 ? rows : [...rows, ...draftRows.map(() => EMPTY_ROW)]),
    [rows, draftRows],
  );
  const { virtualRows, totalHeight } = useVirtualTable({
    rows: displayRows,
    rowHeight,
    overscan: 12,
    scrollElement,
  });

  const colNames = columns.map((c) => c.name);
  const selectRowLabel = t('dataTable.selectRow');
  /** The wording of the marker of a freshly inserted row. */
  const newRowLabel = t('tableData.highlightNewRow');
  /** The wording of the gutter dot of a row that is marked for deletion. */
  const deletedRowLabel = t('tableData.dirtyDeletedRow');
  /**
   * The editor facts of every column (DB-05).
   *
   * The owner normally computes them once and hands them over; a grid
   * without schema behind it (the result grid) falls back to the column
   * definitions. Either way the normalisation happens **once per column**,
   * never once per cell.
   *
   * **An empty map means "the owner knows nothing"**, and it takes the same
   * fallback as a missing one: `??` would only catch `undefined`, and an
   * empty map would silently take every column's type away — the cell would
   * lose its `data-dt-cell-type`, its nullability and its integer check.
   */
  const editorInfo = useMemo(() => {
    const provided = cellEditing?.columnEditorInfo;
    return provided && provided.size > 0
      ? provided
      : buildColumnEditorInfoFromDefs(columns, cellTypeHints);
  }, [cellEditing, columns, cellTypeHints]);
  const cellSelected = useCallback(
    (rowIndex: number, columnName: string) => isCellSelected?.(rowIndex, columnName) ?? false,
    [isCellSelected],
  );
  /**
   * The columns whose cell may be followed, per row (DB-07).
   *
   * A composite key is navigable as a **whole**: one member without a value
   * makes the group unlocatable, because "where a = 1" alone would point at
   * the wrong parent row. The arrow itself lands on the group's first
   * **visible** member, so hiding a column moves the arrow instead of losing
   * it.
   *
   * The values come from the **full** row record (`fkRows`): the positional
   * rows carry only the displayed columns, so a hidden member would look
   * like a member without a value — and the whole group would lose its
   * arrow. Without the full records the answer is "unknown", and an unknown
   * member is treated as a missing one: the grid does not guess.
   */
  const fkNavigableByRow = useMemo(() => {
    const map = new Map<number, ReadonlySet<string>>();
    const fkIndex = foreignKeys?.index;
    if (!fkIndex || fkIndex.size === 0) return map;
    const visible = foreignKeys.visibleColumns;
    const fkRows = foreignKeys.rows;
    const columnIndexes = new Map(columns.map((column, index) => [column.name, index]));
    rows.forEach((row, rowIndex) => {
      const full = fkRows[rowIndex];
      // The values of the foreign-key members of this row, by column name.
      const record: Record<string, unknown> = {};
      for (const columnName of fkIndex.keys()) {
        if (full) {
          record[columnName] = full[columnName];
          continue;
        }
        const columnIndex = columnIndexes.get(columnName);
        record[columnName] = columnIndex === undefined ? undefined : row[columnIndex];
      }
      const navigable = new Set<string>();
      for (const columnName of fkIndex.keys()) {
        const navigates = isFkCellNavigable({
          columnName,
          row: record,
          index: fkIndex,
          visibleColumns: visible,
          editing: false,
        });
        if (navigates) navigable.add(columnName);
      }
      map.set(rowIndex, navigable);
    });
    return map;
  }, [rows, columns, foreignKeys]);
  return (
    <div
      className="min-w-max"
      style={{ height: totalHeight + (onAddDraftRow ? rowHeight : 0), position: 'relative' }}
      role={cellSelectionEnabled ? 'rowgroup' : undefined}
    >
      {virtualRows.map((vRow) =>
        vRow.index < rows.length ? (
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
            cellSelectionEnabled={cellSelectionEnabled}
            isCellSelected={cellSelected}
            activeRow={activeCell?.rowIndex ?? null}
            activeColumnName={activeCell?.columnName ?? null}
            cellIdPrefix={cellIdPrefix}
            onCellDoubleClick={onCellDoubleClick}
            onCellEdit={onCellEdit}
            onCellEditCancel={onCellEditCancel}
            cellEditing={cellEditing}
            editorInfo={editorInfo}
            onRowSelect={onRowSelect}
            onRowGutterSelect={onRowGutterSelect}
            onRowClick={onRowClick}
            isNewRow={newRowIndex !== null && newRowIndex === vRow.index}
            newRowLabel={newRowLabel}
            dirtyRow={dirtyByRow?.get(vRow.index)}
            deletedRowLabel={deletedRowLabel}
            foreignKeys={foreignKeys}
            fkNavigable={fkNavigableByRow.get(vRow.index)}
          />
        ) : (
          <DraftRow
            key={String(vRow.key)}
            vRow={vRow}
            draft={draftRows[vRow.index - rows.length]}
            draftIndex={vRow.index - rows.length}
            columns={columns}
            rowHeight={rowHeight}
            columnWidths={columnWidths}
            editing={editingDraft}
            cellSelectionEnabled={cellSelectionEnabled}
            panelId={cellEditing?.panelId}
            onDraftCellEdit={onDraftCellEdit}
            onDraftCellEditStart={onDraftCellEditStart}
            onDraftCellEditCancel={onDraftCellEditCancel}
            onDraftRowRemove={onDraftRowRemove}
            onDraftRowActivate={onDraftRowActivate}
          />
        ),
      )}
      {onAddDraftRow && (
        <div
          data-testid="draft-add-row"
          className="absolute left-0 w-full"
          style={{ top: totalHeight, height: rowHeight }}
          onDoubleClick={onAddDraftRow}
        />
      )}
    </div>
  );
});
