import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { HostExtensionLoader, type ExtensionModule } from '../lifecycle';
import {
  ValuePresentationRegistryImpl,
  registerValuePresentation,
  type ValuePresentationContribution,
} from '../valuePresentation';

/**
 * BUG-001 regression suite: same-id `load()`/`unload()` interleavings.
 *
 * The bug: `load()`'s 「has() check → await activate → set entry」 was not
 * atomic, so two overlapping loads of one id both passed the check and the
 * later finisher overwrote the other's entry, leaking its subscriptions.
 * The fix serializes every same-id operation in a per-id FIFO queue, so all
 * interleavings below must be equivalent to the same calls issued serially
 * in call order.
 */

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

/**
 * Yield past the microtask queue (one macrotask), so an implementation that
 * wrongly lets a queued operation run during a gated predecessor would have
 * had the chance to run it by the time the caller asserts.
 */
function flushAsync(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function makeContribution(id: string): ValuePresentationContribution {
  return {
    id,
    configVersion: 1,
    validateConfig: () => ({ ok: true, config: null }),
    prepare: () => ({ present: () => ({ kind: 'unchanged' }) }),
  };
}

describe('HostExtensionLoader same-id concurrency (BUG-001)', () => {
  let loader: HostExtensionLoader;

  beforeEach(() => {
    loader = new HostExtensionLoader();
  });

  afterEach(async () => {
    await loader.reset();
  });

  it('concurrent loads of one id serialize: the later load reloads over the earlier instance', async () => {
    const disposed: string[] = [];
    const deactivations: string[] = [];
    const gateA = deferred();
    const gateB = deferred();
    let bActivated = false;

    const moduleA: ExtensionModule = {
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push('A'));
        await gateA.promise;
      },
      deactivate() {
        deactivations.push('A');
      },
    };
    const moduleB: ExtensionModule = {
      async activate(ctx) {
        bActivated = true;
        ctx.subscriptions.push(() => disposed.push('B'));
        await gateB.promise;
      },
      deactivate() {
        deactivations.push('B');
      },
    };

    // Same call order and gate order as the bug report (B's gate opens
    // first). The report's literal `await loadB` before resolving gateA is
    // exactly the interleaving this fix removes — under the queue, loadB
    // waits for loadA, so this test resolves gateA instead of deadlocking.
    const loadA = loader.load('race-ext', moduleA); // starts activating now
    const loadB = loader.load('race-ext', moduleB); // queued behind loadA
    gateB.resolve();

    try {
      await flushAsync();
      // B must not have started activating while A is still in flight, even
      // though B's own gate is already open.
      expect(bActivated).toBe(false);
      expect(loader.isLoaded('race-ext')).toBe(false);
    } finally {
      gateA.resolve();
    }

    await loadA;
    await loadB;

    // Serial-reload equivalence: B's load deactivated and LIFO-disposed A's
    // instance, then recorded B. Nothing was overwritten, nothing leaked.
    expect(deactivations).toEqual(['A']);
    expect(disposed).toEqual(['A']);
    expect(loader.isLoaded('race-ext')).toBe(true);

    await loader.unload('race-ext');
    expect(deactivations).toEqual(['A', 'B']);
    expect(disposed).toEqual(['A', 'B']);
    expect(loader.isLoaded('race-ext')).toBe(false);
  });

  it('concurrent reload releases the valuePresentation contribution of the replaced instance', async () => {
    const registry = new ValuePresentationRegistryImpl();
    const gate = deferred();
    const contributionA = makeContribution('demo.race');
    const contributionB = makeContribution('demo.race');

    const moduleA: ExtensionModule = {
      async activate(ctx) {
        registerValuePresentation(ctx, contributionA, registry);
        await gate.promise;
      },
    };
    const moduleB: ExtensionModule = {
      activate(ctx) {
        registerValuePresentation(ctx, contributionB, registry);
      },
    };

    const loadA = loader.load('vp-ext', moduleA);
    const loadB = loader.load('vp-ext', moduleB); // queued behind A
    gate.resolve();
    await loadA;
    expect(registry.get('demo.race')).toBe(contributionA);

    await loadB;
    // A's release handle ran during the reload, so B's registration found a
    // free id instead of colliding with a leaked contribution.
    expect(registry.get('demo.race')).toBe(contributionB);

    await loader.unload('vp-ext');
    expect(registry.get('demo.race')).toBeUndefined();
  });

  it('three concurrent loads of one id complete in call order; each reload releases its predecessor', async () => {
    const disposed: string[] = [];
    const gate = deferred();
    const mk = (tag: string): ExtensionModule => ({
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push(tag));
        await gate.promise;
      },
    });

    const loads = [
      loader.load('fifo-ext', mk('one')),
      loader.load('fifo-ext', mk('two')),
      loader.load('fifo-ext', mk('three')),
    ];
    gate.resolve();
    await Promise.all(loads);

    // Instances one and two were reloaded away by their successors.
    expect(disposed).toEqual(['one', 'two']);
    expect(loader.isLoaded('fifo-ext')).toBe(true);

    await loader.unload('fifo-ext');
    expect(disposed).toEqual(['one', 'two', 'three']);
    expect(loader.isLoaded('fifo-ext')).toBe(false);
  });

  it('unload during an in-flight load waits for it, then unloads the recorded instance', async () => {
    const disposed: string[] = [];
    const gate = deferred();
    const module: ExtensionModule = {
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push('X'));
        await gate.promise;
      },
    };

    const loading = loader.load('wait-ext', module);
    const unloading = loader.unload('wait-ext'); // queued behind the load
    try {
      await flushAsync();
      expect(loader.isLoaded('wait-ext')).toBe(false); // activating, not recorded
    } finally {
      gate.resolve();
    }

    await loading;
    expect(loader.isLoaded('wait-ext')).toBe(true);

    await unloading;
    // Without the wait, this unload would have been a no-op during the
    // activation window and the id would have ended up loaded.
    expect(loader.isLoaded('wait-ext')).toBe(false);
    expect(disposed).toEqual(['X']);
  });

  it('interleaved load/unload/load on one id run in call order with no leak', async () => {
    const disposed: string[] = [];
    const gate = deferred();
    const mk = (tag: string, gated: boolean): ExtensionModule => ({
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push(tag));
        if (gated) await gate.promise;
      },
    });

    const loadA = loader.load('mix-ext', mk('first', true)); // gated
    const unloading = loader.unload('mix-ext'); // queued: runs after A records
    const loadB = loader.load('mix-ext', mk('second', false)); // queued: after the unload
    gate.resolve();

    await loadA;
    await unloading;
    // The waiting unload removed A's instance before B's load ran.
    expect(disposed).toEqual(['first']);

    await loadB;
    expect(loader.isLoaded('mix-ext')).toBe(true);

    await loader.unload('mix-ext');
    expect(disposed).toEqual(['first', 'second']);
    expect(loader.isLoaded('mix-ext')).toBe(false);
  });

  it('a load whose activation rejects is cleaned up and does not fail or block the loads queued behind it', async () => {
    const disposed: string[] = [];
    const gate = deferred();

    const failing: ExtensionModule = {
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push('fail'));
        await gate.promise;
        throw new Error('activate exploded');
      },
    };
    const good: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => disposed.push('good'));
      },
    };

    const loadFailing = loader.load('chain-ext', failing);
    const loadGood = loader.load('chain-ext', good); // queued behind the failing load
    gate.resolve();

    await expect(loadFailing).rejects.toThrow('activate exploded');
    expect(disposed).toEqual(['fail']); // failed-activation cleanup (issue-32 §3) ran

    await loadGood; // the predecessor's rejection did not poison the queue
    expect(loader.isLoaded('chain-ext')).toBe(true);

    await loader.unload('chain-ext');
    expect(disposed).toEqual(['fail', 'good']);
    expect(loader.isLoaded('chain-ext')).toBe(false);
  });

  it('reset() drains an in-flight load and unloads the instance it records', async () => {
    const disposed: string[] = [];
    const gate = deferred();
    const module: ExtensionModule = {
      async activate(ctx) {
        ctx.subscriptions.push(() => disposed.push('X'));
        await gate.promise;
      },
    };

    const loading = loader.load('drain-ext', module); // fire and forget
    const resetting = loader.reset();
    try {
      await flushAsync();
      expect(loader.isLoaded('drain-ext')).toBe(false);
    } finally {
      gate.resolve();
    }

    await loading;
    await resetting;
    // Without the drain, the load would have recorded its entry after the
    // reset pass and survived it.
    expect(loader.isLoaded('drain-ext')).toBe(false);
    expect(disposed).toEqual(['X']);
  });

  it('loads of different ids do not wait for each other', async () => {
    const gateX = deferred();
    let yActivated = false;

    const loadX = loader.load('x-ext', {
      async activate() {
        await gateX.promise;
      },
    });
    const loadY = loader.load('y-ext', {
      activate() {
        yActivated = true;
      },
    });

    try {
      await loadY;
      expect(yActivated).toBe(true);
      // X is still activating; Y's load was not blocked behind it.
      expect(loader.isLoaded('x-ext')).toBe(false);
    } finally {
      gateX.resolve();
    }

    await loadX;
    expect(loader.isLoaded('x-ext')).toBe(true);
    expect(loader.isLoaded('y-ext')).toBe(true);
  });

  it('serial paths unchanged: reload deactivates + LIFO-disposes; a failed reload leaves the id unloaded', async () => {
    const events: string[] = [];

    const v1: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => events.push('v1-sub-1'));
        ctx.subscriptions.push({ dispose: () => events.push('v1-sub-2') });
      },
      deactivate() {
        events.push('v1-deactivate');
      },
    };
    await loader.load('serial-ext', v1);

    const v2: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => events.push('v2-sub'));
      },
    };
    await loader.load('serial-ext', v2); // serial reload
    expect(events).toEqual(['v1-deactivate', 'v1-sub-2', 'v1-sub-1']);

    const failing: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => events.push('failing-sub'));
        throw new Error('boom');
      },
    };
    await expect(loader.load('serial-ext', failing)).rejects.toThrow('boom');
    // The reload attempt unloaded v2, and the failed round cleaned up its own
    // subscription — the id is left unloaded, exactly like a serial reload
    // whose activation failed.
    expect(events).toEqual(['v1-deactivate', 'v1-sub-2', 'v1-sub-1', 'v2-sub', 'failing-sub']);
    expect(loader.isLoaded('serial-ext')).toBe(false);

    // Unloading an id with no recorded instance stays a silent no-op.
    await expect(loader.unload('serial-ext')).resolves.toBeUndefined();
    await expect(loader.unload('never-loaded')).resolves.toBeUndefined();
  });

  it("BUG-002: a load re-entered from the synchronous prefix of the same id's activate queues behind it", async () => {
    // Exact shape of the bug report: the outer module calls loader.load with
    // the SAME id from inside its own synchronous (await-free) activate. With
    // the BUG-001 enqueue shape (map set after the operation body started),
    // the re-entrant load read no queue tail and ran concurrently: inner
    // recorded first, outer overwrote it, and inner's subscriptions leaked
    // (order printed ['outer-sub'] only).
    const order: string[] = [];
    let inner!: Promise<void>;

    const outer: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => order.push('outer-sub'));
        inner = loader.load('win', {
          activate(ctx2) {
            ctx2.subscriptions.push(() => order.push('inner-sub'));
          },
        });
      },
    };

    const outerLoad = loader.load('win', outer);
    // The re-entrant load must not settle before the outer load has fully
    // recorded its instance (it is queued behind it).
    expect(loader.isLoaded('win')).toBe(false);

    await outerLoad;
    expect(loader.isLoaded('win')).toBe(true);
    expect(order).toEqual([]); // outer's subscription is not disposed yet

    await inner;
    // Serial-reload equivalence: the queued inner load deactivated +
    // LIFO-disposed the outer instance, then recorded its own.
    expect(order).toEqual(['outer-sub']);
    expect(loader.getContext('win')?.subscriptions.length).toBe(1);

    await loader.unload('win');
    expect(order).toEqual(['outer-sub', 'inner-sub']);
    expect(loader.isLoaded('win')).toBe(false);
  });

  it('BUG-002: re-entrant load from an async activate (before its first await) and re-entrant unload both serialize', async () => {
    const order: string[] = [];
    let innerLoad!: Promise<void>;
    let innerUnload!: Promise<void>;

    // Re-entry from the synchronous prefix of an ASYNC activate (C3 shape).
    // The re-entrant calls are deliberately NOT awaited from inside activate:
    // the queue runs them only after this load's operation settles, so an
    // `await innerLoad` inside activate would be a self-deadlock by
    // construction — activate would wait for a load that is waiting for
    // activate. Re-entrant same-id calls are fire-and-forget from the
    // activating module's point of view.
    const outer: ExtensionModule = {
      async activate(ctx) {
        ctx.subscriptions.push(() => order.push('outer-sub'));
        innerLoad = loader.load('win', {
          activate(ctx2) {
            ctx2.subscriptions.push(() => order.push('inner-sub'));
          },
        });
        // Re-entrant unload from the same window (C4 shape): with the bug it
        // ran immediately as a pre-record no-op; it must instead queue and,
        // once outer records its instance, unload it — so innerLoad behind it
        // re-activates into a free id instead of colliding.
        innerUnload = loader.unload('win');
      },
    };

    await loader.load('win', outer);
    await innerLoad;
    await innerUnload;

    // Outer recorded → queued innerLoad reloaded over it (disposed
    // 'outer-sub', recorded inner) → queued innerUnload disposed inner. With
    // the bug this ended as ['outer-sub'] with inner leaked and the id still
    // loaded; both subscriptions must be released here.
    expect(order).toEqual(['outer-sub', 'inner-sub']);
    expect(loader.isLoaded('win')).toBe(false);
    expect(loader.getContext('win')).toBeUndefined();
  });
});
