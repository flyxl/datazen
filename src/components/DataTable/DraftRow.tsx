/**
 * One new-row draft (DB-04), moved out of `VirtualBody.tsx` when DB-03 pushed that file to
 * its size ceiling. The body is **moved, not rewritten**: same markup, same attributes, same
 * conditions. `DraftRowViewModel` stays next to the grid because it is part of the grid's
 * public props.
 */
import { memo, type Key } from 'react';
import { useI18n } from '../../hooks/useI18n';
import type { ColumnDef } from './TableHeader';
import { CellRenderer } from './CellRenderer';
import { EditableCell } from './EditableCell';
import type { Value } from '../../types';
import type { DraftRowViewModel } from './VirtualBody';

const NOOP = () => {};

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
export const DraftRow = memo(function DraftRow({
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
