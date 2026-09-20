/**
 * Canvas viewport semantics (R3).
 *
 * The canvas scrolls natively and zooms only with Ctrl/Cmd held. That split is
 * what allows cards to grow to their full height (no inner scrolling, so every
 * column anchor stays inside its card) while a tall diagram stays reachable.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useCanvasInteraction } from '../useCanvasInteraction';

let container: HTMLDivElement;

beforeEach(() => {
  container = document.createElement('div');
  document.body.appendChild(container);
  container.scrollLeft = 0;
  container.scrollTop = 0;
});

afterEach(() => {
  container.remove();
});

const mount = (zoom = 1, onZoomChange = vi.fn()) => {
  const view = renderHook(() => useCanvasInteraction({ zoom, onZoomChange }));
  // Attach the element the way the canvas does.
  act(() => view.result.current.scrollRef(container));
  return { view, onZoomChange };
};

const wheel = (over: Partial<React.WheelEvent> = {}) =>
  ({
    ctrlKey: false,
    metaKey: false,
    deltaY: 100,
    clientX: 50,
    clientY: 40,
    preventDefault: vi.fn(),
    ...over,
  }) as unknown as React.WheelEvent;

describe('useCanvasInteraction — wheel', () => {
  it('scrolls natively: a plain wheel neither zooms nor prevents default', () => {
    const { view, onZoomChange } = mount(1);
    const event = wheel();
    act(() => view.result.current.handleWheel(event));
    expect(onZoomChange).not.toHaveBeenCalled();
    expect(event.preventDefault).not.toHaveBeenCalled();
  });

  it('zooms out on Ctrl + wheel down', () => {
    const { view, onZoomChange } = mount(1);
    act(() => view.result.current.handleWheel(wheel({ ctrlKey: true })));
    expect(onZoomChange).toHaveBeenCalledWith(0.9);
  });

  it('zooms in on Cmd + wheel up', () => {
    const { view, onZoomChange } = mount(1);
    act(() => view.result.current.handleWheel(wheel({ metaKey: true, deltaY: -100 })));
    expect(onZoomChange).toHaveBeenCalledWith(1.1);
  });

  it('clamps at the minimum zoom', () => {
    const { view, onZoomChange } = mount(0.5);
    act(() => view.result.current.handleWheel(wheel({ ctrlKey: true })));
    expect(onZoomChange).not.toHaveBeenCalled();
  });

  it('clamps at the maximum zoom', () => {
    const { view, onZoomChange } = mount(2);
    act(() => view.result.current.handleWheel(wheel({ ctrlKey: true, deltaY: -100 })));
    expect(onZoomChange).not.toHaveBeenCalled();
  });

  it('prevents the browser default when it zooms', () => {
    const { view } = mount(1);
    const event = wheel({ ctrlKey: true });
    act(() => view.result.current.handleWheel(event));
    expect(event.preventDefault).toHaveBeenCalled();
  });
});

describe('useCanvasInteraction — screenToCanvas', () => {
  it('divides by zoom', () => {
    const { view } = mount(2);
    const point = view.result.current.screenToCanvas(100, 60);
    expect(point).toEqual({ x: 50, y: 30 });
  });

  it('accounts for the scroll position', () => {
    const { view } = mount(1);
    container.scrollLeft = 24;
    container.scrollTop = 48;
    expect(view.result.current.screenToCanvas(100, 60)).toEqual({ x: 124, y: 108 });
  });

  it('combines scroll and zoom', () => {
    const { view } = mount(2);
    container.scrollLeft = 20;
    container.scrollTop = 10;
    expect(view.result.current.screenToCanvas(100, 60)).toEqual({ x: 60, y: 35 });
  });
});

describe('useCanvasInteraction — panning', () => {
  it('middle drag scrolls the canvas', () => {
    const { view } = mount(1);
    act(() =>
      view.result.current.handlePointerDown({
        button: 1,
        clientX: 100,
        clientY: 100,
        preventDefault: vi.fn(),
      } as unknown as React.PointerEvent),
    );
    expect(view.result.current.isPanning).toBe(true);

    act(() =>
      view.result.current.handlePointerMove({
        clientX: 60,
        clientY: 130,
      } as unknown as React.PointerEvent),
    );
    // Dragging right moves the content right (scroll decreases).
    expect(container.scrollLeft).toBe(40);
    expect(container.scrollTop).toBe(-30);

    act(() => view.result.current.handlePointerUp());
    expect(view.result.current.isPanning).toBe(false);
  });

  it('a plain left drag does not pan (that drags cards)', () => {
    const { view } = mount(1);
    act(() =>
      view.result.current.handlePointerDown({
        button: 0,
        clientX: 100,
        clientY: 100,
        preventDefault: vi.fn(),
      } as unknown as React.PointerEvent),
    );
    expect(view.result.current.isPanning).toBe(false);
  });
});
