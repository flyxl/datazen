import { useVirtualizer } from '@tanstack/react-virtual';
import type { RefObject } from 'react';

export interface UseVirtualTableOptions {
  rows: unknown[][];
  rowHeight: number;
  overscan?: number;
  containerRef: RefObject<HTMLDivElement | null>;
}

export function useVirtualTable({
  rows,
  rowHeight,
  overscan = 10,
  containerRef,
}: UseVirtualTableOptions) {
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => rowHeight,
    overscan,
  });

  return {
    virtualRows: virtualizer.getVirtualItems(),
    totalHeight: virtualizer.getTotalSize(),
    scrollToRow: virtualizer.scrollToIndex,
  };
}
