import { useCallback, useRef } from 'react';
import { cn } from '@datazen/ui';
import { X } from 'lucide-react';
import { useI18n } from '../../../hooks/useI18n';
import type { ColumnInfo } from '../../../types';
import {
  CARD_HEADER_HEIGHT,
  CARD_LIST_MAX_HEIGHT,
  CARD_LIST_PADDING_Y,
  CARD_ROW_HEIGHT,
  CARD_WIDTH,
  cardListScrolls,
  resolveDragPosition,
  type CardPositions,
} from './cardLayout';

/** A single table card rendered on the diagram canvas. */
export interface TableCardProps {
  tableName: string;
  alias?: string;
  columns: ColumnInfo[];
  /** Column names that are currently selected (checked). */
  selectedColumns: string[];
  /** Primary key column names (optional, derived from schema). */
  primaryKeyColumns?: string[];
  /** Map of column name → foreign key target table (optional). */
  foreignKeyMap?: Record<string, string>;
  position: { x: number; y: number };
  /** Positions of every card, used to align this one while dragging. */
  otherPositions?: CardPositions;
  onToggleColumn: (column: string) => void;
  /** Check/uncheck every column at once. */
  onToggleAllColumns: (selected: boolean) => void;
  /** Remove the table (and everything referencing it) from the query. */
  onRemove: () => void;
  onDragEnd: (pos: { x: number; y: number }) => void;
  onSetAlias: (alias: string) => void;
  /**
   * Reports the column list's scroll offset. The list scrolls internally (the
   * card is a fixed height), and the relation layer needs the offset to know
   * where each column actually is on screen.
   */
  onListScroll?: (table: string, scrollTop: number) => void;
  /** Begin a manual join by dragging from a column's connector handle. */
  onStartManualJoin?: (
    table: string,
    column: string,
    origin: { clientX: number; clientY: number; pointerId: number },
  ) => void;
}

/** Elements that must keep their own pointer behaviour inside a draggable card. */
function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return !!target.closest('input, button, select, textarea, a, label');
}

/**
 * One table on the canvas.
 *
 * Sizes come from `cardLayout` constants rather than Tailwind spacing so the SVG
 * relation layer can compute exact column anchors: the card height is
 * `cardHeight(columns.length)`, fixed once the list reaches its cap. Long tables
 * scroll **inside** the card (as in Navicat); the relation layer reports a
 * scrolled-out column by clamping its anchor to the list edge.
 */
export function TableCard({
  tableName,
  alias,
  columns,
  selectedColumns,
  primaryKeyColumns = [],
  foreignKeyMap = {},
  position,
  otherPositions = {},
  onToggleColumn,
  onToggleAllColumns,
  onRemove,
  onDragEnd,
  onSetAlias,
  onStartManualJoin,
  onListScroll,
}: TableCardProps) {
  const { t } = useI18n();
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    startPosX: number;
    startPosY: number;
  } | null>(null);

  const selectedSet = new Set(selectedColumns);
  const allSelected = columns.length > 0 && columns.every((c) => selectedSet.has(c.name));
  const someSelected = columns.some((c) => selectedSet.has(c.name));

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0 || isInteractiveTarget(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      const card = e.currentTarget as HTMLElement;
      card.setPointerCapture?.(e.pointerId);
      dragRef.current = {
        pointerId: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        startPosX: position.x,
        startPosY: position.y,
      };
    },
    [position.x, position.y],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      const drag = dragRef.current;
      if (!drag || drag.pointerId !== e.pointerId) return;
      const next = resolveDragPosition(
        {
          x: drag.startPosX + (e.clientX - drag.startX),
          y: drag.startPosY + (e.clientY - drag.startY),
        },
        otherPositions,
        tableName,
      );
      if (next.x !== position.x || next.y !== position.y) {
        onDragEnd(next);
      }
    },
    [onDragEnd, otherPositions, position.x, position.y, tableName],
  );

  const endDrag = useCallback((e: React.PointerEvent) => {
    const card = e.currentTarget as HTMLElement;
    const drag = dragRef.current;
    if (drag && card.hasPointerCapture?.(drag.pointerId)) {
      card.releasePointerCapture(drag.pointerId);
    }
    dragRef.current = null;
  }, []);

  return (
    <div
      className={cn(
        'qb-card absolute select-none rounded-lg border border-edge bg-surface-raised shadow-lg',
        'text-fg cursor-grab active:cursor-grabbing',
      )}
      style={{
        transform: `translate(${position.x}px, ${position.y}px)`,
        width: CARD_WIDTH,
        willChange: 'transform',
        touchAction: 'none',
      }}
      data-table={tableName}
      data-testid={`qb-drag-${tableName}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      {/* Header — accent-tinted (same `bg-accent/15` + `text-accent` recipe as
          the ER diagram's table node header) so the table name reads as the
          card's title. Horizontal rhythm matches the rows below, so the
          select-all checkbox lines up with the per-column checkboxes. */}
      <div
        className="flex items-center gap-2 border-b border-edge bg-accent/15 px-3"
        style={{ height: CARD_HEADER_HEIGHT }}
      >
        <input
          type="checkbox"
          checked={allSelected}
          ref={(el) => {
            if (el) el.indeterminate = !allSelected && someSelected;
          }}
          onChange={(e) => onToggleAllColumns(e.target.checked)}
          onClick={(e) => e.stopPropagation()}
          className="accent-accent h-3.5 w-3.5 shrink-0"
          title={t('query.visualBuilder.selectAllColumns')}
          aria-label={t('query.visualBuilder.selectAllColumns')}
          data-testid={`qb-selectall-${tableName}`}
        />
        <span className="truncate text-[13px] font-semibold text-accent" title={tableName}>
          {tableName}
        </span>
        <input
          type="text"
          value={alias ?? ''}
          onChange={(e) => onSetAlias(e.target.value)}
          placeholder={t('query.visualBuilder.alias')}
          className={cn(
            'ml-auto h-5 w-14 shrink-0 rounded border border-edge bg-surface-inset px-1.5 text-[11px]',
            'text-fg placeholder:text-fg-muted outline-none',
            'focus:border-accent focus:ring-1 focus:ring-accent-ring',
          )}
          title={t('query.visualBuilder.alias')}
          data-testid={`qb-alias-${tableName}`}
        />
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="shrink-0 rounded p-0.5 text-fg-muted transition-colors hover:bg-surface-inset hover:text-danger"
          title={t('query.visualBuilder.removeTable')}
          aria-label={t('query.visualBuilder.removeTable')}
          data-testid={`qb-remove-${tableName}`}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* Column list — a fixed-height window that scrolls internally, so a card
          stays a stable object on the canvas no matter how wide its table is.
          The relation layer clamps any anchor whose column is scrolled out. */}
      <div
        className="qb-col-list"
        style={{
          maxHeight: CARD_LIST_MAX_HEIGHT,
          overflowY: cardListScrolls(columns.length) ? 'auto' : 'hidden',
          paddingTop: CARD_LIST_PADDING_Y,
          paddingBottom: CARD_LIST_PADDING_Y,
        }}
        onScroll={(e) => onListScroll?.(tableName, e.currentTarget.scrollTop)}
        data-testid={`qb-col-list-${tableName}`}
      >
        {columns.map((col) => {
          const isPk = primaryKeyColumns.includes(col.name);
          const fkTarget = foreignKeyMap[col.name];
          return (
            <div
              key={col.name}
              className="qb-col-row group relative flex cursor-pointer items-center gap-2 px-3 text-[12px] hover:bg-surface-inset"
              style={{ height: CARD_ROW_HEIGHT }}
              data-testid={`qb-col-${tableName}-${col.name}`}
              data-qb-col-anchor={`${tableName}.${col.name}`}
            >
              <input
                type="checkbox"
                checked={selectedSet.has(col.name)}
                onChange={() => onToggleColumn(col.name)}
                className="accent-accent h-3.5 w-3.5 shrink-0"
              />
              <span className="truncate">{col.name}</span>
              <span className="ml-auto shrink-0 text-[10px] text-fg-muted">{col.dataType}</span>
              {isPk && (
                <span className="inline-flex shrink-0 items-center rounded bg-amber-500/20 px-1 py-0 text-[9px] font-semibold text-amber-400">
                  PK
                </span>
              )}
              {fkTarget && (
                <span className="inline-flex shrink-0 items-center rounded bg-blue-500/20 px-1 py-0 text-[9px] font-semibold text-blue-400">
                  FK
                </span>
              )}

              {/* Connector handle: drag from here to another column to create a
                  manual join. Kept invisible until the row is hovered so the
                  canvas stays free of chrome. */}
              {onStartManualJoin && (
                <button
                  type="button"
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    e.preventDefault();
                    onStartManualJoin(tableName, col.name, {
                      clientX: e.clientX,
                      clientY: e.clientY,
                      pointerId: e.pointerId,
                    });
                  }}
                  className={cn(
                    'absolute right-1 top-1/2 h-3 w-3 -translate-y-1/2 rounded-full',
                    'cursor-crosshair border border-accent/60 bg-surface opacity-0 transition-opacity',
                    'group-hover:opacity-100 hover:bg-accent',
                  )}
                  title={t('query.visualBuilder.connectColumn')}
                  aria-label={t('query.visualBuilder.connectColumn')}
                  data-testid={`qb-connect-${tableName}-${col.name}`}
                />
              )}
            </div>
          );
        })}
        {columns.length === 0 && (
          <div className="py-2 text-center text-[11px] text-fg-muted">
            {t('query.visualBuilder.noColumns')}
          </div>
        )}
      </div>
    </div>
  );
}
