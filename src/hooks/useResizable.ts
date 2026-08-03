import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseResizableOptions {
  direction: 'horizontal' | 'vertical';
  initialSize: number;
  minSize: number;
  maxSize: number;
  storageKey?: string;
  /** When true, dragging towards positive axis shrinks the panel (for right-side panels). */
  reverse?: boolean;
}

export function useResizable({
  direction,
  initialSize,
  minSize,
  maxSize,
  storageKey,
  reverse = false,
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

  const [handleEl, setHandleEl] = useState<HTMLDivElement | null>(null);
  const handleRef = useCallback((el: HTMLDivElement | null) => {
    setHandleEl(el);
  }, []);

  const clamp = useCallback(
    (n: number) => Math.max(minSize, Math.min(maxSize, n)),
    [minSize, maxSize],
  );

  useEffect(() => {
    if (!handleEl) return;

    let startPos = 0;
    let startSize = 0;
    let active = false;

    function onPointerDown(e: PointerEvent) {
      active = true;
      startPos = direction === 'horizontal' ? e.clientX : e.clientY;
      startSize = sizeRef.current;
      handleEl!.setPointerCapture(e.pointerId);
      document.body.style.cursor = direction === 'horizontal' ? 'col-resize' : 'row-resize';
      document.body.style.userSelect = 'none';
    }

    function onPointerMove(e: PointerEvent) {
      if (!active || !handleEl!.hasPointerCapture(e.pointerId)) return;
      const pos = direction === 'horizontal' ? e.clientX : e.clientY;
      const delta = pos - startPos;
      const next = clamp(startSize + (reverse ? -delta : delta));
      setSize(next);
    }

    function onPointerUp(e: PointerEvent) {
      if (!handleEl!.hasPointerCapture(e.pointerId)) return;
      active = false;
      handleEl!.releasePointerCapture(e.pointerId);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      if (storageKey) {
        localStorage.setItem(`resize:${storageKey}`, String(sizeRef.current));
      }
    }

    handleEl.addEventListener('pointerdown', onPointerDown);
    handleEl.addEventListener('pointermove', onPointerMove);
    handleEl.addEventListener('pointerup', onPointerUp);
    handleEl.addEventListener('pointercancel', onPointerUp);

    return () => {
      handleEl.removeEventListener('pointerdown', onPointerDown);
      handleEl.removeEventListener('pointermove', onPointerMove);
      handleEl.removeEventListener('pointerup', onPointerUp);
      handleEl.removeEventListener('pointercancel', onPointerUp);
    };
  }, [handleEl, clamp, direction, reverse, storageKey]);

  return { size, setSize, handleRef };
}
