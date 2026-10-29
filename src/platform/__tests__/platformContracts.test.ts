import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearPlatform,
  configurePlatform,
  inertHostSettingsPort,
  requireCapabilities,
  requireHostSettings,
  requirePlatform,
  requireTablesExporter,
  type HostSettingsPort,
  type TablesExporterPort,
} from '../contracts';
import {
  setPlatformServices,
  clearPlatformServices,
  requirePlatformServices,
} from '@datazen/backend-client';

/**
 * Form-split F5 port contracts: §7.4 semantics — an unbound port must fail
 * with a **locatable** error (never a silent fallback), and a bound port must
 * hand back exactly what the assembly installed. One suite per port family.
 */

function minimalBindings(overrides?: Partial<Parameters<typeof configurePlatform>[0]>) {
  return {
    services: {} as never,
    windows: {
      openSingletonWindow: () => undefined,
      openMultiInstanceWindow: () => undefined,
      focusMainWindow: async () => undefined,
      hasOpenChildWindows: async () => false,
      windowChrome: {
        isFullscreen: async () => false,
        onFullscreenChanged: async () => () => undefined,
        setBackgroundRootColor: async () => undefined,
        onCloseRequested: async () => () => undefined,
        minimizeCurrent: async () => undefined,
        closeCurrent: async () => undefined,
      },
    },
    events: {
      emitCrossWindow: vi.fn().mockResolvedValue(undefined),
      listenCrossWindow: vi.fn().mockResolvedValue(() => {}),
    },
    ...overrides,
  } as const;
}

beforeEach(() => {
  clearPlatform();
  clearPlatformServices();
});

afterEach(() => {
  clearPlatform();
  clearPlatformServices();
});

describe('Platform bindings port (§7.4)', () => {
  it('unbound requirePlatform() names the missing assembly step', () => {
    expect(() => requirePlatform()).toThrow(
      'Platform ports have not been configured; check that the app assembly root ran configurePlatform().',
    );
  });

  it('bound requirePlatform() returns the installed bindings', () => {
    const bindings = minimalBindings();
    configurePlatform(bindings);
    expect(requirePlatform().events).toBe(bindings.events);
  });

  it('rebinding replaces (tests rebind freely)', () => {
    const first = minimalBindings();
    const second = minimalBindings();
    configurePlatform(first);
    configurePlatform(second);
    expect(requirePlatform().events).toBe(second.events);
  });
});

describe('PlatformServices port (backend-client §7.4)', () => {
  it('unbound requirePlatformServices() names the missing adapter', () => {
    expect(() => requirePlatformServices()).toThrow(
      'PlatformServices has not been bound; check that the platform adapter ran.',
    );
  });

  it('bound requirePlatformServices() serves the injected implementation', async () => {
    const writeClipboard = vi.fn().mockResolvedValue(undefined);
    setPlatformServices({
      saveTextWithDialog: async () => true,
      saveBinaryWithDialog: async () => true,
      openTextWithDialog: async () => null,
      openDirectoryWithDialog: async () => null,
      writeClipboard,
      writeClipboardHtml: async () => undefined,
      readClipboard: async () => 'copied',
      openExternal: async () => undefined,
      toggleFullscreen: async () => undefined,
      showNotification: async () => undefined,
      pickPath: async () => null,
      getOsPlatform: async () => 'unknown' as const,
    });
    await expect(requirePlatformServices().readClipboard()).resolves.toBe('copied');
  });
});

describe('HostSettings port (F4)', () => {
  it('unbound requireHostSettings() degrades to the inert default — branch-free by design', async () => {
    // Unlike the §7.4 throw ports, the host-settings accessor deliberately
    // resolves an inert port: settings call sites are ambient reads that every
    // form makes, and the inert port IS the web behavior.
    await expect(requireHostSettings().readHostExtras()).resolves.toEqual({});
    await expect(requireHostSettings().writeHostSettings({} as never)).resolves.toBeUndefined();
  });

  it('bound requireHostSettings() returns the desktop settings client', async () => {
    const port: HostSettingsPort = {
      ...inertHostSettingsPort(),
      readHostExtras: async () => ({ logPath: '/var/log' }),
    };
    configurePlatform(minimalBindings({ hostSettings: port }));
    await expect(requireHostSettings().readHostExtras()).resolves.toEqual({ logPath: '/var/log' });
  });
});

describe('TablesExporter port (F4 dual-flow)', () => {
  it('unbound requireTablesExporter() fails loudly — a missing download is a wiring gap', () => {
    expect(() => requireTablesExporter()).toThrow(
      'No tables exporter has been bound; check that the app assembly root ran configurePlatform().',
    );
  });

  it('bound requireTablesExporter() serves the installed form', async () => {
    const exporter: TablesExporterPort = {
      exportTables: async () => ({ Saved: 1 }),
    };
    configurePlatform(minimalBindings({ tablesExporter: exporter }));
    await expect(requireTablesExporter().exportTables({} as never)).resolves.toEqual({ Saved: 1 });
  });
});

describe('Capabilities table (F5)', () => {
  it('requireCapabilities() throws while no table is bound', () => {
    configurePlatform(minimalBindings());
    expect(() => requireCapabilities()).toThrow(/form-split F5/);
  });

  it('bound capabilities hand back the static table', () => {
    const capabilities = {
      settingsSections: new Set(['general'] as const),
      updater: false,
      versionSource: 'build-constant' as const,
    };
    configurePlatform(minimalBindings({ capabilities }));
    expect(requireCapabilities().updater).toBe(false);
    expect(requireCapabilities().versionSource).toBe('build-constant');
  });
});
