import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';
import { useConnectionStore } from '../stores/connectionStore';

/**
 * Listens for cross-window connection-change events to keep the list in sync.
 * Should be mounted once in the main window.
 */
export function useTauriEvent() {
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;

    void (async () => {
      try {
        const unlisten = await listenCrossWindow(
          'datazen:connections-changed',
          () => {
            if (!cancelled) void fetchConnections();
          },
        );
        if (cancelled) unlisten();
        else cleanup = unlisten;
      } catch {
        // Not available in current environment
      }
    })();

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [fetchConnections]);
}
