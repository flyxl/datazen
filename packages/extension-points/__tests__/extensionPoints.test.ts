import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  createExtensionPoint,
  ExtensionRegistry,
  type ExtensionPoint,
} from '../src/extensionPoints';

interface MockFeature {
  renderButton(): string;
  calcValue(n: number): number;
}

const defaultMockFeature: MockFeature = {
  renderButton: () => 'default-btn',
  calcValue: (n) => n * 1,
};

const proMockFeature: MockFeature = {
  renderButton: () => 'pro-btn',
  calcValue: (n) => n * 10,
};

describe('ExtensionPoints & ExtensionRegistry', () => {
  let registry: ExtensionRegistry;
  let point: ExtensionPoint<MockFeature>;

  beforeEach(() => {
    registry = new ExtensionRegistry();
    point = createExtensionPoint<MockFeature>({
      id: 'test.feature.mock',
      name: 'Mock Feature',
      description: 'Used for unit testing extension points',
      getDefault: () => defaultMockFeature,
    });
  });

  it('creates an extension point with immutable properties', () => {
    expect(point.id).toBe('test.feature.mock');
    expect(point.name).toBe('Mock Feature');
    expect(point.description).toBe('Used for unit testing extension points');
    expect(point.getDefault()).toBe(defaultMockFeature);
  });

  it('returns default implementation when no extension is registered', () => {
    expect(registry.isEnhanced(point)).toBe(false);
    const impl = registry.get(point);
    expect(impl).toBe(defaultMockFeature);
    expect(impl.renderButton()).toBe('default-btn');
    expect(impl.calcValue(5)).toBe(5);
  });

  it('guarantees referential stability for default fallback even if getDefault returns a new object', () => {
    const freshPoint = createExtensionPoint<{ count: number }>({
      id: 'test.fresh',
      name: 'Fresh Object Point',
      getDefault: () => ({ count: 1 }),
    });

    const first = registry.get(freshPoint);
    const second = registry.get(freshPoint);
    expect(first).toBe(second);
  });

  it('registers and resolves enhanced implementation', () => {
    const unregister = registry.register(point, proMockFeature);

    expect(registry.isEnhanced(point)).toBe(true);
    expect(registry.getAllRegisteredIds()).toEqual(['test.feature.mock']);

    const impl = registry.get(point);
    expect(impl).toBe(proMockFeature);
    expect(impl.renderButton()).toBe('pro-btn');
    expect(impl.calcValue(5)).toBe(50);

    unregister();
    expect(registry.isEnhanced(point)).toBe(false);
    expect(registry.get(point)).toBe(defaultMockFeature);
  });

  it('supports explicit unregister via registry.unregister(point)', () => {
    registry.register(point, proMockFeature);
    expect(registry.isEnhanced(point)).toBe(true);

    registry.unregister(point);
    expect(registry.isEnhanced(point)).toBe(false);
    expect(registry.get(point)).toBe(defaultMockFeature);
  });

  it('notifies subscribers on registration and unregistration', () => {
    const listener = vi.fn();
    const globalListener = vi.fn();

    const unsub = registry.subscribe(point.id, listener);
    const unsubGlobal = registry.subscribeAll(globalListener);

    expect(listener).not.toHaveBeenCalled();
    expect(globalListener).not.toHaveBeenCalled();

    registry.register(point, proMockFeature);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(globalListener).toHaveBeenCalledTimes(1);

    registry.unregister(point);
    expect(listener).toHaveBeenCalledTimes(2);
    expect(globalListener).toHaveBeenCalledTimes(2);

    unsub();
    unsubGlobal();
    registry.register(point, proMockFeature);
    expect(listener).toHaveBeenCalledTimes(2);
    expect(globalListener).toHaveBeenCalledTimes(2);
  });

  it('resets all registrations and notifies subscribers', () => {
    const listener = vi.fn();
    registry.subscribe(point.id, listener);

    registry.register(point, proMockFeature);
    expect(registry.isEnhanced(point)).toBe(true);
    listener.mockClear();

    registry.reset();
    expect(registry.isEnhanced(point)).toBe(false);
    expect(registry.getAllRegisteredIds()).toEqual([]);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('sqlEditorProEP fallback returns empty settingsContributions array', async () => {
    const { sqlEditorProEP } = await import('../src/sqlEditorProEP');
    const fallback = sqlEditorProEP.getDefault();
    expect(fallback.settingsContributions).toEqual([]);
  });
});
