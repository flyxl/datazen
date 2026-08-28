import { useLayoutEffect, useRef, useState } from 'react';

/** Estimated width of one expanded toolbar button (icon + label + padding). */
export const TOOLBAR_EXPANDED_BUTTON_WIDTH = 96;
export const TOOLBAR_GAP = 8;
export const TOOLBAR_HORIZONTAL_PADDING = 32;

/** Estimate minimum toolbar width required to show text labels. */
export function estimateExpandedToolbarWidth(options: {
  expandedButtonCount: number;
  fixedExtraWidth?: number;
}): number {
  const { expandedButtonCount, fixedExtraWidth = 0 } = options;
  const count = Math.max(0, expandedButtonCount);
  return (
    TOOLBAR_HORIZONTAL_PADDING +
    count * TOOLBAR_EXPANDED_BUTTON_WIDTH +
    Math.max(0, count - 1) * TOOLBAR_GAP +
    fixedExtraWidth
  );
}

/** Hide toolbar labels when the container cannot fit expanded buttons. */
export function useCompactToolbar(threshold = 920) {
  const ref = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;

    const update = () => {
      const width = el.clientWidth;
      if (width <= 0) return;

      setCompact((prev) => {
        if (prev) {
          // Hysteresis: require a little extra room before expanding again.
          return width < threshold + 16;
        }
        const overflows = el.scrollWidth > width + 1;
        return overflows || width < threshold;
      });
    };

    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [threshold]);

  return { ref, compact };
}
