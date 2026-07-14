import { useVirtualizer } from '@tanstack/react-virtual';

export interface UseVirtualTableOptions {
  rows: unknown[][];
  rowHeight: number;
  overscan?: number;
  scrollElement: HTMLDivElement | null;
}

export function useVirtualTable({
  rows,
  rowHeight,
  overscan = 10,
  scrollElement,
}: UseVirtualTableOptions) {
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElement,
    estimateSize: () => rowHeight,
    overscan,
  });

  return {
    virtualRows: virtualizer.getVirtualItems(),
    totalHeight: virtualizer.getTotalSize(),
    scrollToRow: virtualizer.scrollToIndex,
  };
}
