import { vi } from 'vitest';

/** Shared mutable state for ConnectionPage (and other) tests using dynamic `import('@tauri-apps/api/window')`. */
export const tauriWindowTestState = {
  closeMock: vi.fn().mockResolvedValue(undefined),
  minimizeMock: vi.fn().mockResolvedValue(undefined),
  closeRequestedHandler: {
    // The contract's callback slot is `=> void` (contracts.ts WindowChromePort):
    // an async handler is assignable to it, a sync one too, so the stored
    // handler may return either.
    current: null as null | ((event: { preventDefault: () => void }) => void | Promise<void>),
  },
  closeHandlerRegistrationCount: { current: 0 },
};

export function getCurrentWindow() {
  return {
    close: (...args: unknown[]) => tauriWindowTestState.closeMock(...args),
    minimize: (...args: unknown[]) => tauriWindowTestState.minimizeMock(...args),
    onCloseRequested: (
      handler: (event: { preventDefault: () => void }) => void | Promise<void>,
    ) => {
      tauriWindowTestState.closeHandlerRegistrationCount.current += 1;
      tauriWindowTestState.closeRequestedHandler.current = handler;
      return Promise.resolve(() => {});
    },
  };
}
