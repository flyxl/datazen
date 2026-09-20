import { useCallback, useEffect, useRef, useState, type RefCallback } from 'react';

interface UseAutoScrollOptions {
  /** Distance from bottom (px) to consider "at bottom". Default 120. */
  threshold?: number;
}

interface UseAutoScrollReturn {
  /** Whether the scroll container is at (or near) the bottom. */
  atBottom: boolean;
  /** Number of unread messages since the user scrolled up. */
  unreadCount: number;
  /** Scroll to the bottom immediately and reset unread count. */
  jumpToBottom: () => void;
  /** Attach to the scroll container's onScroll. */
  onScroll: () => void;
  /** Attach to the scroll container ref (callback ref). */
  containerRef: RefCallback<HTMLDivElement>;
}

/**
 * Hook that tracks whether a scrollable container is pinned to the bottom,
 * counts new messages that arrived while the user was scrolled up, and
 * provides a "jump to bottom" action.
 *
 * Usage:
 * ```tsx
 * const { atBottom, unreadCount, jumpToBottom, onScroll, containerRef } =
 *   useAutoScroll({ threshold: 120 });
 *
 * <div ref={containerRef} onScroll={onScroll} className="overflow-y-auto">
 *   {messages.map(m => <Bubble key={m.id} ... />)}
 * </div>
 * {!atBottom && unreadCount > 0 && (
 *   <button onClick={jumpToBottom}>↓ {unreadCount} new</button>
 * )}
 * ```
 */
export function useAutoScroll(options: UseAutoScrollOptions = {}): UseAutoScrollReturn {
  const { threshold = 120 } = options;

  const containerElRef = useRef<HTMLDivElement | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [unreadCount, setUnreadCount] = useState(0);

  const checkAtBottom = useCallback(() => {
    const el = containerElRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    const isAtBottom = distanceFromBottom <= threshold;
    setAtBottom(isAtBottom);
    if (isAtBottom) {
      setUnreadCount(0);
    }
  }, [threshold]);

  const jumpToBottom = useCallback(() => {
    const el = containerElRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
    setAtBottom(true);
    setUnreadCount(0);
  }, []);

  const containerRef: RefCallback<HTMLDivElement> = useCallback((node) => {
    containerElRef.current = node;
  }, []);

  // Expose onScroll to attach to the container.
  const onScroll = checkAtBottom;

  return {
    atBottom,
    unreadCount,
    jumpToBottom,
    onScroll,
    containerRef,
  };
}

/**
 * Given a message count, increment unread if the user is not at the bottom.
 * Call this inside the consuming component when `messages.length` changes.
 */
export function useTrackUnread(messageCount: number, atBottom: boolean): number {
  const prevRef = useRef(0);
  const [unread, setUnread] = useState(0);

  useEffect(() => {
    if (messageCount > prevRef.current) {
      const delta = messageCount - prevRef.current;
      if (atBottom) {
        setUnread(0);
      } else {
        setUnread((prev) => prev + delta);
      }
    }
    prevRef.current = messageCount;
  }, [messageCount, atBottom]);

  // Reset when user scrolls to bottom
  useEffect(() => {
    if (atBottom) {
      setUnread(0);
    }
  }, [atBottom]);

  return unread;
}
