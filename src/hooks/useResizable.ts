import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseResizableOptions {
  direction: 'horizontal' | 'vertical';
  initialSize: number;
  minSize: number;
  maxSize: number;
  storageKey?: string;
}

export function useResizable({
  direction,
  initialSize,
  minSize,
  maxSize,
  storageKey,
}: UseResizableOptions) {
  const [size, setSize] = useState(() => {
    if (storageKey) {
      const saved = localStorage.getItem(`resize:${storageKey}`);
      if (saved) {
        const n = Number(saved);
        if (!Number.isNaN(n)) return Math.max(minSize, Math.min(maxSize, n));
      }
    }
    return initialSize;
  });

  const sizeRef = useRef(size);
  sizeRef.current = size;

  const handleRef = useRef<HTMLDivElement | null>(null);

  const clamp = useCallback(
    (n: number) => Math.max(minSize, Math.min(maxSize, n)),
    [minSize, maxSize],
  );

  useEffect(() => {
    const handle = handleRef.current;
    if (!handle) return;

    let startPos = 0;
    let startSize = 0;
    let active = false;

    function onPointerDown(e: PointerEvent) {
      active = true;
      startPos = direction === 'horizontal' ? e.clientX : e.clientY;
      startSize = sizeRef.current;
      handle!.setPointerCapture(e.pointerId);
      document.body.style.cursor = direction === 'horizontal' ? 'col-resize' : 'row-resize';
      document.body.style.userSelect = 'none';
    }

    function onPointerMove(e: PointerEvent) {
      if (!active || !handle!.hasPointerCapture(e.pointerId)) return;
      const pos = direction === 'horizontal' ? e.clientX : e.clientY;
      const delta = pos - startPos;
      const next = clamp(startSize + delta);
      setSize(next);
    }

    function onPointerUp(e: PointerEvent) {
      if (!handle!.hasPointerCapture(e.pointerId)) return;
      active = false;
      handle!.releasePointerCapture(e.pointerId);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      if (storageKey) {
        localStorage.setItem(`resize:${storageKey}`, String(sizeRef.current));
      }
    }

    handle.addEventListener('pointerdown', onPointerDown);
    handle.addEventListener('pointermove', onPointerMove);
    handle.addEventListener('pointerup', onPointerUp);
    handle.addEventListener('pointercancel', onPointerUp);

    return () => {
      handle.removeEventListener('pointerdown', onPointerDown);
      handle.removeEventListener('pointermove', onPointerMove);
      handle.removeEventListener('pointerup', onPointerUp);
      handle.removeEventListener('pointercancel', onPointerUp);
    };
  }, [clamp, direction, storageKey]);

  return { size, setSize, handleRef };
}
