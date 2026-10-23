import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTableForeignKeys } from '../../hooks/useTableForeignKeys';
import { useFkNavigationShortcuts } from '../../hooks/useFkNavigationShortcuts';
import { useFkNavigationStore } from '../../stores/fkNavigationStore';
import { usePanelStore } from '../../stores/panelStore';
import { openForeignKeyTarget } from '../../lib/fkNavigationActions';
import {
  pickAnchorRow,
  scrollTopForAnchorRow,
  type FkGroupAffordance,
  type GridViewport,
} from '../../lib/fkNavigation';
import type { GridErrorCode } from '../../lib/gridErrors';
import { toCellValue } from '../../stores/tableData/connectionState';
import type { CellCoord } from '../../stores/tableData/gridSelection';
import type { FkOrigin } from '../../stores/panelTypes';
import type { ForeignKeyInfo, Value } from '../../types';

/**
 * The foreign-key navigation of one table tab (DB-07).
 *
 * The hook owns everything the tab needs to follow a foreign key and to come
 * back: the metadata, the jump, the error, the breadcrumb, the keyboard and
 * the viewport.
 *
 * It is a hook of its own because `TableView` is the place every chapter adds
 * lines to, and the navigation is a chapter of its own: the file that holds
 * the wiring has to stay readable.
 */
export interface UseTableForeignKeyNavigationOptions {
  /** The tab this grid belongs to. */
  panelId: string;
  /** The runtime session of the tab. */
  dbSessionId: string;
  /** The database of the tab. */
  database: string;
  /** The table of the tab. */
  tableName: string;
  /** The namespace of the tab; `null` when the driver has none. */
  schema: string | null;
  /** The connection of the tab. */
  connectionId?: string | null;
  /** The driver of the tab. */
  databaseType?: string | null;
  /** The rows of the current page, by column name. */
  rows: Record<string, unknown>[];
  /** The displayed columns, in render order. */
  displayedColumns: readonly { name: string }[];
  /** The row height of the grid — the anchor is a row index. */
  rowHeight: number;
  /** The focused cell of the grid; it is the anchor of the viewport. */
  activeCell?: CellCoord | null;
}

/** What the tab needs to render and to react to. */
export interface TableForeignKeyNavigation {
  /**
   * The foreign-key affordances of the grid, as **one** object.
   *
   * The pieces belong together: the arrow of a composite key is decided by
   * the values of **all** of its members — including a member the user has
   * hidden — and only the full row records carry those values. A grid that
   * got the index but not the records would silently lose the arrow of every
   * composite key with a hidden member (FKU-D1), so the type makes the
   * combination impossible to write.
   */
  foreignKeys: {
    /** The columns that take part in a foreign key, mapped to their group. */
    index: ReadonlyMap<string, FkGroupAffordance>;
    /** The **full** row records (every column of the table), by row index. */
    rows: readonly Record<string, unknown>[];
    /** The displayed column names, in render order — the order the grid renders. */
    visibleColumns: readonly string[];
    /** The cells whose resolution is in flight, keyed `${rowIndex}:${columnName}`. */
    resolving: ReadonlySet<string>;
    /** Follow the foreign key of a cell. */
    onJump: (rowIndex: number, columnName: string, element?: HTMLElement | null) => void;
  };
  /** Why the last jump was refused; `null` when there was no refusal. */
  error: { code: GridErrorCode | 'unknown'; raw: string } | null;
  /** Hide the error of a refused jump. */
  dismissError: () => void;
  /** The breadcrumb of this tab, or `null` when the tab was not reached by a jump. */
  breadcrumb: {
    origin: FkOrigin;
    truncated: boolean;
    canGoBack: boolean;
    canGoForward: boolean;
  } | null;
  /** Go back one jump. */
  onBack: () => void;
  /** Go forward one jump. */
  onForward: () => void;
  /** The keyboard of the navigation; bind it to the grid container. */
  onKeyDown: (event: React.KeyboardEvent) => void;
  /** The grid's scroll container, handed over by the grid. */
  onScrollElementChange: (element: HTMLDivElement | null) => void;
}

const EMPTY_RESOLVING: ReadonlySet<string> = new Set();

/// ~1s of frames: the restoring tab has to fetch its first page over IPC, and
/// until that page lands the virtual body has nothing to scroll.
const MAX_RESTORE_FRAMES = 60;

export function useTableForeignKeyNavigation(
  options: UseTableForeignKeyNavigationOptions,
): TableForeignKeyNavigation {
  const {
    panelId,
    dbSessionId,
    database,
    tableName,
    schema,
    connectionId,
    databaseType,
    rows,
    displayedColumns,
    rowHeight,
    activeCell,
  } = options;

  /** The displayed column names, in render order — the order the grid renders. */
  const visibleColumns = useMemo(
    () => displayedColumns.map((column) => column.name),
    [displayedColumns],
  );

  const metadata = useTableForeignKeys({
    dbSessionId,
    tableName,
    database,
    schema,
    connectionId,
  });
  const fkIndex = metadata.index;

  const [error, setError] = useState<{ code: GridErrorCode | 'unknown'; raw: string } | null>(null);
  const [resolving, setResolving] = useState<ReadonlySet<string>>(EMPTY_RESOLVING);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  const scrollElementRef = useRef<HTMLDivElement | null>(null);

  const onScrollElementChange = useCallback((element: HTMLDivElement | null) => {
    scrollElementRef.current = element;
    setScrollElement(element);
  }, []);

  /**
   * The viewport of the tab, as it is right now.
   *
   * The anchor is the focused cell when there is one, and the topmost visible
   * row otherwise — a row that is certainly on the page.
   */
  const snapshotViewport = useCallback(
    (source?: HTMLElement | null): GridViewport | null => {
      // The grid the arrow lives in is authoritative. A cached element can belong
      // to a surface that has since been replaced (the tab re-mounted while its
      // panel kept its identity), and reading that one would record 0 — the
      // restore would then send the user back to the top of the table.
      const element =
        source instanceof HTMLElement && source.isConnected
          ? source
          : scrollElementRef.current instanceof HTMLElement && scrollElementRef.current.isConnected
            ? scrollElementRef.current
            : null;
      if (!element) return null;
      const scrollTop = element.scrollTop;
      // The active cell wins only while it is on screen; a cell the user scrolled
      // past must not drag the tab back to the top on return.
      const { anchorRow, anchorVisible } = pickAnchorRow({
        scrollTop,
        rowHeight,
        viewportHeight: element.clientHeight,
        activeRow: activeCell ? activeCell.rowIndex : null,
      });
      const anchorColumn = anchorVisible && activeCell ? activeCell.columnName : null;
      return { scrollTop, anchorRow, anchorColumn };
    },
    [activeCell, rowHeight],
  );

  const handleFkJump = useCallback(
    (rowIndex: number, columnName: string, source?: HTMLElement | null) => {
      const group = fkIndex.get(columnName);
      if (!group) return;
      const row = rows[rowIndex];
      if (!row) return;
      // The values of the **whole** group, in pairing order: a composite key
      // is located by all of its columns or by none of them.
      const values: Value[] = group.localColumns.map((column) => toCellValue(row[column]));
      if (values.some((value) => value === null)) return;
      const foreignKey: ForeignKeyInfo = {
        name: group.constraintName,
        columns: [...group.localColumns],
        referencedTable: group.referenceTable,
        referencedColumns: [...group.targetColumns],
      };
      const cellKey = `${rowIndex}:${columnName}`;
      setResolving((current) => new Set(current).add(cellKey));
      void openForeignKeyTarget({
        sourcePanelId: panelId,
        sourceTable: tableName,
        dbSessionId,
        database,
        schema,
        connectionId: connectionId ?? null,
        driverType: databaseType ?? null,
        foreignKey,
        values,
        sourceViewport: snapshotViewport(source),
      })
        .then((outcome) => {
          if (!outcome.ok) {
            setError(outcome.error ?? { code: 'unknown', raw: '' });
            return;
          }
          setError(null);
        })
        .finally(() => {
          setResolving((current) => {
            const next = new Set(current);
            next.delete(cellKey);
            return next;
          });
        });
    },
    [
      fkIndex,
      rows,
      panelId,
      tableName,
      dbSessionId,
      database,
      schema,
      connectionId,
      databaseType,
      snapshotViewport,
    ],
  );

  /**
   * The stack of this tab: the breadcrumb of the tab, and the two moves.
   *
   * The panel store is the source of truth for "which tabs exist" — the stack
   * is pruned against it, so a jump never activates a tab the user closed.
   *
   * Every field is selected on its own: a selector that built a new object
   * would hand the store a fresh snapshot on every render.
   */
  const back = useFkNavigationStore((state) => state.back);
  const forward = useFkNavigationStore((state) => state.forward);
  const truncated = useFkNavigationStore((state) => state.truncated);
  const panel = usePanelStore((state) => state.panels.find((item) => item.id === panelId));

  const origin = panel && panel.type === 'table' ? panel.fkOrigin : undefined;

  const goBack = useCallback(() => {
    const store = useFkNavigationStore.getState();
    const panels = usePanelStore.getState().panels;
    const entry = store.goBack(new Set(panels.map((item) => item.id)));
    if (!entry) return;
    usePanelStore.getState().setActivePanel(entry.origin.sourcePanelId);
  }, []);

  const goForward = useCallback(() => {
    const store = useFkNavigationStore.getState();
    const panels = usePanelStore.getState().panels;
    const entry = store.goForward(new Set(panels.map((item) => item.id)));
    if (!entry) return;
    usePanelStore.getState().setActivePanel(entry.panelId);
  }, []);

  const onKeyDown = useFkNavigationShortcuts({
    enabled: back.length > 0,
    onBack: goBack,
    onForward: goForward,
  });

  /**
   * The viewport of the tab is restored when the tab is entered again.
   *
   * The order matters: the scroll position goes first, and the anchor is
   * located in the next frame — the virtual window re-lays out after the
   * scroll, so focusing first would put the focus on a row that is about to
   * move.
   */
  const [pendingViewport, setPendingViewport] = useState<GridViewport | null>(null);
  useEffect(() => {
    const viewport = useFkNavigationStore.getState().takeViewport(panelId);
    setPendingViewport(viewport);
  }, [panelId]);
  useEffect(() => {
    // Restoring before the rows exist cannot stick: the surface is clamped to 0
    // and the frame budget runs out while the data load is still in flight, so
    // the tab comes back at the top. Wait for the rows, then place the offset.
    if (!pendingViewport || !scrollElement || rows.length === 0) return;
    const target =
      pendingViewport.anchorRow !== null
        ? scrollTopForAnchorRow(pendingViewport.anchorRow, rowHeight)
        : pendingViewport.scrollTop;
    // The body is virtualised: the moment the tab is entered the surface exists
    // but its rows do not yet, so the assignment is clamped to 0 and the
    // position is silently lost. Retry over a few frames and only drop the
    // snapshot once the offset actually stuck.
    let frames = 0;
    let frame = 0;
    const apply = () => {
      scrollElement.scrollTop = target;
      if (Math.abs(scrollElement.scrollTop - target) <= 1 || frames >= MAX_RESTORE_FRAMES) {
        setPendingViewport(null);
        return;
      }
      frames += 1;
      frame = requestAnimationFrame(apply);
    };
    frame = requestAnimationFrame(apply);
    const { anchorRow, anchorColumn } = pendingViewport;
    if (anchorRow !== null && anchorColumn) {
      requestAnimationFrame(() => {
        const cell = document.querySelector(
          `[data-dt-row="${anchorRow}"][data-dt-col="${anchorColumn}"]`,
        );
        if (cell instanceof HTMLElement) cell.focus();
      });
    }
    return () => cancelAnimationFrame(frame);
  }, [pendingViewport, scrollElement, rowHeight, rows.length]);

  const breadcrumb = useMemo(() => {
    if (!origin) return null;
    return {
      origin,
      truncated,
      canGoBack: back.length > 0,
      canGoForward: forward.length > 0,
    };
  }, [origin, truncated, back.length, forward.length]);

  const dismissError = useCallback(() => setError(null), []);

  /**
   * The affordances of the grid, memoised as one object.
   *
   * The grid is `memo`-ised on its props, so a fresh object on every render
   * would re-render every visible cell of the page.
   */
  const foreignKeys = useMemo(
    () => ({
      index: fkIndex,
      rows,
      visibleColumns,
      resolving,
      onJump: handleFkJump,
    }),
    [fkIndex, rows, visibleColumns, resolving, handleFkJump],
  );

  return {
    foreignKeys,
    error,
    dismissError,
    breadcrumb,
    onBack: goBack,
    onForward: goForward,
    onKeyDown,
    onScrollElementChange,
  };
}
