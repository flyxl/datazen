/**
 * Test double for the host's `onPanelClosed`.
 *
 * The driver is handed one function and must not learn anything else about the
 * host (see `@datazen/driver-sdk` `ConnectionViewProps`). So the double records
 * what the driver registers and lets a case fire the close — which is the only
 * way to exercise the real disposal path, since the host fires it from its own
 * store diff.
 *
 * Lives outside `__tests__/` on purpose: the driver vitest config collects every
 * `.ts`/`.tsx` under `__tests__/` as a test file, so a shared helper placed
 * there would run as an empty suite.
 */
export interface PanelCloseStub {
  /** Pass straight to the view under test. */
  onPanelClosed: (handler: () => void) => () => void;
  /** How many handlers are currently registered. */
  readonly count: number;
  /** Fire every registered handler, as the host does when a tab is closed. */
  close(): void;
  /** Drop all handlers without firing them (tab switched away, not closed). */
  clear(): void;
}

export function panelCloseStub(): PanelCloseStub {
  const handlers = new Set<() => void>();
  return {
    onPanelClosed(handler) {
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    get count() {
      return handlers.size;
    },
    close() {
      const pending = [...handlers];
      handlers.clear();
      for (const handler of pending) handler();
    },
    clear() {
      handlers.clear();
    },
  };
}
