/**
 * Editor-session grant lifecycle for the AI chat panel (P6 §14.4).
 *
 *     independent ──grant()──▶ granting ──receipt──▶ granted
 *          ▲                       │                   │
 *          │                     denied             revoke()
 *          │                       ▼                   ▼
 *          └────local expiry──── revoked ◀─────────────┘
 *
 * The machine has all three elements on purpose:
 * - **Entry**: `grant()` is only accepted from `independent`/`revoked`;
 *   a duplicate call while `granting` joins the in-flight request.
 * - **In-state**: while `granted` a `ttl / 2` timer touches the grant; a
 *   failed touch moves the machine to `revoked`, and the local
 *   `expiresAtMs` deadline (extended by successful touches) moves it back to
 *   `independent`.
 * - **Exit**: every state has a transition out. `revoked` is not terminal —
 *   the user may grant again.
 *
 * The module owns no React and no IPC: `grant`/`revoke`/`touch` are injected,
 * so tests drive it with fake timers and plain promises.
 */

import type { AiGrantReceipt } from './targetClaim';

export type GrantPhase = 'independent' | 'granting' | 'granted' | 'revoked';

/**
 * Identifying fields of the editor session to share. The host's opaque
 * `sessionHandle` and the claimed client id are resolved by the caller inside
 * `deps.grant`, so this machine stays independent of transport details.
 */
export interface GrantInput {
  connectionId: string;
  database: string | null;
  editorSessionId: string;
}

export interface GrantSnapshot {
  phase: GrantPhase;
  receipt: AiGrantReceipt | null;
  /** Locally tracked deadline, extended when a touch succeeds. */
  expiresAtMs: number | null;
  /** Last failure, shown by the panel; cleared on a new successful grant. */
  error: string | null;
}

export type GrantTimerHandle = ReturnType<typeof setTimeout>;

export interface GrantStateDeps {
  grant: (input: GrantInput) => Promise<AiGrantReceipt>;
  revoke: (grantId: string) => Promise<boolean>;
  touch: (grantId: string) => Promise<boolean>;
  now?: () => number;
  setTimeout?: (callback: () => void, delayMs: number) => GrantTimerHandle;
  clearTimeout?: (handle: GrantTimerHandle) => void;
  /** Touch at this fraction of the remaining ttl. Defaults to 0.5. */
  touchIntervalRatio?: number;
  onWarn?: (message: string, error?: unknown) => void;
}

const INDEPENDENT: GrantSnapshot = {
  phase: 'independent',
  receipt: null,
  expiresAtMs: null,
  error: null,
};

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class GrantState {
  private current: GrantSnapshot = { ...INDEPENDENT };
  private readonly listeners = new Set<(snapshot: GrantSnapshot) => void>();
  private readonly deps: Required<
    Pick<GrantStateDeps, 'grant' | 'revoke' | 'touch' | 'now' | 'setTimeout' | 'clearTimeout'>
  > & { touchIntervalRatio: number; onWarn: (message: string, error?: unknown) => void };

  private touchTimer: GrantTimerHandle | null = null;
  private expiryTimer: GrantTimerHandle | null = null;
  private ttlMs = 0;
  /** Invalidates an in-flight grant when revoke/dispose wins the race. */
  private grantToken = 0;
  private inFlight: Promise<AiGrantReceipt | null> | null = null;
  private disposed = false;

  constructor(deps: GrantStateDeps) {
    this.deps = {
      grant: deps.grant,
      revoke: deps.revoke,
      touch: deps.touch,
      now: deps.now ?? (() => Date.now()),
      setTimeout: deps.setTimeout ?? ((callback, delayMs) => setTimeout(callback, delayMs)),
      clearTimeout: deps.clearTimeout ?? ((handle) => clearTimeout(handle)),
      touchIntervalRatio: deps.touchIntervalRatio ?? 0.5,
      onWarn: deps.onWarn ?? ((message, error) => console.warn(`[AI] ${message}`, error)),
    };
  }

  get snapshot(): GrantSnapshot {
    return this.current;
  }

  get isGranted(): boolean {
    return this.current.phase === 'granted' && this.current.receipt !== null;
  }

  subscribe(listener: (snapshot: GrantSnapshot) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Request a grant. From `granted` this returns the existing receipt; from
   * `granting` it joins the in-flight request; from `revoked` it starts over.
   * A denied grant leaves the machine `independent` with a visible error.
   */
  grant(input: GrantInput): Promise<AiGrantReceipt | null> {
    if (this.disposed) return Promise.resolve(null);
    if (this.current.phase === 'granted' && this.current.receipt) {
      return Promise.resolve(this.current.receipt);
    }
    if (this.current.phase === 'granting' && this.inFlight) return this.inFlight;

    this.clearTimers();
    const token = ++this.grantToken;
    this.update({ phase: 'granting', receipt: null, expiresAtMs: null, error: null });

    const request = this.deps
      .grant(input)
      .then((receipt) => {
        if (this.disposed || token !== this.grantToken) {
          // The panel moved on while the host was granting: release what we
          // just received instead of leaving an orphan grant behind.
          void this.revokeQuietly(receipt.grantId);
          return null;
        }
        const now = this.deps.now();
        this.ttlMs = Math.max(0, receipt.expiresAtMs - now);
        this.update({
          phase: 'granted',
          receipt,
          expiresAtMs: receipt.expiresAtMs,
          error: null,
        });
        this.scheduleTouch();
        return receipt;
      })
      .catch((error: unknown) => {
        if (token !== this.grantToken || this.disposed) return null;
        this.update({
          phase: 'independent',
          receipt: null,
          expiresAtMs: null,
          error: messageOf(error),
        });
        return null;
      })
      .finally(() => {
        if (token === this.grantToken) this.inFlight = null;
      });

    this.inFlight = request;
    return request;
  }

  /**
   * Revoke the current grant (if any) and move to `revoked`.
   *
   * Safe to call while `granting`: the in-flight response is invalidated and
   * released when it arrives.
   */
  async revoke(): Promise<void> {
    const receipt = this.current.receipt;
    this.clearTimers();
    this.grantToken += 1;
    this.inFlight = null;
    this.update({ phase: 'revoked', receipt: null, expiresAtMs: null, error: null });
    if (receipt) await this.revokeQuietly(receipt.grantId);
  }

  /**
   * Stop timers and release an active grant without emitting further state.
   * Called from the panel's effect cleanup.
   */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.clearTimers();
    this.grantToken += 1;
    this.inFlight = null;
    const receipt = this.current.receipt;
    this.listeners.clear();
    if (receipt) void this.revokeQuietly(receipt.grantId);
  }

  private async revokeQuietly(grantId: string): Promise<void> {
    try {
      await this.deps.revoke(grantId);
    } catch (error) {
      this.deps.onWarn(`failed to revoke editor grant ${grantId}`, error);
    }
  }

  private scheduleTouch(): void {
    this.clearTimers();
    const receipt = this.current.receipt;
    if (!receipt || this.current.phase !== 'granted') return;
    const remaining = Math.max(0, (this.current.expiresAtMs ?? 0) - this.deps.now());
    if (remaining <= 0) {
      this.expireLocally();
      return;
    }
    const touchDelay = Math.max(1, Math.floor(remaining * this.deps.touchIntervalRatio));
    this.touchTimer = this.deps.setTimeout(() => {
      this.touchTimer = null;
      void this.runTouch();
    }, touchDelay);
    // The local deadline is an independent exit: even a stuck touch request
    // cannot keep the UI claiming a grant past `expiresAtMs`.
    this.expiryTimer = this.deps.setTimeout(() => {
      this.expiryTimer = null;
      this.expireLocally();
    }, remaining);
  }

  private async runTouch(): Promise<void> {
    const receipt = this.current.receipt;
    if (!receipt || this.current.phase !== 'granted') return;
    if (this.deps.now() >= (this.current.expiresAtMs ?? 0)) {
      this.expireLocally();
      return;
    }
    try {
      const ok = await this.deps.touch(receipt.grantId);
      if (!ok) throw new Error('touch rejected');
      // A revoke/expiry during the await must win over the touch result.
      if (this.current.phase !== 'granted' || this.current.receipt?.grantId !== receipt.grantId) {
        return;
      }
      this.update({ expiresAtMs: this.deps.now() + this.ttlMs });
      this.scheduleTouch();
    } catch (error) {
      if (this.current.phase !== 'granted' || this.current.receipt?.grantId !== receipt.grantId) {
        return;
      }
      this.clearTimers();
      this.update({
        phase: 'revoked',
        receipt: null,
        expiresAtMs: null,
        error: messageOf(error),
      });
    }
  }

  private expireLocally(): void {
    if (this.current.phase !== 'granted') return;
    this.clearTimers();
    this.update({ phase: 'independent', receipt: null, expiresAtMs: null, error: null });
  }

  private clearTimers(): void {
    if (this.touchTimer !== null) {
      this.deps.clearTimeout(this.touchTimer);
      this.touchTimer = null;
    }
    if (this.expiryTimer !== null) {
      this.deps.clearTimeout(this.expiryTimer);
      this.expiryTimer = null;
    }
  }

  private update(patch: Partial<GrantSnapshot>): void {
    this.current = { ...this.current, ...patch };
    for (const listener of this.listeners) listener(this.current);
  }
}
