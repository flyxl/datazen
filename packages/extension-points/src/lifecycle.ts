/**
 * Extension module lifecycle contracts — activate/deactivate with safe disposal.
 *
 */

import type { ExtensionPoint } from './extensionPoints';
import { extensionRegistry } from './extensionPoints';

/** Resource that must be released when an extension module is unloaded. */
export interface Disposable {
  dispose(): void;
}

/** A subscription handle returned by register/subscribe APIs. */
export type ExtensionSubscription = (() => void) | Disposable;

/** Context passed to ExtensionModule.activate — holds lifecycle subscriptions. */
export interface ExtensionContext {
  /** Side-effect handles disposed in LIFO order on unload. */
  subscriptions: ExtensionSubscription[];
  /** Stable extension identifier (e.g. 'sql-editor-pro'). */
  extensionId: string;
}

/** Standard ESM extension module entry contract. */
export interface ExtensionModule {
  /** Activate: register extension points and wire listeners. */
  activate(context: ExtensionContext): void | Promise<void>;
  /** Optional module-level cleanup (cache pools, timers, etc.). */
  deactivate?(): void | Promise<void>;
}

function disposeSubscription(sub: ExtensionSubscription): void {
  if (typeof sub === 'function') {
    sub();
  } else {
    sub.dispose();
  }
}

/**
 * Host-controlled dynamic extension loader.
 * Tracks active modules and disposes subscriptions in strict LIFO order on unload.
 *
 * ## Same-id concurrency (BUG-001)
 *
 * `activate` is awaited, so a `load(id)` has a window in which the loader has
 * no recorded entry for that id yet. Overlapping same-id loads used to both
 * pass the `has()` precheck, and whichever settled later overwrote the other's
 * entry — the overwritten instance's `context.subscriptions` (extension-point
 * handles, valuePresentation contributions, …) became unreachable and leaked
 * with no warning.
 *
 * Every state-mutating operation on one id therefore runs inside a per-id
 * FIFO queue (an awaited promise chain). The locked-down contract:
 *
 * - Concurrent `load(id)` calls serialize in call order. Each later load
 *   performs the ordinary reload sequence — deactivate + LIFO dispose of the
 *   instance the previous load recorded — so overlapping loads are
 *   equivalent to the same calls issued serially. A load whose activation
 *   rejected neither fails nor blocks the loads queued behind it.
 * - `unload(id)` called while a load of that id is in flight waits for the
 *   load to settle, then unloads the instance it recorded (a failed load
 *   recorded nothing, so the unload is a no-op). Chosen over a documented
 *   no-op because the caller of `await unload(id)` must be able to rely on
 *   `isLoaded(id) === false` afterwards; without the wait, the in-flight load
 *   would finish later and leave the id loaded. Cost, stated plainly: a hung
 *   `activate` also hangs the operations queued behind it — the same trade
 *   the load-load serialization makes, and consistent with this file's
 *   no-timeout philosophy.
 * - `reset()` unloads every id the loader knows — active instances AND ids
 *   with an operation still in flight — by enqueuing an unload behind their
 *   queues, so a fire-and-forget load cannot record an instance after the
 *   reset pass and survive it. Operations started after `reset()` begins are
 *   not awaited. Unloads for different ids run in parallel; if any
 *   `deactivate` rejects, `reset()` still waits for all of them and then
 *   rethrows the first rejection (the old sequential loop abandoned the
 *   remaining ids on the first failure).
 * - `isLoaded`/`getContext` stay synchronous snapshots of *recorded*
 *   instances: an activation still in flight is not loaded yet.
 * - A same-id `load`/`unload`/`reset` re-entered from inside the synchronous
 *   prefix of an in-flight operation (e.g. from a module's own `activate`)
 *   queues behind that operation like any other caller (BUG-002): the queue
 *   tail is registered before the operation body starts, so there is no
 *   window in which a re-entrant same-id call can observe an unqueued loader.
 *   Consequence, stated plainly: `activate` must not await a same-id re-entrant
 *   load — the queue runs it only after this load settles, so that await is a
 *   self-deadlock by construction. Re-entrant same-id calls are
 *   fire-and-forget from the activating module's point of view.
 */
export class HostExtensionLoader {
  private activeExtensions = new Map<
    string,
    {
      context: ExtensionContext;
      deactivate?: () => void | Promise<void>;
    }
  >();

  /**
   * Per-id operation queues (BUG-001). Each value is the settled-safe tail of
   * one id's chain: it never rejects, and it is deleted once it is the settled
   * tail so idle ids accumulate nothing.
   */
  private operationQueues = new Map<string, Promise<void>>();

  /**
   * Load and activate an extension module (unloads any prior instance with
   * same id). Same-id loads serialize in call order; see class doc.
   */
  load(extensionId: string, module: ExtensionModule): Promise<void> {
    return this.enqueue(extensionId, () => this.loadExclusive(extensionId, module));
  }

  /** Dynamically import and activate an ESM bundle entry. */
  async loadFromUrl(extensionId: string, entryUrl: string): Promise<void> {
    const mod: ExtensionModule = await import(/* @vite-ignore */ entryUrl);
    await this.load(extensionId, mod);
  }

  /**
   * Unload: call deactivate, then dispose all subscriptions in LIFO order.
   * If a load of this id is still in flight, wait for it first; see class doc.
   */
  unload(extensionId: string): Promise<void> {
    return this.enqueue(extensionId, () => this.unloadExclusive(extensionId));
  }

  isLoaded(extensionId: string): boolean {
    return this.activeExtensions.has(extensionId);
  }

  getContext(extensionId: string): ExtensionContext | undefined {
    return this.activeExtensions.get(extensionId)?.context;
  }

  /** Unload all active extensions (primarily for tests). */
  async reset(): Promise<void> {
    // BUG-001: an id can have an operation in flight without an entry yet
    // (mid-activation). Enqueuing an unload behind its queue drains it and
    // removes whatever instance it ends up recording, so a fire-and-forget
    // load cannot survive the reset.
    const ids = new Set(this.activeExtensions.keys());
    for (const id of this.operationQueues.keys()) ids.add(id);
    const results = await Promise.allSettled([...ids].map((id) => this.unload(id)));
    const failure = results.find((r): r is PromiseRejectedResult => r.status === 'rejected');
    if (failure) {
      throw failure.reason;
    }
  }

  /**
   * Append `operation` to the id's FIFO queue and return its outcome.
   *
   * The placeholder tail is placed in the map BEFORE `operation()` runs
   * (BUG-002). `operation()`'s synchronous prefix — `loadExclusive`'s has()
   * check, the context allocation, and the synchronous prefix of the module's
   * own `activate` — can therefore re-enter `load`/`unload`/`reset` on this
   * same id, and those calls read this operation's tail from the map and
   * queue behind it instead of running concurrently. With the set() after
   * the operation (the BUG-001 shape) that window was observable only from
   * inside it, and a same-id re-entrant load bypassed the queue entirely.
   *
   * The stored tail is a manually-resolved placeholder rather than a
   * `.then()` of `run`: it settles only when this operation has settled
   * (`signal()`), never rejects, and so remains a settled-safe tail for
   * whatever is chained onto it. The caller still receives the raw `run`
   * promise, so a failed load rejects exactly as before. An idle id still
   * starts its first operation synchronously (`previous` is undefined →
   * `operation()` runs immediately), preserving the R12 contract.
   */
  private enqueue<T>(extensionId: string, operation: () => Promise<T>): Promise<T> {
    const previous = this.operationQueues.get(extensionId);
    let signal!: () => void;
    const tail = new Promise<void>((resolve) => {
      signal = resolve;
    });
    // Close the re-entrancy window first: from this line on, any same-id
    // operation — including one started from inside `operation()` — observes
    // `tail` as the queue end and chains onto it.
    this.operationQueues.set(extensionId, tail);
    // The two-argument then is deliberate: one load's activation error must
    // not fail the loads queued behind it. `previous` is a settled-safe tail
    // by construction, so this is defense in depth for that invariant.
    const run = previous ? previous.then(operation, operation) : operation();
    void run.then(
      () => this.settleQueueEntry(extensionId, tail, signal),
      () => this.settleQueueEntry(extensionId, tail, signal),
    );
    return run;
  }

  /**
   * Settle the placeholder tail and, if it is still the queue end, remove the
   * id's entry so idle ids accumulate nothing. A re-entrant operation
   * enqueued during this one's window has already overwritten the map with
   * its own tail; the identity check fails, the delete is skipped, and that
   * newer tail is responsible for its own removal.
   */
  private settleQueueEntry(extensionId: string, tail: Promise<void>, signal: () => void): void {
    signal();
    if (this.operationQueues.get(extensionId) === tail) {
      this.operationQueues.delete(extensionId);
    }
  }

  /**
   * load() body — already serialized per id by enqueue(). No other caller may
   * invoke it directly: it assumes exclusive access to `extensionId`'s state.
   */
  private async loadExclusive(extensionId: string, module: ExtensionModule): Promise<void> {
    if (this.activeExtensions.has(extensionId)) {
      // Raw (un-queued) unload: this body already owns the id's queue, so
      // going through the public unload() here would wait for itself and
      // deadlock.
      await this.unloadExclusive(extensionId);
    }

    const context: ExtensionContext = {
      extensionId,
      subscriptions: [],
    };

    try {
      await module.activate(context);
    } catch (err) {
      // Half-activation cleanup (issue-32 §3): activate failed before the
      // extension was recorded, so unload() will never see this context and
      // anything activate already registered would leak. Dispose what this
      // round pushed, in the same LIFO order unload() uses, then rethrow.
      // module.deactivate is deliberately NOT called: the module never
      // finished activating. The success path below is unchanged.
      const subs = context.subscriptions;
      for (let i = subs.length - 1; i >= 0; i--) {
        try {
          disposeSubscription(subs[i]);
        } catch (disposeErr) {
          console.error(
            '[ExtensionLoader] Error disposing resource after failed activation:',
            disposeErr,
          );
        }
      }
      throw err;
    }

    this.activeExtensions.set(extensionId, {
      context,
      deactivate: module.deactivate,
    });
  }

  /**
   * unload() body — already serialized per id by enqueue() (or called by
   * loadExclusive, which owns the queue). No instance recorded → no-op.
   */
  private async unloadExclusive(extensionId: string): Promise<void> {
    const active = this.activeExtensions.get(extensionId);
    if (!active) return;

    try {
      if (active.deactivate) {
        await active.deactivate();
      }
    } finally {
      const subs = active.context.subscriptions;
      for (let i = subs.length - 1; i >= 0; i--) {
        try {
          disposeSubscription(subs[i]);
        } catch (err) {
          console.error('[ExtensionLoader] Error disposing resource:', err);
        }
      }
      this.activeExtensions.delete(extensionId);
    }
  }
}

/** Global singleton extension loader. */
export const hostExtensionLoader = new HostExtensionLoader();

/**
 * Register an extension point implementation and track the unsubscribe handle
 * on the module context for automatic cleanup on unload.
 */
export function registerExtensionPoint<T>(
  context: ExtensionContext,
  point: ExtensionPoint<T>,
  impl: T,
): void {
  const unsub = extensionRegistry.register(point, impl);
  context.subscriptions.push(unsub);
}
