import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { cn } from '@datazen/ui';
import type { ColumnInfo } from '../../../types';
import type { QbJoinType, QbColumnSelection } from '../types';
import { useI18n } from '../../../hooks/useI18n';
import { useCanvasInteraction } from './useCanvasInteraction';
import { TableCard } from './TableCard';
import { RelationLine } from './RelationLine';
import { JoinPopover } from './JoinPopover';
import { alignDroppedCard, CARD_WIDTH, canvasContentSize, rowCenterY } from './cardLayout';
import { buildRelationShapes, type RelationGroup } from './fkGeometry';

/** Props for the DiagramCanvas component. */
export interface DiagramCanvasProps {
  selectedTables: string[];
  tablePositions: Record<string, { x: number; y: number }>;
  /** Every relation to draw: one entry per constraint (or per manual join). */
  relationGroups?: RelationGroup[];
  columnMap: Record<string, string[]>;
  columnInfoMap: Record<string, ColumnInfo[]>;
  selectedColumns: QbColumnSelection[];
  tableAliases: Record<string, string>;
  /** Map of table → column names that are primary keys. */
  primaryKeyMap?: Record<string, string[]>;
  onToggleColumn: (table: string, column: string) => void;
  /** Check/uncheck every column of a card. */
  onToggleAllColumns?: (table: string, columns: string[], selected: boolean) => void;
  /** Remove a table (and its references) from the query. */
  onRemoveTable?: (table: string) => void;
  onUpdatePosition: (table: string, pos: { x: number; y: number }) => void;
  /** Re-type every confirmed pair of a relation group. */
  onSetGroupType?: (groupId: string, type: QbJoinType) => void;
  /** Promote every pair of a relation group into the SQL. */
  onConfirmGroup?: (groupId: string) => void;
  /** Remove every pair of a relation group from the SQL. */
  onRemoveGroup?: (groupId: string) => void;
  /** Create a manual join between two columns. */
  onAddManualJoin?: (
    from: { table: string; column: string },
    to: { table: string; column: string },
  ) => void;
  onSetTableAlias: (table: string, alias: string) => void;
  /** Called when a table is dropped onto the canvas from the object tree. */
  onDropTable?: (tableName: string, pos: { x: number; y: number }) => void;
  /** Current zoom level (from store). */
  zoom?: number;
  /** Callback when zoom changes. */
  onZoomChange?: (zoom: number) => void;
}

/**
 * DiagramCanvas — the visual builder's canvas.
 *
 * Viewport model (R3): the canvas **scrolls natively** (wheel/trackpad, drag,
 * or the scrollbars) and zooms with Ctrl/Cmd + wheel around the pointer. The
 * content is laid out unscaled inside a spacer sized `content × zoom`, so the
 * scrollbars match the zoomed extent. Cards therefore grow to their full height
 * instead of scrolling internally — an internally clipped row would put its
 * column anchor outside the card, which is exactly what the lines must avoid.
 *
 * Hybrid SVG + DOM: relation lines are SVG inside the same scaled wrapper as the
 * cards, so canvas-space coordinates stay valid for both. Relation lines carry
 * **no text**; everything actionable lives in a popover opened at the click.
 */
export function DiagramCanvas({
  selectedTables,
  tablePositions,
  relationGroups = [],
  columnMap,
  columnInfoMap,
  selectedColumns,
  tableAliases,
  primaryKeyMap = {},
  onToggleColumn,
  onToggleAllColumns,
  onRemoveTable,
  onUpdatePosition,
  onSetGroupType,
  onConfirmGroup,
  onRemoveGroup,
  onAddManualJoin,
  onSetTableAlias,
  onDropTable,
  zoom: storeZoom = 1,
  onZoomChange = () => {},
}: DiagramCanvasProps) {
  const { t } = useI18n();

  /** Column names per table in row order — the geometry's index source. */
  const columnOrder = useMemo(() => {
    const order: Record<string, string[]> = {};
    for (const table of selectedTables) {
      const info = columnInfoMap[table] ?? [];
      order[table] = info.length > 0 ? info.map((column) => column.name) : (columnMap[table] ?? []);
    }
    return order;
  }, [selectedTables, columnInfoMap, columnMap]);

  const {
    scrollRef,
    handleWheel,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    screenToCanvas,
    isPanning,
    viewport,
  } = useCanvasInteraction({ zoom: storeZoom, onZoomChange });

  const zoom = storeZoom;

  /** The scroll element, needed for anchoring the popover in viewport space. */
  const scrollElRef = useRef<HTMLDivElement | null>(null);
  const attachScrollRef = useCallback(
    (el: HTMLDivElement | null) => {
      scrollElRef.current = el;
      scrollRef(el);
    },
    [scrollRef],
  );

  /** Scrollable extent: the cards' bounding box, never smaller than the viewport. */
  const contentSize = useMemo(() => {
    const cards = selectedTables.map((table) => ({
      pos: tablePositions[table] ?? { x: 0, y: 0 },
      columnCount: (columnOrder[table] ?? []).length,
    }));
    return canvasContentSize(cards, viewport);
  }, [selectedTables, tablePositions, columnOrder, viewport]);

  /**
   * Per-card column-list scroll offsets, so relation anchors follow the rows.
   */
  const [scrollTops, setScrollTops] = useState<Record<string, number>>({});
  const handleListScroll = useCallback((table: string, scrollTop: number) => {
    setScrollTops((prev) => (prev[table] === scrollTop ? prev : { ...prev, [table]: scrollTop }));
  }, []);

  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [popover, setPopover] = useState<{ groupId: string; at: { x: number; y: number } } | null>(
    null,
  );

  const shapes = useMemo(
    () =>
      buildRelationShapes({
        groups: relationGroups,
        positions: tablePositions,
        columnOrder,
        scrollTops,
      }),
    [relationGroups, tablePositions, columnOrder, scrollTops],
  );

  const shapeById = useMemo(() => new Map(shapes.map((shape) => [shape.groupId, shape])), [shapes]);

  /**
   * Highlight the column rows a hovered/selected relation touches, so "which two
   * columns does this line join?" is answerable without any label.
   */
  const relatedColumns = useMemo(() => {
    const groupId = popover?.groupId ?? activeGroupId;
    if (!groupId) return new Set<string>();
    return new Set(shapeById.get(groupId)?.columnKeys ?? []);
  }, [activeGroupId, popover?.groupId, shapeById]);

  useEffect(() => {
    document.querySelectorAll<HTMLElement>('[data-qb-col-anchor]').forEach((node) => {
      const key = node.getAttribute('data-qb-col-anchor');
      node.classList.toggle('is-related', !!key && relatedColumns.has(key));
    });
  }, [relatedColumns]);

  /**
   * Reveal the columns of the hovered/selected relation. A card's list scrolls
   * internally, so a column the line points at can be scrolled out; bringing it
   * into view turns the clamped terminal back into the real row.
   */
  useEffect(() => {
    const groupId = popover?.groupId ?? activeGroupId;
    if (!groupId) return;
    const shape = shapeById.get(groupId);
    if (!shape) return;
    for (const key of shape.columnKeys) {
      document
        .querySelector<HTMLElement>(`[data-qb-col-anchor="${key}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    }
  }, [activeGroupId, popover?.groupId, shapeById]);

  // ── Manual join drag ────────────────────────────────────────
  const [manualJoin, setManualJoin] = useState<{
    from: { table: string; column: string };
    /** Latest pointer position, in client coordinates. */
    client: { x: number; y: number };
    target: { table: string; column: string } | null;
  } | null>(null);
  const manualJoinRef = useRef(manualJoin);
  manualJoinRef.current = manualJoin;

  const startManualJoin = useCallback(
    (table: string, column: string, origin: { clientX: number; clientY: number }) => {
      setManualJoin({
        from: { table, column },
        client: { x: origin.clientX, y: origin.clientY },
        target: null,
      });
    },
    [],
  );

  /**
   * Listeners are registered once per drag, keyed on a boolean: depending on the
   * `manualJoin` object would re-run the effect on every pointermove, and each
   * re-run's cleanup strips the drop-target highlight that the move just added
   * (so the highlight would never be visible).
   */
  const isDragging = manualJoin !== null;

  useEffect(() => {
    if (!isDragging) return;

    const clearTargets = () => {
      document
        .querySelectorAll('.qb-drop-target')
        .forEach((node) => node.classList.remove('qb-drop-target'));
    };

    const onPointerMove = (e: PointerEvent) => {
      const element = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      const row = element?.closest<HTMLElement>('[data-qb-col-anchor]') ?? null;
      const key = row?.getAttribute('data-qb-col-anchor') ?? '';
      const dot = key.indexOf('.');
      const target = dot > 0 ? { table: key.slice(0, dot), column: key.slice(dot + 1) } : null;

      clearTargets();
      const from = manualJoinRef.current?.from;
      // A self join cannot be expressed, so the same table is never a target.
      if (row && target && from && target.table !== from.table) {
        row.classList.add('qb-drop-target');
      }
      setManualJoin((prev) =>
        prev ? { ...prev, client: { x: e.clientX, y: e.clientY }, target } : prev,
      );
    };

    const onPointerUp = () => {
      const drag = manualJoinRef.current;
      clearTargets();
      if (drag?.target && onAddManualJoin && drag.target.table !== drag.from.table) {
        onAddManualJoin(drag.from, drag.target);
      }
      setManualJoin(null);
    };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerUp);
    return () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerUp);
      clearTargets();
    };
  }, [isDragging, onAddManualJoin]);

  /** Canvas-space endpoints of the manual-join preview line. */
  const manualPreview = useMemo(() => {
    if (!manualJoin) return null;
    const cardPos = tablePositions[manualJoin.from.table];
    if (!cardPos) return null;
    const index = (columnOrder[manualJoin.from.table] ?? []).indexOf(manualJoin.from.column);
    if (index === -1) return null;
    return {
      start: { x: cardPos.x + CARD_WIDTH, y: rowCenterY(cardPos.y, index) },
      end: screenToCanvas(manualJoin.client.x, manualJoin.client.y),
    };
  }, [manualJoin, tablePositions, columnOrder, screenToCanvas]);

  // ── Drop from the schema tree ───────────────────────────────
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      if (!onDropTable) return;

      const raw = e.dataTransfer.getData('application/datazen-schema-object');
      if (!raw) return;
      try {
        const payload = JSON.parse(raw) as { namespace?: { table?: string } };
        const tableName = payload.namespace?.table;
        if (!tableName) return;
        const dropped = screenToCanvas(e.clientX, e.clientY);
        // Snap to the grid and share the top edge of the row being dropped
        // into, so dragging several tables in does not leave their tops askew.
        onDropTable(tableName, alignDroppedCard(dropped, tablePositions));
      } catch {
        // invalid payload — ignore
      }
    },
    [onDropTable, screenToCanvas, tablePositions],
  );

  const popoverShape = popover ? shapeById.get(popover.groupId) : undefined;

  return (
    <div className="relative h-full w-full overflow-hidden">
      <div
        ref={attachScrollRef}
        className={cn(
          'qb-canvas-scroll h-full w-full overflow-auto bg-surface',
          isPanning ? 'cursor-grabbing' : 'cursor-default',
        )}
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        data-testid="qb-diagram-canvas"
      >
        {/*
         * The spacer carries the *zoomed* extent so the native scrollbars are
         * right; the scaled wrapper inside holds the unscaled canvas content.
         */}
        <div
          className="relative"
          style={{ width: contentSize.width * zoom, height: contentSize.height * zoom }}
        >
          <div
            className="absolute top-0 left-0"
            style={{
              width: contentSize.width,
              height: contentSize.height,
              transform: `scale(${zoom})`,
              transformOrigin: '0 0',
            }}
          >
            {/* Grid dot pattern background */}
            <svg className="pointer-events-none absolute inset-0 h-full w-full" aria-hidden="true">
              <defs>
                <pattern id="qb-grid-dots" width="20" height="20" patternUnits="userSpaceOnUse">
                  <circle cx="10" cy="10" r="0.8" fill="var(--c-fg-muted)" opacity="0.25" />
                </pattern>
              </defs>
              <rect width="100%" height="100%" fill="url(#qb-grid-dots)" />
            </svg>

            {/* Relation lines share the cards' coordinate space */}
            <svg
              className="pointer-events-none absolute inset-0"
              style={{ width: contentSize.width, height: contentSize.height, overflow: 'visible' }}
              aria-hidden="true"
            >
              {shapes.map((shape) => (
                <RelationLine
                  key={shape.groupId}
                  shape={shape}
                  active={activeGroupId === shape.groupId || popover?.groupId === shape.groupId}
                  dimmed={manualJoin !== null}
                  onHoverChange={setActiveGroupId}
                  onActivate={(groupId, origin) => {
                    const rect = scrollElRef.current?.getBoundingClientRect();
                    setPopover({
                      groupId,
                      at: {
                        x: origin.clientX - (rect?.left ?? 0),
                        y: origin.clientY - (rect?.top ?? 0),
                      },
                    });
                  }}
                />
              ))}

              {manualPreview && (
                <path
                  className="qb-relation-line qb-relation-line--confirmed"
                  d={`M ${manualPreview.start.x} ${manualPreview.start.y} L ${manualPreview.end.x} ${manualPreview.end.y}`}
                  data-testid="qb-manual-join-preview"
                />
              )}
            </svg>

            {/* DOM layer for table cards */}
            {selectedTables.map((table) => {
              const pos = tablePositions[table] ?? { x: 0, y: 0 };
              const columns = columnInfoMap[table] ?? [];
              const columnNames = columnMap[table] ?? [];
              const effectiveColumns =
                columns.length > 0
                  ? columns
                  : columnNames.map((name) => ({ name, dataType: '', nullable: true }));

              const tableSelectedCols = selectedColumns
                .filter((sc) => sc.table === table)
                .map((sc) => sc.column);

              const foreignKeyMap: Record<string, string> = {};
              for (const group of relationGroups) {
                for (const pair of group.pairs) {
                  if (pair.fromTable === table) foreignKeyMap[pair.fromColumn] = pair.toTable;
                }
              }

              return (
                <TableCard
                  key={table}
                  tableName={table}
                  alias={tableAliases[table]}
                  columns={effectiveColumns}
                  selectedColumns={tableSelectedCols}
                  primaryKeyColumns={primaryKeyMap[table]}
                  foreignKeyMap={foreignKeyMap}
                  position={pos}
                  otherPositions={tablePositions}
                  onToggleColumn={(col) => onToggleColumn(table, col)}
                  onToggleAllColumns={(selected) =>
                    onToggleAllColumns?.(table, columnNames, selected)
                  }
                  onRemove={() => onRemoveTable?.(table)}
                  onDragEnd={(newPos) => onUpdatePosition(table, newPos)}
                  onSetAlias={(alias) => onSetTableAlias(table, alias)}
                  onStartManualJoin={startManualJoin}
                  onListScroll={handleListScroll}
                />
              );
            })}

            {/* Empty state hint */}
            {selectedTables.length === 0 && (
              <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
                <div className="text-center text-sm text-fg-muted">
                  {t('query.canvas.dragHint')}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Actions popover — anchored to the viewport, so it never scrolls away. */}
      {popover && popoverShape && (
        <JoinPopover
          shape={popoverShape}
          at={popover.at}
          onSetType={(type) => onSetGroupType?.(popover.groupId, type)}
          onConfirm={() => {
            onConfirmGroup?.(popover.groupId);
            setPopover(null);
          }}
          onRemove={() => {
            onRemoveGroup?.(popover.groupId);
            setPopover(null);
          }}
          onClose={() => setPopover(null)}
        />
      )}

      {/* Zoom indicator */}
      <div className="pointer-events-none absolute right-2 bottom-2 rounded bg-surface-alt/80 px-1.5 py-0.5 text-[10px] text-muted select-none">
        {Math.round(zoom * 100)}%
      </div>
    </div>
  );
}
