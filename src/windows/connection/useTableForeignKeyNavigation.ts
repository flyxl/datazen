import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTableForeignKeys } from '../../hooks/useTableForeignKeys';
import { useFkNavigationShortcuts } from '../../hooks/useFkNavigationShortcuts';
import { useFkNavigationStore } from '../../stores/fkNavigationStore';
import { usePanelStore } from '../../stores/panelStore';
import { openForeignKeyTarget } from '../../lib/fkNavigationActions';
import { openReferencingRows } from '../../lib/fkReferencedBy';
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
  /**
   * Open the tabs of the rows that reference one row (DB-20). The refusals
   * land on the same error channel as a refused jump.
   */
  onShowReferencingRows: (rowIndex: number) => void;
  /** The keyboard of the navigation; bind it to the grid container. */
  onKeyDown: (event: React.KeyboardEvent) => void;
  /** The grid's scroll container, handed over by the grid. */
  onScrollElementChange: (element: HTMLDivElement | null) => void;
}

const EMPTY_RESOLVING: ReadonlySet<string> = new Set();

/// How often a placement that could not land yet is retried...
const RESTORE_RETRY_MS = 50;
/// ...and for how long. The restoring tab may have to fetch its first page over
/// IPC before its body has anything to scroll, so a frame budget (about a
/// second) is too short: five seconds covers the IPC round trip and the render.
const MAX_RESTORE_ATTEMPTS = 100;

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
  /**
   * The scroll offset the surface had when the pointer went down.
   *
   * Focusing a button inside the transform-based virtual window makes the engine
   * scroll the grid surface to its top *while dispatching that very press*, so by
   * the time the click handler runs the live offset is 0 and the position the tab
   * would have returned to is gone. The capture-phase listener runs before that
   * default action, which makes it the only honest record of where the user was.
   */
  const pointerOffsetRef = useRef<number | null>(null);
  useEffect(() => {
    const element = scrollElement;
    if (!element) return;
    const capture = () => {
      pointerOffsetRef.current = element.scrollTop;
    };
    // Pointer events are the ones a webview actually delivers for a click;
    // `mousedown`/`touchstart` stay for the engines that dispatch only those.
    element.addEventListener('pointerdown', capture, true);
    element.addEventListener('mousedown', capture, true);
    element.addEventListener('touchstart', capture, true);
    return () => {
      element.removeEventListener('pointerdown', capture, true);
      element.removeEventListener('mousedown', capture, true);
      element.removeEventListener('touchstart', capture, true);
    };
  }, [scrollElement]);

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
      // A press may have moved the surface (see `pointerOffsetRef`); the offset
      // it had when the press started is the one the user saw.
      const live = element.scrollTop;
      const pressed = pointerOffsetRef.current;
      const scrollTop = pressed !== null && pressed > live ? pressed : live;
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
          pointerOffsetRef.current = null;
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

  const handleShowReferencingRows = useCallback(
    (rowIndex: number) => {
      const row = rows[rowIndex];
      if (!row) return;
      void openReferencingRows({
        sourcePanelId: panelId,
        sourceTable: tableName,
        dbSessionId,
        database,
        schema,
        connectionId: connectionId ?? null,
        driverType: databaseType ?? null,
        row,
        sourceViewport: snapshotViewport(),
      })
        .then((outcome) => {
          if (!outcome.ok) {
            setError(outcome.error ?? { code: 'unknown', raw: '' });
            return;
          }
          setError(null);
        })
        .catch(() => undefined);
    },
    [
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
  const isActive = usePanelStore((state) => state.activePanelId === panelId);
  const [pendingViewport, setPendingViewport] = useState<GridViewport | null>(null);
  // The snapshot is *peeked*, not taken: re-entering the tab remounts this
  // panel, and the mount that first reads it may be the one that never gets a
  // surface to place the offset on. It is consumed below, once the offset
  // actually stuck.
  useEffect(() => {
    // A tab that is not in front cannot place an offset, and reading its own
    // snapshot while hidden would consume the very position "back" restores:
    // the tab the jump leaves stays mounted, so it must not read here.
    if (!isActive) return;
    const peeked = useFkNavigationStore.getState().peekViewport(panelId);
    setPendingViewport(peeked);
  }, [panelId, isActive, rows.length]);
  useEffect(() => {
    // The rows are part of the dependency list on purpose: entering a tab whose
    // data is still loading clamps the offset to 0 and burns the frame budget,
    // so the arrival of the rows restarts the placement with a fresh budget.
    if (!pendingViewport || !scrollElement || !isActive) return;
    const target =
      pendingViewport.anchorRow !== null
        ? scrollTopForAnchorRow(pendingViewport.anchorRow, rowHeight)
        : pendingViewport.scrollTop;
    // The body is virtualised, so the surface can be present and still refuse
    // the offset for a while: a tab that comes back loading renders no spacer,
    // its scroll height equals its client height, and the assignment is clamped
    // to 0. That is a *state* to wait out, not a failure — and the row count
    // does not change while it happens, so nothing would restart a frame loop
    // that gave up. Retry on a timer until the offset sticks, and only drop the
    // snapshot once it does.
    let attempts = 0;
    let timer = 0;
    const apply = () => {
      if (scrollElement.clientHeight > 0) {
        scrollElement.scrollTop = target;
      }
      if (Math.abs(scrollElement.scrollTop - target) <= 1) {
        useFkNavigationStore.getState().takeViewport(panelId);
        setPendingViewport(null);
        return;
      }
      attempts += 1;
      // Out of attempts is not a failure either: the surface may have been
      // replaced under us. The snapshot is kept, so entering the tab again
      // starts another placement.
      if (attempts >= MAX_RESTORE_ATTEMPTS) return;
      timer = window.setTimeout(apply, RESTORE_RETRY_MS);
    };
    apply();
    const { anchorRow, anchorColumn } = pendingViewport;
    if (anchorRow !== null && anchorColumn) {
      requestAnimationFrame(() => {
        const cell = document.querySelector(
          `[data-dt-row="${anchorRow}"][data-dt-col="${anchorColumn}"]`,
        );
        if (cell instanceof HTMLElement) cell.focus();
      });
    }
    return () => window.clearTimeout(timer);
    // A tab that is not in front is remounted hidden and only becomes the
    // scrolling surface once it is selected again; the placement has to restart
    // at that point, not at mount.
  }, [pendingViewport, scrollElement, rowHeight, rows.length, isActive]);

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
    onShowReferencingRows: handleShowReferencingRows,
    onKeyDown,
    onScrollElementChange,
  };
}
