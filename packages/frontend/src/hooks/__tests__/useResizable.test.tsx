import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import { useResizable } from '../useResizable';

interface HarnessProps {
  getStartSize?: () => number | null | undefined;
  onResizeStart?: () => void;
}

/** Renders the hook and exposes its current size for assertions. */
function Harness({ getStartSize, onResizeStart }: HarnessProps) {
  const { size, handleRef } = useResizable({
    direction: 'vertical',
    initialSize: 280,
    minSize: 100,
    maxSize: 900,
    getStartSize,
    onResizeStart,
  });

  return (
    <div>
      <span data-testid="size">{size}</span>
      <div ref={handleRef} data-testid="handle" />
    </div>
  );
}

/**
 * jsdom implements `PointerEvent` but not pointer capture, and the hook needs
 * capture to route pointermove events during a drag.
 */
function primePointerCapture(el: HTMLElement) {
  const captured = new Set<unknown>();
  el.setPointerCapture = (id: number) => captured.add(id);
  el.hasPointerCapture = (id: number) => captured.has(id);
  el.releasePointerCapture = (id: number) => captured.delete(id);
}

function renderHarness(props: HarnessProps = {}) {
  render(<Harness {...props} />);
  const handle = screen.getByTestId('handle');
  primePointerCapture(handle);
  return handle;
}

const size = () => screen.getByTestId('size').textContent;

describe('useResizable', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('starts from initialSize and follows the pointer', () => {
    const handle = renderHarness();

    fireEvent.pointerDown(handle, { pointerId: 1, clientY: 100 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 140 });
    expect(size()).toBe('320');

    // Clamped to maxSize.
    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 5000 });
    expect(size()).toBe('900');
  });

  it('adopts a measured start size so taking over CSS sizing cannot jump', () => {
    const onResizeStart = vi.fn();
    const handle = renderHarness({ getStartSize: () => 512, onResizeStart });

    fireEvent.pointerDown(handle, { pointerId: 1, clientY: 100 });
    expect(onResizeStart).toHaveBeenCalledTimes(1);
    // 512 is not the hook's own 280: it must be adopted on pointerdown, before
    // the first move, so the element keeps the height it had on screen.
    expect(size()).toBe('512');

    fireEvent.pointerMove(handle, { pointerId: 1, clientY: 130 });
    expect(size()).toBe('542');
  });

  it('falls back to the tracked size when nothing usable can be measured', () => {
    // 0 is what a hidden / unlaid-out panel reports.
    const handle = renderHarness({ getStartSize: () => 0 });

    fireEvent.pointerDown(handle, { pointerId: 1, clientY: 100 });
    expect(size()).toBe('280');
  });
});
