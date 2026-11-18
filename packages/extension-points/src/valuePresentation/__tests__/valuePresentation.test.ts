import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  valuePresentationRegistry,
  type ConfigValidationResult,
  type PresentationPrepareInput,
  type PresentationValue,
  type PreparedPresentation,
  type ValuePresentationContribution,
  type ValuePresentationRegistry,
} from '../index';
import { HostExtensionLoader, type ExtensionModule } from '../../lifecycle';
import { registerValuePresentation } from '../lifecycle';

let contributionSeq = 0;

/** Minimal contribution stub; each call produces a distinct object. */
function makeContribution(id: string): ValuePresentationContribution {
  const tag = `${id}#${++contributionSeq}`;
  return {
    id,
    configVersion: 1,
    validateConfig(): ConfigValidationResult {
      return { ok: true, config: { stub: tag } };
    },
    prepare(): PreparedPresentation {
      return { present: () => ({ kind: 'unchanged', reason: `stub:${tag}` }) };
    },
  };
}

describe('ValuePresentationRegistry (collection registry)', () => {
  let registry: ValuePresentationRegistryImpl;

  beforeEach(() => {
    registry = new ValuePresentationRegistryImpl();
  });

  it('register/get/list basic read-write', () => {
    expect(registry.get('missing')).toBeUndefined();
    expect(registry.list()).toEqual([]);

    const a = makeContribution('demo.a');
    const b = makeContribution('demo.b');
    const releaseA = registry.register(a);
    const releaseB = registry.register(b);

    expect(registry.get('demo.a')).toBe(a);
    expect(registry.get('demo.b')).toBe(b);
    // Insertion order is preserved.
    expect(registry.list()).toEqual([a, b]);

    releaseA();
    expect(registry.get('demo.a')).toBeUndefined();
    expect(registry.list()).toEqual([b]);

    releaseB();
    expect(registry.list()).toEqual([]);
  });

  it('list() returns an isolated snapshot that cannot tamper with the registry', () => {
    const a = makeContribution('demo.a');
    registry.register(a);

    const first = registry.list();
    const second = registry.list();
    // A fresh array per call, not a shared reference to internal state.
    expect(first).not.toBe(second);
    expect(first).toEqual(second);

    // Mutating a returned snapshot leaves the registry untouched.
    (first as ValuePresentationContribution[]).pop();
    (first as ValuePresentationContribution[]).push(makeContribution('demo.evil'));
    expect(registry.list()).toEqual([a]);
    expect(registry.get('demo.a')).toBe(a);
  });

  it('rejects duplicate ids: keeps the original, no revision bump, no notification', () => {
    const original = makeContribution('demo.dup');
    registry.register(original);
    const revisionBefore = registry.revision;
    const listener = vi.fn();
    registry.subscribe(listener);

    const duplicate = makeContribution('demo.dup');
    expect(() => registry.register(duplicate)).toThrow(/demo\.dup/);
    expect(() => registry.register(duplicate)).toThrow(/already registered/i);

    expect(registry.get('demo.dup')).toBe(original);
    expect(registry.list()).toEqual([original]);
    expect(registry.revision).toBe(revisionBefore);
    expect(listener).not.toHaveBeenCalled();
  });

  it('unregister(id) removes and notifies; unknown id is a silent no-op', () => {
    const a = makeContribution('demo.a');
    registry.register(a);
    const listener = vi.fn();
    registry.subscribe(listener);
    const revisionBefore = registry.revision;

    registry.unregister('demo.a');
    expect(registry.get('demo.a')).toBeUndefined();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.revision).toBe(revisionBefore + 1);

    // Unknown id (including an already-unregistered one): no state change,
    // no notification, no revision bump.
    registry.unregister('demo.a');
    registry.unregister('never-registered');
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.revision).toBe(revisionBefore + 1);
  });

  it('release handle is bound to its own registration instance', () => {
    const first = makeContribution('demo.handle');
    const h1 = registry.register(first);
    expect(registry.get('demo.handle')).toBe(first);

    h1();
    expect(registry.get('demo.handle')).toBeUndefined();

    const second = makeContribution('demo.handle');
    const h2 = registry.register(second);
    expect(registry.get('demo.handle')).toBe(second);

    const revisionBefore = registry.revision;
    // Stale handle from the first registration: must be a no-op and must not
    // remove the re-registered instance.
    h1();
    expect(registry.get('demo.handle')).toBe(second);
    expect(registry.revision).toBe(revisionBefore);

    h2();
    expect(registry.get('demo.handle')).toBeUndefined();
    expect(registry.revision).toBe(revisionBefore + 1);
  });

  it('a stale handle cannot remove a re-registered contribution even when it is the same object', () => {
    const contribution = makeContribution('demo.same-object');
    const stale = registry.register(contribution);
    stale();

    // Same object, second registration: still a new registration the old
    // handle has no authority over.
    const fresh = registry.register(contribution);
    stale();
    expect(registry.get('demo.same-object')).toBe(contribution);

    fresh();
    expect(registry.get('demo.same-object')).toBeUndefined();
  });

  it('subscribe is notified on register/unregister and can unsubscribe', () => {
    const listener = vi.fn();
    const unsubscribe = registry.subscribe(listener);
    expect(listener).not.toHaveBeenCalled();

    const a = makeContribution('demo.a');
    registry.register(a);
    expect(listener).toHaveBeenCalledTimes(1);

    registry.unregister('demo.a');
    expect(listener).toHaveBeenCalledTimes(2);

    const release = registry.register(a);
    expect(listener).toHaveBeenCalledTimes(3);

    release();
    expect(listener).toHaveBeenCalledTimes(4);

    unsubscribe();
    registry.register(makeContribution('demo.b'));
    expect(listener).toHaveBeenCalledTimes(4);
  });

  it('listener exceptions do not break other listeners or the registration flow', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const badListener = vi.fn(() => {
      throw new Error('listener exploded');
    });
    const goodListener = vi.fn();
    registry.subscribe(badListener);
    registry.subscribe(goodListener);

    const a = makeContribution('demo.a');
    expect(() => registry.register(a)).not.toThrow();
    expect(registry.get('demo.a')).toBe(a);
    expect(goodListener).toHaveBeenCalledTimes(1);
    expect(consoleSpy).toHaveBeenCalledTimes(1);

    registry.unregister('demo.a');
    expect(goodListener).toHaveBeenCalledTimes(2);
    expect(consoleSpy).toHaveBeenCalledTimes(2);
    expect(registry.get('demo.a')).toBeUndefined();

    consoleSpy.mockRestore();
  });

  it('revision increments once per successful mutation and is stable when read', () => {
    expect(registry.revision).toBe(0);
    expect(registry.revision).toBe(0);

    const a = makeContribution('demo.a');
    const b = makeContribution('demo.b');
    const releaseA = registry.register(a);
    expect(registry.revision).toBe(1);
    const releaseB = registry.register(b);
    expect(registry.revision).toBe(2);

    releaseA();
    expect(registry.revision).toBe(3);
    releaseA(); // already released: no-op
    expect(registry.revision).toBe(3);
    releaseB();
    expect(registry.revision).toBe(4);

    registry.unregister('demo.b'); // already gone: no-op
    expect(registry.revision).toBe(4);

    // Reads and subscriptions never bump the revision.
    registry.get('demo.a');
    registry.list();
    const unsubscribe = registry.subscribe(() => {});
    unsubscribe();
    expect(registry.revision).toBe(4);
  });

  it('reset() clears contributions for test isolation and notifies', () => {
    registry.register(makeContribution('demo.a'));
    registry.register(makeContribution('demo.b'));
    const listener = vi.fn();
    registry.subscribe(listener);
    const revisionBefore = registry.revision;

    registry.reset();
    expect(registry.list()).toEqual([]);
    expect(registry.get('demo.a')).toBeUndefined();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.revision).toBe(revisionBefore + 1);

    // reset on an empty registry is a no-op.
    registry.reset();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.revision).toBe(revisionBefore + 1);
  });

  it('supports a realistic contribution end to end: validateConfig → prepare → present', () => {
    const contribution: ValuePresentationContribution = {
      id: 'datazen.timestamp',
      configVersion: 2,
      validateConfig(input: unknown): ConfigValidationResult {
        if (typeof input !== 'object' || input === null) {
          return { ok: false, problems: [{ path: '$', message: 'expected an object' }] };
        }
        const format = 'format' in input ? input.format : undefined;
        if (typeof format !== 'string') {
          return { ok: false, problems: [{ path: 'format', message: 'expected a string' }] };
        }
        return { ok: true, config: { format } };
      },
      prepare(input: PresentationPrepareInput): PreparedPresentation {
        return {
          present(value: PresentationValue) {
            if (typeof value === 'number' && Number.isSafeInteger(value)) {
              return {
                kind: 'text',
                text: String(value),
                interpretation: `v${input.config.configVersion}`,
              };
            }
            return { kind: 'unchanged', reason: 'unsupported value' };
          },
        };
      },
    };

    registry.register(contribution);
    expect(registry.get('datazen.timestamp')).toBe(contribution);

    expect(contribution.validateConfig({ format: 'iso' })).toEqual({
      ok: true,
      config: { format: 'iso' },
    });
    expect(contribution.validateConfig(42)).toEqual({
      ok: false,
      problems: [{ path: '$', message: 'expected an object' }],
    });
    expect(contribution.validateConfig({ format: 7 })).toEqual({
      ok: false,
      problems: [{ path: 'format', message: 'expected a string' }],
    });

    const prepared = contribution.prepare({
      config: { configVersion: 2, config: { format: 'iso' } },
      columnOverride: {
        mode: 'custom',
        rendererId: 'datazen.timestamp',
        options: { format: 'iso' },
      },
      column: { columnName: 'created_at', semanticKind: 'instant' },
      environment: { locale: 'zh-CN', localTimeZone: 'Asia/Shanghai' },
    });
    expect(prepared.present(1700000000)).toEqual({
      kind: 'text',
      text: '1700000000',
      interpretation: 'v2',
    });
    expect(prepared.present('not-a-number')).toEqual({
      kind: 'unchanged',
      reason: 'unsupported value',
    });
    expect(prepared.present(null)).toEqual({
      kind: 'unchanged',
      reason: 'unsupported value',
    });
  });
});

describe('valuePresentationRegistry singleton', () => {
  it('is a ValuePresentationRegistryImpl instance, independent of fresh instances', () => {
    expect(valuePresentationRegistry).toBeInstanceOf(ValuePresentationRegistryImpl);

    const isolated: ValuePresentationRegistry = new ValuePresentationRegistryImpl();
    const contribution = makeContribution('demo.isolated');
    isolated.register(contribution);
    expect(isolated.get('demo.isolated')).toBe(contribution);
    expect(valuePresentationRegistry.get('demo.isolated')).toBeUndefined();
  });
});

describe('registerValuePresentation + HostExtensionLoader', () => {
  let registry: ValuePresentationRegistryImpl;
  let loader: HostExtensionLoader;

  beforeEach(() => {
    registry = new ValuePresentationRegistryImpl();
    loader = new HostExtensionLoader();
  });

  afterEach(async () => {
    await loader.reset();
    valuePresentationRegistry.reset();
  });

  it('releases the contribution on unload (injected registry)', async () => {
    const contribution = makeContribution('demo.present');
    const module: ExtensionModule = {
      activate(ctx) {
        registerValuePresentation(ctx, contribution, registry);
      },
    };

    await loader.load('demo-ext', module);
    expect(registry.get('demo.present')).toBe(contribution);
    expect(registry.list()).toEqual([contribution]);
    expect(loader.getContext('demo-ext')?.subscriptions.length).toBe(1);

    await loader.unload('demo-ext');
    expect(registry.get('demo.present')).toBeUndefined();
    expect(registry.list()).toEqual([]);
    expect(loader.getContext('demo-ext')).toBeUndefined();
  });

  it('defaults to the global singleton when no registry is passed', async () => {
    const contribution = makeContribution('demo.singleton');
    const module: ExtensionModule = {
      activate(ctx) {
        registerValuePresentation(ctx, contribution);
      },
    };

    await loader.load('singleton-ext', module);
    expect(valuePresentationRegistry.get('demo.singleton')).toBe(contribution);

    await loader.unload('singleton-ext');
    expect(valuePresentationRegistry.get('demo.singleton')).toBeUndefined();
  });

  it('activation failure rejects the load, cleans the contribution, and allows re-load', async () => {
    const contribution = makeContribution('demo.present');
    const failing: ExtensionModule = {
      activate(ctx) {
        registerValuePresentation(ctx, contribution, registry);
        throw new Error('activate exploded');
      },
    };

    await expect(loader.load('failing-ext', failing)).rejects.toThrow('activate exploded');
    expect(loader.isLoaded('failing-ext')).toBe(false);
    // No half-activated residue: the contribution registered before the
    // throw has been released again.
    expect(registry.get('demo.present')).toBeUndefined();
    // register (+1) followed by the cleanup unregister (+1).
    expect(registry.revision).toBe(2);

    const replacement = makeContribution('demo.present');
    const good: ExtensionModule = {
      activate(ctx) {
        registerValuePresentation(ctx, replacement, registry);
      },
    };
    await loader.load('failing-ext', good);
    expect(loader.isLoaded('failing-ext')).toBe(true);
    expect(registry.get('demo.present')).toBe(replacement);
  });

  it('activation failure disposes the failed round of subscriptions in LIFO order', async () => {
    const order: string[] = [];
    const module: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => order.push('first'));
        registerValuePresentation(ctx, makeContribution('demo.present'), registry);
        ctx.subscriptions.push({ dispose: () => order.push('last') });
        throw new Error('boom');
      },
    };

    await expect(loader.load('lifo-ext', module)).rejects.toThrow('boom');
    expect(order).toEqual(['last', 'first']);
    expect(registry.get('demo.present')).toBeUndefined();
  });
});
