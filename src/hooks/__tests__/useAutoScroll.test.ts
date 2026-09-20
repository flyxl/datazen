import { describe, expect, it, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useAutoScroll, useTrackUnread } from '../useAutoScroll';

describe('useAutoScroll', () => {
  it('starts with atBottom=true and unreadCount=0', () => {
    const { result } = renderHook(() => useAutoScroll());
    expect(result.current.atBottom).toBe(true);
    expect(result.current.unreadCount).toBe(0);
  });

  it('has a containerRef callback function', () => {
    const { result } = renderHook(() => useAutoScroll());
    expect(typeof result.current.containerRef).toBe('function');
  });

  it('provides onScroll callback', () => {
    const { result } = renderHook(() => useAutoScroll());
    expect(typeof result.current.onScroll).toBe('function');
  });

  it('provides jumpToBottom callback', () => {
    const { result } = renderHook(() => useAutoScroll());
    expect(typeof result.current.jumpToBottom).toBe('function');
  });
});

describe('useAutoScroll integration', () => {
  it('detects scroll position via mock element through callback ref', () => {
    const { result } = renderHook(() => useAutoScroll({ threshold: 50 }));

    // Mock a scroll container that is at the bottom
    const mockEl = {
      scrollHeight: 1000,
      scrollTop: 900,
      clientHeight: 100,
      // distance from bottom = 1000 - 900 - 100 = 0 → at bottom
    };

    // Simulate attaching via callback ref
    act(() => {
      (result.current.containerRef as (el: HTMLDivElement | null) => void)(
        mockEl as unknown as HTMLDivElement,
      );
    });

    act(() => {
      result.current.onScroll();
    });

    expect(result.current.atBottom).toBe(true);
  });

  it('detects when scrolled away from bottom', () => {
    const { result } = renderHook(() => useAutoScroll({ threshold: 50 }));

    // Mock a scroll container that is NOT at the bottom
    const mockEl = {
      scrollHeight: 1000,
      scrollTop: 500,
      clientHeight: 100,
      // distance from bottom = 1000 - 500 - 100 = 400 → NOT at bottom
    };

    act(() => {
      (result.current.containerRef as (el: HTMLDivElement | null) => void)(
        mockEl as unknown as HTMLDivElement,
      );
    });

    act(() => {
      result.current.onScroll();
    });

    expect(result.current.atBottom).toBe(false);
  });

  it('[tester] jumpToBottom resets state and scrolls element', () => {
    const { result } = renderHook(() => useAutoScroll({ threshold: 50 }));

    const mockEl = {
      scrollHeight: 1000,
      scrollTop: 500,
      clientHeight: 100,
    };

    act(() => {
      (result.current.containerRef as (el: HTMLDivElement | null) => void)(
        mockEl as unknown as HTMLDivElement,
      );
    });

    // Scroll away
    act(() => {
      result.current.onScroll();
    });
    expect(result.current.atBottom).toBe(false);

    // Jump to bottom
    act(() => {
      result.current.jumpToBottom();
    });
    expect(result.current.atBottom).toBe(true);
    expect(mockEl.scrollTop).toBe(1000);
  });

  it('[tester] handles null container gracefully', () => {
    const { result } = renderHook(() => useAutoScroll());

    // Call onScroll without attaching a container — should not throw
    act(() => {
      result.current.onScroll();
    });
    expect(result.current.atBottom).toBe(true);
  });

  it('[tester] handles empty container (scrollHeight === clientHeight)', () => {
    const { result } = renderHook(() => useAutoScroll({ threshold: 50 }));

    const mockEl = {
      scrollHeight: 100,
      scrollTop: 0,
      clientHeight: 100,
      // distance from bottom = 100 - 0 - 100 = 0 → at bottom
    };

    act(() => {
      (result.current.containerRef as (el: HTMLDivElement | null) => void)(
        mockEl as unknown as HTMLDivElement,
      );
    });

    act(() => {
      result.current.onScroll();
    });

    expect(result.current.atBottom).toBe(true);
  });

  it('[tester] default threshold is 120px', () => {
    const { result } = renderHook(() => useAutoScroll());

    // Distance from bottom = 100 (within default 120px threshold)
    const mockEl = {
      scrollHeight: 1000,
      scrollTop: 780,
      clientHeight: 100,
      // distance = 1000 - 780 - 100 = 120 → exactly at threshold
    };

    act(() => {
      (result.current.containerRef as (el: HTMLDivElement | null) => void)(
        mockEl as unknown as HTMLDivElement,
      );
    });

    act(() => {
      result.current.onScroll();
    });

    expect(result.current.atBottom).toBe(true);
  });
});

describe('useTrackUnread', () => {
  it('[tester] starts with 0 unread', () => {
    const { result } = renderHook(() => useTrackUnread(0, true));
    expect(result.current).toBe(0);
  });

  it('[tester] increments unread when messages arrive while scrolled up', () => {
    const { result, rerender } = renderHook(
      ({ count, atBottom }) => useTrackUnread(count, atBottom),
      { initialProps: { count: 0, atBottom: false } },
    );

    rerender({ count: 3, atBottom: false });
    expect(result.current).toBe(3);
  });

  it('[tester] resets unread when at bottom', () => {
    const { result, rerender } = renderHook(
      ({ count, atBottom }) => useTrackUnread(count, atBottom),
      { initialProps: { count: 0, atBottom: true } },
    );

    // Scroll up, get messages
    rerender({ count: 5, atBottom: false });
    expect(result.current).toBe(5);

    // Scroll back to bottom
    rerender({ count: 5, atBottom: true });
    expect(result.current).toBe(0);
  });

  it('[tester] does not decrement when messages decrease', () => {
    const { result, rerender } = renderHook(
      ({ count, atBottom }) => useTrackUnread(count, atBottom),
      { initialProps: { count: 5, atBottom: false } },
    );

    // First render: prevRef=0, messageCount=5, delta=5 → unread=5
    expect(result.current).toBe(5);

    // Messages decrease from 5 to 3 — delta is negative, no increment
    rerender({ count: 3, atBottom: false });
    expect(result.current).toBe(5); // unchanged
  });

  it('[tester] accumulates unread across multiple batches', () => {
    const { result, rerender } = renderHook(
      ({ count, atBottom }) => useTrackUnread(count, atBottom),
      { initialProps: { count: 0, atBottom: false } },
    );

    rerender({ count: 2, atBottom: false });
    expect(result.current).toBe(2);

    rerender({ count: 5, atBottom: false });
    expect(result.current).toBe(5);
  });
});
