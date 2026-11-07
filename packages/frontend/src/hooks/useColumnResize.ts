import { useCallback, useMemo, useRef, useState } from 'react';

const DEFAULT_COL_WIDTH = 150;
const MIN_COL_WIDTH = 60;
const MAX_COL_WIDTH = 400;
const HEADER_CHAR_PX = 6.5;
const DATA_CHAR_PX = 7.5;
const CELL_PADDING = 24;
const SAMPLE_ROWS = 20;

export const SORT_ICON_WIDTH = 36;

export function computeInitialColumnWidths(
  columns: { name: string; type?: string }[],
  rows: unknown[][],
): number[] {
  return columns.map((col, colIdx) => {
    const t = (col.type ?? '').toLowerCase();

    let typeWidth: number;
    if (t.includes('bool')) {
      typeWidth = 80;
    } else if (
      t.includes('serial') ||
      t.includes('bigint') ||
      t === 'integer' ||
      t === 'int4' ||
      t === 'int2' ||
      t === 'smallint'
    ) {
      typeWidth = 90;
    } else if (
      t.includes('numeric') ||
      t.includes('decimal') ||
      t.includes('float') ||
      t.includes('double') ||
      t.includes('real')
    ) {
      typeWidth = 100;
    } else if (t.includes('timestamp') || t.includes('date')) {
      typeWidth = 180;
    } else if (t.includes('json')) {
      typeWidth = 260;
    } else if (t.includes('text') || t.includes('char') || t.includes('varchar')) {
      typeWidth = 150;
    } else {
      typeWidth = DEFAULT_COL_WIDTH;
    }

    const headerWidth = col.name.length * HEADER_CHAR_PX + CELL_PADDING;

    let dataWidth = 0;
    const sampleCount = Math.min(rows.length, SAMPLE_ROWS);
    for (let r = 0; r < sampleCount; r++) {
      const cell = rows[r]?.[colIdx];
      if (cell == null) continue;
      const str = typeof cell === 'object' ? JSON.stringify(cell) : String(cell);
      const w = str.length * DATA_CHAR_PX + CELL_PADDING;
      if (w > dataWidth) dataWidth = w;
    }

    return Math.min(MAX_COL_WIDTH, Math.max(MIN_COL_WIDTH, typeWidth, headerWidth, dataWidth));
  });
}

export interface UseColumnResizeOptions {
  /** Column definitions for smart width calculation. */
  columns?: { name: string; type?: string }[];
  /** First page of rows for data-aware width calculation. */
  rows?: unknown[][];
  /** Number of columns (fallback when columns not provided). */
  count: number;
  /** Default width per column (px). */
  defaultWidth?: number;
  /** Minimum width per column (px). */
  minWidth?: number;
  /**
   * The widths this table was last left with, **by column name** (DB-12).
   * They are the base the smart widths have to beat: a column the user
   * resized keeps its width across reorders, page changes and restarts.
   */
  persistedWidths?: Record<string, number> | null;
  /**
   * Fired once per completed drag with the **effective** widths of the
   * current columns, keyed by name (DB-12) — the host persists them per
   * table. Not fired when `columns` is absent: without names there is
   * nothing stable to key a width by.
   */
  onWidthsCommit?: (widths: Record<string, number>) => void;
}

export function useColumnResize({
  columns,
  rows,
  count,
  defaultWidth = DEFAULT_COL_WIDTH,
  minWidth = MIN_COL_WIDTH,
  persistedWidths,
  onWidthsCommit,
}: UseColumnResizeOptions) {
  const smartWidths = useMemo(
    () => (columns && columns.length > 0 ? computeInitialColumnWidths(columns, rows ?? []) : null),
    // Re-compute only when column count changes (not on every row change)
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [columns?.length, columns?.map((c) => c.name).join(',')],
  );

  /**
   * The widths the user dragged, keyed by **column name** (DB-12): a width
   * belongs to a column, not to a position — reordering the columns must
   * not redistribute what the user resized.
   *
   * A caller without column names falls back to positional storage below:
   * its widths still drag, they just cannot survive a reorder.
   */
  const [resizedByName, setResizedByName] = useState<Record<string, number>>({});
  const [resizedByIndex, setResizedByIndex] = useState<Record<number, number>>({});
  const resizedRef = useRef(resizedByName);
  resizedRef.current = resizedByName;
  const resizedIndexRef = useRef(resizedByIndex);
  resizedIndexRef.current = resizedByIndex;

  const columnWidths = useMemo(() => {
    const smart = smartWidths ?? Array.from({ length: count }, () => defaultWidth);
    if (!columns) return smart.map((w, i) => resizedIndexRef.current[i] ?? w);
    return columns.map(
      (col, i) =>
        resizedRef.current[col.name] ?? persistedWidths?.[col.name] ?? smart[i] ?? defaultWidth,
    );
    // `resizedByName`/`resizedByIndex` are in the deps on purpose: a drag
    // must show immediately. The refs only keep the latest between renders.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [smartWidths, persistedWidths, count, defaultWidth, columns, resizedByName, resizedByIndex]);

  const onResizeStart = useCallback(
    (colIndex: number, startX: number) => {
      const startWidth = columnWidths[colIndex] ?? defaultWidth;
      const name = columns?.[colIndex]?.name;

      const onMove = (e: PointerEvent) => {
        const delta = e.clientX - startX;
        const next = Math.max(minWidth, startWidth + delta);
        if (name) {
          setResizedByName((prev) => ({ ...prev, [name]: next }));
        } else {
          // No names — the drag still works, filed by position.
          setResizedByIndex((prev) => ({ ...prev, [colIndex]: next }));
        }
      };

      const onUp = () => {
        document.removeEventListener('pointermove', onMove);
        document.removeEventListener('pointerup', onUp);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        // Report the widths that are now in force, by name: the host files
        // them under the table's identity (DB-12).
        if (columns && onWidthsCommit) {
          const effective: Record<string, number> = {};
          columns.forEach((col, i) => {
            effective[col.name] =
              resizedRef.current[col.name] ??
              persistedWidths?.[col.name] ??
              (smartWidths ?? [])[i] ??
              defaultWidth;
          });
          onWidthsCommit(effective);
        }
      };

      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    },
    [columnWidths, defaultWidth, minWidth, columns, persistedWidths, smartWidths, onWidthsCommit],
  );

  return { columnWidths, onResizeStart };
}

export function adjustWidthsForSort(
  widths: number[],
  columns: { name: string }[],
  sorts: { column: string; descending?: boolean }[],
): number[] {
  if (sorts.length === 0) return widths;
  // Every sorted column carries its own arrow (and, with a multi-column
  // sort, its position badge — DB-11), so every one of them needs the room.
  const sortedColumns = new Set(sorts.map((sort) => sort.column));
  return widths.map((w, i) => (sortedColumns.has(columns[i]?.name) ? w + SORT_ICON_WIDTH : w));
}
