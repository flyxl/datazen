import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, act, cleanup } from '@testing-library/react';
import { useCompactToolbar, estimateExpandedToolbarWidth } from '../useCompactToolbar';

/* ------------------------------------------------------------------ */
/* estimateExpandedToolbarWidth                                        */
/* ------------------------------------------------------------------ */
describe('estimateExpandedToolbarWidth', () => {
  it('returns padding-only width for zero buttons', () => {
    expect(estimateExpandedToolbarWidth({ expandedButtonCount: 0 })).toBe(32);
  });

  it('scales with button count and fixed extras', () => {
    expect(
      estimateExpandedToolbarWidth({
        expandedButtonCount: 6,
        fixedExtraWidth: 120,
      }),
    ).toBe(32 + 6 * 96 + 5 * 8 + 120);
  });
});

/* ------------------------------------------------------------------ */
/* useCompactToolbar hook                                              */
/* ------------------------------------------------------------------ */

/** Minimal test component that exposes the compact state. */
function ToolbarProbe({ threshold }: { threshold?: number }) {
  const { ref, compact } = useCompactToolbar(threshold);
  return (
    <div ref={ref} data-testid="probe">
      {compact ? 'compact' : 'expanded'}
    </div>
  );
}

// Mock ResizeObserver used by the hook.
let resizeCallback: ResizeObserverCallback;
beforeEach(() => {
  resizeCallback = vi.fn();
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(cb: ResizeObserverCallback) {
        resizeCallback = cb;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  cleanup();
});

/** Simulate a resize by firing the ResizeObserver callback. */
function fireResize(width: number) {
  act(() => {
    resizeCallback(
      [{ contentRect: { width } } as ResizeObserverEntry],
      {} as ResizeObserver,
    );
  });
}

describe('useCompactToolbar', () => {
  it('starts expanded when container meets the threshold', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');
    Object.defineProperty(el, 'clientWidth', { value: 400, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 400, configurable: true });

    fireResize(400);
    expect(el.textContent).toBe('expanded');
  });

  it('starts expanded when container exceeds the threshold', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');
    Object.defineProperty(el, 'clientWidth', { value: 600, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 500, configurable: true });

    fireResize(600);
    expect(el.textContent).toBe('expanded');
  });

  it('starts compact when container is narrower than threshold', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');
    Object.defineProperty(el, 'clientWidth', { value: 300, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 300, configurable: true });

    fireResize(300);
    expect(el.textContent).toBe('compact');
  });

  it('expands when width grows past threshold + hysteresis', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');

    // Start compact: width = 300 (< 400)
    Object.defineProperty(el, 'clientWidth', { value: 300, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 300, configurable: true });
    fireResize(300);
    expect(el.textContent).toBe('compact');

    // Resize to threshold + 16 → should expand
    Object.defineProperty(el, 'clientWidth', { value: 416, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 416, configurable: true });
    fireResize(416);
    expect(el.textContent).toBe('expanded');
  });

  it('stays compact when width is between threshold and threshold + 16', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');

    // Start compact
    Object.defineProperty(el, 'clientWidth', { value: 300, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 300, configurable: true });
    fireResize(300);
    expect(el.textContent).toBe('compact');

    // Width = 410 (>= threshold but < threshold + 16) → stays compact
    Object.defineProperty(el, 'clientWidth', { value: 410, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 410, configurable: true });
    fireResize(410);
    expect(el.textContent).toBe('compact');
  });

  it('does NOT compact when width >= threshold even if scrollWidth overflows slightly', () => {
    // Core bug fix: scrollWidth can exceed clientWidth due to unaccounted
    // elements (separators, badges, hints) without the content being genuinely
    // too wide.  Previously this triggered compact and the toolbar got stuck
    // because hysteresis required threshold+16 to recover.
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');
    Object.defineProperty(el, 'clientWidth', { value: 400, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 410, configurable: true });

    fireResize(400);
    expect(el.textContent).toBe('expanded');
  });

  it('compacts when width < threshold regardless of scrollWidth', () => {
    const { getByTestId } = render(<ToolbarProbe threshold={400} />);
    const el = getByTestId('probe');

    // Below threshold, even if scrollWidth == clientWidth
    Object.defineProperty(el, 'clientWidth', { value: 350, configurable: true });
    Object.defineProperty(el, 'scrollWidth', { value: 350, configurable: true });
    fireResize(350);
    expect(el.textContent).toBe('compact');
  });
});
