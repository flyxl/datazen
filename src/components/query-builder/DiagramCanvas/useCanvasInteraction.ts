import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseCanvasInteractionOptions {
  zoom: number;
  onZoomChange: (zoom: number) => void;
}

/** Scroll-and-zoom viewport behaviour for the diagram canvas. */
export interface CanvasInteraction {
  /** Attach to the scroll container. */
  scrollRef: (el: HTMLDivElement | null) => void;
  /** Wheel handler: scroll normally, zoom with Ctrl/Cmd held. */
  handleWheel: (e: React.WheelEvent) => void;
  handlePointerDown: (e: React.PointerEvent) => void;
  handlePointerMove: (e: React.PointerEvent) => void;
  handlePointerUp: () => void;
  /** Convert a client point to canvas coordinates (undoing scroll + zoom). */
  screenToCanvas: (clientX: number, clientY: number) => { x: number; y: number };
  isPanning: boolean;
  /** Measured size of the scroll container. */
  viewport: { width: number; height: number };
}

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 2;
const ZOOM_STEP = 0.1;

/**
 * Canvas viewport: **native scrolling** for panning plus Ctrl/Cmd + wheel for
 * zooming — the same model as Navicat's builder and most diagram editors, and
 * the reason cards can grow to their full height instead of scrolling
 * internally (an internally scrolled row would put its anchor outside the card).
 *
 * The content is laid out unscaled inside a spacer sized `content * zoom`, so
 * the scrollbars reflect the zoomed extent rather than the layout extent.
 */
export function useCanvasInteraction({
  zoom,
  onZoomChange,
}: UseCanvasInteractionOptions): CanvasInteraction {
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const scrollRef = useCallback((el: HTMLDivElement | null) => setScrollEl(el), []);

  const [isPanning, setIsPanning] = useState(false);
  const panStartRef = useRef<{
    x: number;
    y: number;
    scrollLeft: number;
    scrollTop: number;
  } | null>(null);
  const spaceHeldRef = useRef(false);
  const [viewport, setViewport] = useState({ width: 0, height: 0 });

  const zoomRef = useRef(zoom);
  zoomRef.current = zoom;

  // Track the viewport size so the content can never be smaller than it.
  useEffect(() => {
    if (!scrollEl) return;
    const measure = () =>
      setViewport({ width: scrollEl.clientWidth, height: scrollEl.clientHeight });
    measure();
    // jsdom has no ResizeObserver; fall back to window resize so tests and any
    // exotic webview still size correctly.
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(scrollEl);
    return () => observer.disconnect();
  }, [scrollEl]);

  // Space = temporary pan mode (middle-drag always pans).
  useEffect(() => {
    const isTextField = (target: EventTarget | null) =>
      target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !e.repeat && !isTextField(e.target)) {
        e.preventDefault();
        spaceHeldRef.current = true;
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spaceHeldRef.current = false;
        setIsPanning(false);
        panStartRef.current = null;
      }
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('keyup', onKeyUp);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('keyup', onKeyUp);
    };
  }, []);

  /**
   * Wheel: plain wheel/trackpad scrolls the canvas natively; Ctrl/Cmd + wheel
   * zooms around the pointer so the point under the cursor stays put.
   */
  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return; // let the browser scroll
      e.preventDefault();
      const el = scrollEl;
      const current = zoomRef.current;
      const delta = e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP;
      const nextZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number((current + delta).toFixed(2))));
      if (nextZoom === current) return;

      if (!el) {
        onZoomChange(nextZoom);
        return;
      }

      const rect = el.getBoundingClientRect();
      const pointerX = e.clientX - rect.left;
      const pointerY = e.clientY - rect.top;
      const ratio = nextZoom / current;
      const nextScrollLeft = (el.scrollLeft + pointerX) * ratio - pointerX;
      const nextScrollTop = (el.scrollTop + pointerY) * ratio - pointerY;
      onZoomChange(nextZoom);
      // The spacer resizes on the next render, so restore the scroll after it.
      requestAnimationFrame(() => {
        el.scrollLeft = Math.max(0, nextScrollLeft);
        el.scrollTop = Math.max(0, nextScrollTop);
      });
    },
    [onZoomChange, scrollEl],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      // Middle button always pans; left button pans only while Space is held.
      if (e.button !== 1 && !(e.button === 0 && spaceHeldRef.current)) return;
      if (!scrollEl) return;
      e.preventDefault();
      setIsPanning(true);
      panStartRef.current = {
        x: e.clientX,
        y: e.clientY,
        scrollLeft: scrollEl.scrollLeft,
        scrollTop: scrollEl.scrollTop,
      };
    },
    [scrollEl],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      const start = panStartRef.current;
      if (!isPanning || !start || !scrollEl) return;
      // Panning *is* scrolling: dragging right moves the content right.
      scrollEl.scrollLeft = start.scrollLeft - (e.clientX - start.x);
      scrollEl.scrollTop = start.scrollTop - (e.clientY - start.y);
    },
    [isPanning, scrollEl],
  );

  const handlePointerUp = useCallback(() => {
    setIsPanning(false);
    panStartRef.current = null;
  }, []);

  const screenToCanvas = useCallback(
    (clientX: number, clientY: number) => {
      if (!scrollEl) return { x: 0, y: 0 };
      const rect = scrollEl.getBoundingClientRect();
      const current = zoomRef.current;
      return {
        x: (clientX - rect.left + scrollEl.scrollLeft) / current,
        y: (clientY - rect.top + scrollEl.scrollTop) / current,
      };
    },
    [scrollEl],
  );

  return {
    scrollRef,
    handleWheel,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    screenToCanvas,
    isPanning,
    viewport,
  };
}
