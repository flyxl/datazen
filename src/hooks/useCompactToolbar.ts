import { useEffect, useRef, useState } from 'react';

/** Hide toolbar labels when the container is narrower than `threshold` (px). */
export function useCompactToolbar(threshold = 920) {
  const ref = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const update = (width: number) => {
      setCompact(width < threshold);
    };

    update(el.getBoundingClientRect().width);
    const ro = new ResizeObserver(([entry]) => {
      update(entry.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [threshold]);

  return { ref, compact };
}
