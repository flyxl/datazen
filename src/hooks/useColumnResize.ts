import { useCallback, useRef, useState } from 'react';

const DEFAULT_COL_WIDTH = 160;
const MIN_COL_WIDTH = 60;

export interface UseColumnResizeOptions {
  /** Number of columns. */
  count: number;
  /** Default width per column (px). */
  defaultWidth?: number;
  /** Minimum width per column (px). */
  minWidth?: number;
}

export function useColumnResize({
  count,
  defaultWidth = DEFAULT_COL_WIDTH,
  minWidth = MIN_COL_WIDTH,
}: UseColumnResizeOptions) {
  const [widths, setWidths] = useState<number[]>(() =>
    Array.from({ length: count }, () => defaultWidth),
  );

  const widthsRef = useRef(widths);
  widthsRef.current = widths;

  // Sync length when column count changes
  if (widths.length !== count) {
    const next = Array.from({ length: count }, (_, i) => widths[i] ?? defaultWidth);
    // Intentionally set during render for synchronisation; safe because
    // the value is derived from props + previous state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    setWidths(next);
  }

  const onResizeStart = useCallback(
    (colIndex: number, startX: number) => {
      const startWidth = widthsRef.current[colIndex] ?? defaultWidth;

      const onMove = (e: PointerEvent) => {
        const delta = e.clientX - startX;
        const next = Math.max(minWidth, startWidth + delta);
        setWidths((prev) => {
          const copy = [...prev];
          copy[colIndex] = next;
          return copy;
        });
      };

      const onUp = () => {
        document.removeEventListener('pointermove', onMove);
        document.removeEventListener('pointerup', onUp);
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
      };

      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    },
    [defaultWidth, minWidth],
  );

  return { columnWidths: widths, onResizeStart };
}
