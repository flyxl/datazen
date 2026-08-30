import { vi } from 'vitest';

/** Shared mutable state for ConnectionPage (and other) tests using dynamic `import('@tauri-apps/api/window')`. */
export const tauriWindowTestState = {
  closeMock: vi.fn().mockResolvedValue(undefined),
  minimizeMock: vi.fn().mockResolvedValue(undefined),
  closeRequestedHandler: {
    current: null as null | ((event: { preventDefault: () => void }) => Promise<void>),
  },
  closeHandlerRegistrationCount: { current: 0 },
};

export function getCurrentWindow() {
  return {
    close: (...args: unknown[]) => tauriWindowTestState.closeMock(...args),
    minimize: (...args: unknown[]) => tauriWindowTestState.minimizeMock(...args),
    onCloseRequested: (handler: (event: { preventDefault: () => void }) => Promise<void>) => {
      tauriWindowTestState.closeHandlerRegistrationCount.current += 1;
      tauriWindowTestState.closeRequestedHandler.current = handler;
      return Promise.resolve(() => {});
    },
  };
}
