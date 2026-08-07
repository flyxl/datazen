import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';

/** Opens the docs window when Help → Documentation is chosen from the native menu. */
export function useDocsMenuListener() {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listenCrossWindow('menu:open-docs', (payload) => {
      if (cancelled) return;
      const section = typeof payload === 'string' ? payload : undefined;
      void import('../lib/windowManager').then((m) => m.openDocsWindow(section));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
