import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearPlatformServices,
  isPlatformServicesBound,
  requirePlatformServices,
  setPlatformServices,
  type OpenedDirectory,
  type OpenedTextFile,
  type PlatformServices,
} from '../src/index';

/**
 * A full implementation built from spies.
 *
 * `openDirectoryWithDialog` is implemented (not omitted) so that "injection
 * succeeded" is asserted against a complete `PlatformServices`; a mock that
 * only covers the members under test would pass even if the interface grew.
 */
function createSpyServices() {
  return {
    saveTextWithDialog: vi.fn(async () => true),
    saveBinaryWithDialog: vi.fn(async () => true),
    openTextWithDialog: vi.fn(
      async () => ({ fileName: 'a.sql', content: 'SELECT 1;' }) satisfies OpenedTextFile,
    ),
    openDirectoryWithDialog: vi.fn(async () => ({ path: '/tmp/out' }) satisfies OpenedDirectory),
    writeClipboard: vi.fn(async () => undefined),
    readClipboard: vi.fn(async () => 'clipboard text'),
  } satisfies PlatformServices;
}

describe('PlatformServices injection', () => {
  beforeEach(() => clearPlatformServices());
  afterEach(() => clearPlatformServices());

  it('is unbound before the host injects anything', () => {
    expect(isPlatformServicesBound()).toBe(false);
  });

  it('resolves to the injected implementation and forwards the call', async () => {
    const services = createSpyServices();
    setPlatformServices(services);

    expect(isPlatformServicesBound()).toBe(true);
    const resolved = requirePlatformServices();
    expect(resolved).toBe(services);

    await resolved.saveTextWithDialog({
      contents: 'SELECT 1;',
      defaultFileName: 'q.sql',
      filterName: 'SQL',
      extensions: ['sql'],
    });

    expect(services.saveTextWithDialog).toHaveBeenCalledWith({
      contents: 'SELECT 1;',
      defaultFileName: 'q.sql',
      filterName: 'SQL',
      extensions: ['sql'],
    });
  });

  it('lets a later injection replace the earlier one', () => {
    const first = createSpyServices();
    const second = createSpyServices();

    setPlatformServices(first);
    setPlatformServices(second);

    expect(requirePlatformServices()).toBe(second);
  });

  it('round-trips the clipboard in both directions', async () => {
    const services = createSpyServices();
    setPlatformServices(services);

    await requirePlatformServices().writeClipboard('copied');
    await expect(requirePlatformServices().readClipboard()).resolves.toBe('clipboard text');

    expect(services.writeClipboard).toHaveBeenCalledWith('copied');
  });
});

describe('unbound PlatformServices reports an explicit error', () => {
  beforeEach(() => clearPlatformServices());
  afterEach(() => clearPlatformServices());

  it('throws rather than falling back to a direct host call', () => {
    expect(isPlatformServicesBound()).toBe(false);
    expect(() => requirePlatformServices()).toThrow(
      'PlatformServices has not been bound; check that the platform adapter ran.',
    );
  });

  it('names the missing binding so the stack trace locates the adapter', () => {
    let message = '';
    try {
      requirePlatformServices();
    } catch (error) {
      message = error instanceof Error ? error.message : '';
    }
    // The message has to point at the thing that was supposed to run.
    expect(message).toContain('has not been bound');
    expect(message).toContain('platform adapter');
  });

  it('goes back to throwing after the binding is cleared', () => {
    setPlatformServices(createSpyServices());
    expect(() => requirePlatformServices()).not.toThrow();

    clearPlatformServices();
    expect(() => requirePlatformServices()).toThrow('has not been bound');
  });
});
