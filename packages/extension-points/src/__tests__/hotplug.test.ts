import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
  createExtensionPoint,
  ExtensionRegistry,
  extensionRegistry,
  type ExtensionPoint,
} from '../extensionPoints';
import {
  HostExtensionLoader,
  registerExtensionPoint,
  type ExtensionContext,
  type ExtensionModule,
} from '../lifecycle';
import { SafeCompartmentWrapper } from '../safeCompartment';
import { sqlEditorEnhancedEP, sqlEditorProEP } from '../sqlEditorEnhancedEP';

interface DemoFeatures {
  label: string;
}

const fallback: DemoFeatures = { label: 'core' };
const enhanced: DemoFeatures = { label: 'pro' };

const demoPoint = createExtensionPoint<DemoFeatures>({
  id: 'demo.hotplug',
  name: 'Demo Hotplug',
  getDefault: () => fallback,
});

describe('EP hot-plug lifecycle (hotplug.test.ts)', () => {
  let registry: ExtensionRegistry;
  let loader: HostExtensionLoader;

  beforeEach(() => {
    registry = new ExtensionRegistry();
    loader = new HostExtensionLoader();
    extensionRegistry.reset();
  });

  afterEach(async () => {
    await loader.reset();
    extensionRegistry.reset();
  });

  it('register → get → unregister falls back to default with referential stability', () => {
    expect(registry.get(demoPoint)).toBe(fallback);

    const unsub = registry.register(demoPoint, enhanced);
    expect(registry.get(demoPoint)).toBe(enhanced);
    expect(registry.isEnhanced(demoPoint)).toBe(true);

    unsub();
    expect(registry.get(demoPoint)).toBe(fallback);
    expect(registry.isEnhanced(demoPoint)).toBe(false);
  });

  it('subscribe notifies on register and unregister; unsub stops further notifications', () => {
    const listener = vi.fn();
    const unsubListener = registry.subscribe(demoPoint.id, listener);

    registry.register(demoPoint, enhanced);
    expect(listener).toHaveBeenCalledTimes(1);

    registry.unregister(demoPoint);
    expect(listener).toHaveBeenCalledTimes(2);

    unsubListener();
    registry.register(demoPoint, enhanced);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it('subscribePoint mirrors subscribe by extension point object', () => {
    const listener = vi.fn();
    const unsub = registry.subscribePoint(demoPoint, listener);

    registry.register(demoPoint, enhanced);
    expect(listener).toHaveBeenCalledTimes(1);

    unsub();
    registry.unregister(demoPoint);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('HostExtensionLoader disposes subscriptions in strict LIFO order on unload', async () => {
    const order: string[] = [];

    const module: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => order.push('first'));
        ctx.subscriptions.push(() => order.push('second'));
        ctx.subscriptions.push({ dispose: () => order.push('third') });
      },
    };

    await loader.load('demo-ext', module);
    expect(loader.isLoaded('demo-ext')).toBe(true);

    await loader.unload('demo-ext');
    expect(order).toEqual(['third', 'second', 'first']);
    expect(loader.isLoaded('demo-ext')).toBe(false);
  });

  it('registerExtensionPoint tracks registry unsubscribe on context for auto cleanup', async () => {
    const module: ExtensionModule = {
      activate(ctx) {
        registerExtensionPoint(ctx, demoPoint, enhanced);
      },
    };

    await loader.load('ctx-ext', module);
    expect(extensionRegistry.isEnhanced(demoPoint)).toBe(true);

    await loader.unload('ctx-ext');
    expect(extensionRegistry.isEnhanced(demoPoint)).toBe(false);
    expect(extensionRegistry.get(demoPoint).label).toBe('core');
  });

  it('HostExtensionLoader reload replaces prior instance and cleans up old subscriptions', async () => {
    let deactivateCalls = 0;
    const disposeOrder: string[] = [];

    const mkModule = (tag: string): ExtensionModule => ({
      activate(ctx) {
        ctx.subscriptions.push(() => disposeOrder.push(`${tag}-sub`));
      },
      deactivate() {
        deactivateCalls += 1;
      },
    });

    await loader.load('reload-ext', mkModule('v1'));
    await loader.load('reload-ext', mkModule('v2'));

    expect(deactivateCalls).toBe(1);
    expect(disposeOrder).toEqual(['v1-sub']);

    await loader.unload('reload-ext');
    expect(disposeOrder).toEqual(['v1-sub', 'v2-sub']);
  });

  it('SafeCompartmentWrapper circuit-breaks on decorator crash and unregisters EP', () => {
    const crashingPoint = createExtensionPoint<{ boom(): string[] }>({
      id: 'demo.crash',
      name: 'Crash Demo',
      getDefault: () => ({ boom: () => [] }),
    });

    extensionRegistry.register(crashingPoint, {
      boom: () => {
        throw new Error('decorator exploded');
      },
    });

    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    const result = SafeCompartmentWrapper(
      { point: crashingPoint, featureName: 'boom' },
      () => extensionRegistry.get(crashingPoint).boom(),
      [] as string[],
    );

    expect(result).toEqual([]);
    expect(extensionRegistry.isEnhanced(crashingPoint)).toBe(false);
    expect(consoleSpy).toHaveBeenCalled();

    consoleSpy.mockRestore();
  });

  it('SafeCompartmentWrapper passes through successful decorator results', () => {
    const okPoint = createExtensionPoint<{ value(): number }>({
      id: 'demo.ok',
      name: 'OK Demo',
      getDefault: () => ({ value: () => 0 }),
    });

    extensionRegistry.register(okPoint, { value: () => 42 });

    const result = SafeCompartmentWrapper(
      { point: okPoint, featureName: 'value' },
      () => extensionRegistry.get(okPoint).value(),
      0,
    );

    expect(result).toBe(42);
    extensionRegistry.unregister(okPoint);
  });

  it('[tester] getContext returns active context or undefined', async () => {
    expect(loader.getContext('ctx-ext')).toBeUndefined();

    const module: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => {});
      },
    };
    await loader.load('ctx-ext', module);

    expect(loader.getContext('ctx-ext')?.extensionId).toBe('ctx-ext');
    await loader.unload('ctx-ext');
    expect(loader.getContext('ctx-ext')).toBeUndefined();
  });

  it('[tester] reset unloads all active extensions', async () => {
    const module: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => {});
      },
    };
    await loader.load('ext-a', module);
    await loader.load('ext-b', module);
    expect(loader.isLoaded('ext-a')).toBe(true);
    expect(loader.isLoaded('ext-b')).toBe(true);

    await loader.reset();
    expect(loader.isLoaded('ext-a')).toBe(false);
    expect(loader.isLoaded('ext-b')).toBe(false);
  });

  it('[tester] unload tolerates dispose errors and still removes extension', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const module: ExtensionModule = {
      activate(ctx) {
        ctx.subscriptions.push(() => {
          throw new Error('dispose failed');
        });
      },
    };

    await loader.load('fragile-ext', module);
    await loader.unload('fragile-ext');

    expect(loader.isLoaded('fragile-ext')).toBe(false);
    expect(consoleSpy).toHaveBeenCalled();
    consoleSpy.mockRestore();
  });

  it('[tester] unregister is no-op when extension point is not registered', () => {
    expect(() => registry.unregister(demoPoint)).not.toThrow();
    expect(registry.isEnhanced(demoPoint)).toBe(false);
  });

  it('[tester] SafeCompartmentWrapper invokes onCircuitBreak callback', () => {
    const crashingPoint = createExtensionPoint<{ boom(): string[] }>({
      id: 'demo.circuit-cb',
      name: 'Circuit CB Demo',
      getDefault: () => ({ boom: () => [] }),
    });

    extensionRegistry.register(crashingPoint, {
      boom: () => {
        throw new Error('boom');
      },
    });

    const onCircuitBreak = vi.fn();
    vi.spyOn(console, 'error').mockImplementation(() => {});

    SafeCompartmentWrapper(
      { point: crashingPoint, featureName: 'boom', onCircuitBreak },
      () => extensionRegistry.get(crashingPoint).boom(),
      [] as string[],
    );

    expect(onCircuitBreak).toHaveBeenCalledTimes(1);
    vi.restoreAllMocks();
  });

  it('sqlEditorEnhancedEP fallback is used after hot-unregister of enhanced implementation', () => {
    const enhancedImpl = {
      createStatementDecorations: () => [{ tag: 'enhanced-marker' }],
    };

    const unsub = extensionRegistry.register(sqlEditorEnhancedEP, enhancedImpl);
    expect(extensionRegistry.isEnhanced(sqlEditorEnhancedEP)).toBe(true);
    // Legacy alias also reflects enhanced state
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(true);
    expect(extensionRegistry.get(sqlEditorEnhancedEP).createStatementDecorations?.()).toEqual([
      { tag: 'enhanced-marker' },
    ]);

    unsub();
    const fallbackImpl = extensionRegistry.get(sqlEditorEnhancedEP);
    expect(extensionRegistry.isEnhanced(sqlEditorEnhancedEP)).toBe(false);
    expect(fallbackImpl.createStatementDecorations?.()).toEqual([]);
  });
});
