/**
 * cap-bridge-BUG-001 coverage — confirmDialogBridge 专属单测。
 *
 * Paths covered:
 *  1. unbound `useBoundConfirmDialog` throws the documented message;
 *  2. bound hook forwarding: returns the `[confirmFn, node]` tuple and the
 *     confirm function passes options through verbatim (React mount).
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import type {
  BoundConfirmDialogHook,
  ConfirmDialogFn,
  ConfirmDialogOptions,
} from '../src/confirmDialogBridge';

type ConfirmDialogBridgeModule = typeof import('../src/confirmDialogBridge');

const NOT_BOUND_MESSAGE = 'ConfirmDialog has not been bound to driver-sdk yet.';

/** Fresh module instance so the unbound branch is reachable after other suites bind. */
async function importFreshBridge(): Promise<ConfirmDialogBridgeModule> {
  vi.resetModules();
  return import('../src/confirmDialogBridge');
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('confirmDialogBridge (unbound)', () => {
  it('useBoundConfirmDialog throws with the documented message', async () => {
    const mod = await importFreshBridge();
    expect(() => mod.useBoundConfirmDialog()).toThrow(NOT_BOUND_MESSAGE);
  });
});

describe('confirmDialogBridge (bound)', () => {
  it('returns the [confirmFn, node] tuple and passes options through verbatim', async () => {
    const mod = await importFreshBridge();
    const confirmFn = vi.fn<ConfirmDialogFn>(async () => true);
    const dialogNode: ReactNode = <span data-testid="dialog-node" />;
    const boundHook = vi.fn<BoundConfirmDialogHook>(() => [confirmFn, dialogNode]);
    mod.bindConfirmDialog(boundHook);

    let captured: { tuple?: [ConfirmDialogFn, ReactNode] } = {};
    function Probe() {
      captured.tuple = mod.useBoundConfirmDialog();
      return <>{captured.tuple[1]}</>;
    }

    render(<Probe />);
    expect(boundHook).toHaveBeenCalledTimes(1);
    // The node half of the tuple is what the host hook asked to render.
    expect(screen.queryByTestId('dialog-node')).not.toBeNull();
    expect(captured.tuple?.[0]).toBe(confirmFn);
    expect(captured.tuple?.[1]).toBe(dialogNode);

    const options: ConfirmDialogOptions = {
      title: 'Drop key',
      message: 'This is irreversible.',
      kind: 'warning',
      codePreview: 'DEL mykey',
    };
    await expect(captured.tuple?.[0]?.(options)).resolves.toBe(true);
    expect(confirmFn).toHaveBeenCalledTimes(1);
    expect(confirmFn.mock.calls[0]?.[0]).toBe(options);
  });

  it('forwards the bound hook response (cancel path) and re-renders per hook state', async () => {
    const mod = await importFreshBridge();
    const confirmFn = vi.fn<ConfirmDialogFn>(async () => false);
    mod.bindConfirmDialog(() => [confirmFn, null]);

    function Probe() {
      const [confirm, node] = mod.useBoundConfirmDialog();
      return (
        <div>
          {node}
          <span data-testid="result">{String(typeof confirm)}</span>
        </div>
      );
    }

    render(<Probe />);
    expect(screen.getByTestId('result').textContent).toBe('function');
    await expect(confirmFn({ title: 't', message: 'm' })).resolves.toBe(false);
  });
});
