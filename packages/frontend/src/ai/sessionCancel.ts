/**
 * `ai_cancel` wrapper + the 8s terminal fallback (P6 §14.4).
 *
 * The backend answers with one of three receipts:
 *
 * - `marked` → the stream machine enters `cancelling` and waits for the
 *   terminal chunk (`done && cancelled`). If the chunk never arrives within
 *   `CANCEL_TIMEOUT_MS`, the machine is forced to `terminal(cancelled)` and
 *   the host callback finalises the UI, so "Stop" can never leave a run
 *   spinning forever.
 * - `alreadyFinished` → nothing to do; the normal terminal chunk path owns
 *   the transition.
 * - `forbidden` → warn only. The caller is not allowed to cancel this run;
 *   silently pretending it stopped would be worse than leaving it running.
 *
 * A transport failure on the IPC call itself is also warn-only: no receipt
 * means no proof the host observed the cancel, so the state machine is left
 * untouched.
 */

import { aiCommands } from '../commands/ai';
import type { CancelReceipt } from './targetClaim';
import type { StreamRegistry } from './streamRegistry';

/** How long the UI waits for the terminal chunk after a `marked` cancel. */
export const CANCEL_TIMEOUT_MS = 8_000;

type TimerHandle = ReturnType<typeof setTimeout>;

export interface SessionCancelDeps {
  registry: StreamRegistry;
  /** Injectable for tests; defaults to `aiCommands.cancel`. */
  cancel?: (requestId: string) => Promise<CancelReceipt>;
  timeoutMs?: number;
  setTimeout?: (callback: () => void, delayMs: number) => TimerHandle;
  clearTimeout?: (handle: TimerHandle) => void;
  /** Called only when the timeout forced a terminal state. */
  onCancelTimeout?: (requestId: string) => void;
  onWarn?: (message: string, error?: unknown) => void;
}

export class SessionCanceller {
  private readonly registry: StreamRegistry;
  private readonly cancelFn: (requestId: string) => Promise<CancelReceipt>;
  private readonly timeoutMs: number;
  private readonly setTimeoutFn: (callback: () => void, delayMs: number) => TimerHandle;
  private readonly clearTimeoutFn: (handle: TimerHandle) => void;
  private readonly onWarn: (message: string, error?: unknown) => void;
  private readonly timers = new Map<string, TimerHandle>();
  private readonly unsubscribe: () => void;

  constructor(private readonly deps: SessionCancelDeps) {
    this.registry = deps.registry;
    this.cancelFn = deps.cancel ?? ((requestId) => aiCommands.cancel(requestId));
    this.timeoutMs = deps.timeoutMs ?? CANCEL_TIMEOUT_MS;
    this.setTimeoutFn = deps.setTimeout ?? ((callback, delayMs) => setTimeout(callback, delayMs));
    this.clearTimeoutFn = deps.clearTimeout ?? ((handle) => clearTimeout(handle));
    this.onWarn = deps.onWarn ?? ((message, error) => console.warn(`[AI] ${message}`, error));
    // Any terminal transition (normal chunk, error, or forced settle) clears
    // the fallback timer for that request.
    this.unsubscribe = this.registry.subscribe((event) => {
      if (event.type === 'settled') this.clearTimer(event.entry.requestId);
    });
  }

  /**
   * Ask the host to cancel `requestId` and translate the receipt into the
   * stream state machine. Returns the receipt, or `null` when the IPC call
   * failed (the run keeps streaming).
   */
  async cancel(requestId: string): Promise<CancelReceipt | null> {
    let receipt: CancelReceipt;
    try {
      receipt = await this.cancelFn(requestId);
    } catch (error) {
      this.onWarn(`ai_cancel failed for request ${requestId}`, error);
      return null;
    }

    switch (receipt.kind) {
      case 'forbidden':
        this.onWarn(`ai_cancel forbidden for request ${requestId}`);
        return receipt;
      case 'alreadyFinished':
        return receipt;
      case 'marked':
        if (this.registry.markCancelling(requestId)) this.armFallback(requestId);
        return receipt;
    }
  }

  /** Clear all fallback timers and detach from the registry. */
  dispose(): void {
    for (const handle of this.timers.values()) this.clearTimeoutFn(handle);
    this.timers.clear();
    this.unsubscribe();
  }

  private armFallback(requestId: string): void {
    this.clearTimer(requestId);
    const handle = this.setTimeoutFn(() => {
      this.timers.delete(requestId);
      const entry = this.registry.get(requestId);
      if (!entry || entry.phase === 'terminal') return;
      this.registry.settle(requestId, 'cancelled');
      this.deps.onCancelTimeout?.(requestId);
    }, this.timeoutMs);
    this.timers.set(requestId, handle);
  }

  private clearTimer(requestId: string): void {
    const handle = this.timers.get(requestId);
    if (handle === undefined) return;
    this.clearTimeoutFn(handle);
    this.timers.delete(requestId);
  }
}
