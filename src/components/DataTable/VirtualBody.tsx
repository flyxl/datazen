import { memo, useCallback, useMemo, type Key } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useVirtualTable } from '../../hooks/useVirtualTable';
import type { ColumnDef } from './TableHeader';
import { GridCell } from './GridCell';
import { CellRenderer } from './CellRenderer';
import { EditableCell } from './EditableCell';
import { classifyDataType } from '../../lib/dataTypeColors';
import type { CellCoord } from '../../stores/tableData/gridSelection';
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
  /** Normalised column type, rendered as `data-dt-cell-type` (DB-05 swaps the source). */
  const columnTypes = useMemo(() => columns.map((c) => classifyDataType(c.type)), [columns]);
  const cellSelected = useCallback(
    (rowIndex: number, columnName: string) => isCellSelected?.(rowIndex, columnName) ?? false,
    [isCellSelected],
  );

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
