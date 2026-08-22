import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  applyPluginTheme,
  applyThemePack,
  clearThemePackDom,
  encodePluginThemePackId,
  parsePluginThemePackId,
} from '../themePackApply';
import { emitCrossWindow } from '../crossWindowBus';

const mockReadPluginFile = vi.fn();
const mockGetPluginManifest = vi.fn();
const mockReadThemePackFile = vi.fn();

vi.mock('../../commands/plugins', () => ({
  pluginCommands: {
    readPluginFile: (...args: unknown[]) => mockReadPluginFile(...args),
    getPluginManifest: (...args: unknown[]) => mockGetPluginManifest(...args),
  },
}));

vi.mock('../../commands/theme', () => ({
  themeCommands: {
    readThemePackFile: (...args: unknown[]) => mockReadThemePackFile(...args),
    setSurfaceBackground: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

const mockEmitCrossWindow = vi.mocked(emitCrossWindow);

function toBytes(text: string): number[] {
  return [...new TextEncoder().encode(text)];
}

const MANIFEST = {
  id: 'acme.bill-audit',
  name: 'Bill Audit',
  version: '1.0.0',
  apiVersion: 2,
  contributes: {
    pages: [],
    themes: [
      {
        id: 'midnight-blue',
        name: 'Midnight Blue',
        tokensCss: 'themes/midnight-blue/tokens.css',
        modes: ['dark'],
      },
      { id: 'solar', name: 'Solar', tokensCss: 'themes/solar/tokens.css', modes: ['light'] },
    ],
  },
  permissions: [],
};

describe('plugin theme pack id codec', () => {
  beforeEach(() => {
    mockEmitCrossWindow.mockClear();
  });

  it('round-trips pluginId and themeId', () => {
    const encoded = encodePluginThemePackId('acme.bill-audit', 'midnight-blue');
    expect(encoded).toBe('plugin:acme.bill-audit:midnight-blue');
    expect(parsePluginThemePackId(encoded)).toEqual({
      pluginId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
  });

  it('returns null for legacy theme pack ids and malformed input', () => {
    expect(parsePluginThemePackId('legacy-pack')).toBeNull();
    expect(parsePluginThemePackId(null)).toBeNull();
    expect(parsePluginThemePackId(undefined)).toBeNull();
    expect(parsePluginThemePackId('plugin:')).toBeNull();
    expect(parsePluginThemePackId('plugin:only-plugin-id')).toBeNull();
    expect(parsePluginThemePackId('plugin::theme')).toBeNull();
  });
});

describe('applyPluginTheme', () => {
  beforeEach(() => {
    clearThemePackDom();
    mockReadPluginFile.mockReset();
    mockGetPluginManifest.mockReset().mockResolvedValue(MANIFEST);
    mockReadThemePackFile.mockReset();
    mockEmitCrossWindow.mockClear();
  });

  it('loads tokens.css from the plugin via readPluginFile and injects it', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css') {
        return toBytes(':root { --c-accent: #1122ff; }');
      }
      return null;
    });

    const result = await applyPluginTheme({
      pluginId: 'acme.bill-audit',
      themeId: 'midnight-blue',
      name: 'Midnight Blue',
    });
    expect(result).toEqual({ ok: true });
    expect(mockGetPluginManifest).toHaveBeenCalledWith('acme.bill-audit');
    expect(mockReadPluginFile).toHaveBeenCalledWith(
      'acme.bill-audit',
      'themes/midnight-blue/tokens.css',
    );
    expect(mockReadThemePackFile).not.toHaveBeenCalled();
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
  });

  it('rewrites local url(...) references to blob URLs via readPluginFile', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("./fonts/custom.woff2"); }');
      }
      if (path === 'themes/solar/fonts/custom.woff2') return [0x77, 0x4f, 0x46, 0x46];
      return null;
    });

    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadPluginFile).toHaveBeenCalledWith(
      'acme.bill-audit',
      'themes/solar/fonts/custom.woff2',
    );
    const css = document.getElementById('datazen-theme-pack')?.textContent ?? '';
    expect(css).toMatch(/url\("blob:/);
  });

  it('rejects remote http URLs referenced by the tokens css', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css') {
        return toBytes('@font-face { src: url("https://evil.example/font.woff2"); }');
      }
      return null;
    });

    const result = await applyPluginTheme({
      pluginId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/Remote font URL not allowed/);
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('returns an error when the tokens css file is missing', async () => {
    mockReadPluginFile.mockResolvedValue(null);
    const result = await applyPluginTheme({
      pluginId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
    expect(result).toEqual({ ok: false, error: 'tokens.css missing' });
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('returns an error when the theme id is not in the manifest', async () => {
    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'nope' });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Theme "nope" not found in plugin "acme\.bill-audit"/);
    }
    expect(mockReadPluginFile).not.toHaveBeenCalled();
  });

  it('PT-10: resolves bare url() refs against the tokens.css directory', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') return toBytes('@font-face { src: url("bg.png"); }');
      if (path === 'themes/solar/bg.png') return [0x89, 0x50];
      return null;
    });

    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadPluginFile).toHaveBeenCalledWith('acme.bill-audit', 'themes/solar/bg.png');
  });

  it('PT-11: resolves ../ url() refs against the tokens.css directory without escaping it lexically upward beyond root', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("../shared/accent.svg"); }');
      }
      if (path === 'themes/shared/accent.svg') return [0x3c, 0x73];
      return null;
    });

    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadPluginFile).toHaveBeenCalledWith('acme.bill-audit', 'themes/shared/accent.svg');
  });

  it('PT-12: rejects ../../ traversal that would leave the plugin root (stays inside the sandbox)', async () => {
    const requestedPaths: string[] = [];
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      requestedPaths.push(path);
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("../../../../../etc/evil.woff2"); }');
      }
      return null;
    });

    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
    expect(result.ok).toBe(false);
    for (const path of requestedPaths) {
      expect(path.startsWith('..')).toBe(false);
      expect(path.includes('/../')).toBe(false);
      expect(path.startsWith('/')).toBe(false);
    }
    expect(requestedPaths).toContain('etc/evil.woff2');
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('PT-13: treats root-absolute url() refs as plugin-root relative paths', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("/assets/logo.png"); }');
      }
      if (path === 'assets/logo.png') return [0x89, 0x50];
      return null;
    });

    const result = await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadPluginFile).toHaveBeenCalledWith('acme.bill-audit', 'assets/logo.png');
  });

  it('PT-14: switching to another plugin theme replaces the injected css and revokes previous blob urls', async () => {
    const createdBlobUrls: string[] = [];
    const originalCreateObjectURL = URL.createObjectURL;
    const originalRevokeObjectURL = URL.revokeObjectURL;
    vi.spyOn(URL, 'createObjectURL').mockImplementation((blob: Blob) => {
      const url = originalCreateObjectURL.call(URL, blob);
      createdBlobUrls.push(url);
      return url;
    });
    const revoked: string[] = [];
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation((url: string) => {
      revoked.push(url);
      originalRevokeObjectURL.call(URL, url);
    });

    try {
      mockGetPluginManifest.mockResolvedValue({
        ...MANIFEST,
        contributes: {
          pages: [],
          themes: [
            {
              id: 'midnight-blue',
              name: 'Midnight Blue',
              tokensCss: 'themes/midnight-blue/tokens.css',
              modes: ['dark'],
            },
            { id: 'solar', name: 'Solar', tokensCss: 'themes/solar/tokens.css', modes: ['light'] },
          ],
        },
      });
      mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
        if (path === 'themes/midnight-blue/tokens.css') {
          return toBytes(':root { --c-accent: #1122ff; }');
        }
        if (path === 'themes/solar/tokens.css') {
          return toBytes('@font-face { src: url("./f.woff2"); }:root { --c-accent: #ffaa00; }');
        }
        if (path === 'themes/solar/f.woff2') return [0x77, 0x4f];
        return null;
      });

      // First apply creates one blob url for the referenced font asset.
      await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'solar' });
      expect(document.getElementById('datazen-theme-pack')?.textContent).toMatch(/url\("blob:/);
      expect(createdBlobUrls.length).toBe(1);
      expect(revoked).not.toContain(createdBlobUrls[0]);

      // Switching back revokes the previous blob url and replaces the injected css.
      await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'midnight-blue' });
      const css = document.getElementById('datazen-theme-pack')?.textContent ?? '';
      expect(css).toContain('#1122ff');
      expect(css).not.toContain('#ffaa00');
      expect(revoked).toContain(createdBlobUrls[0]);
    } finally {
      vi.restoreAllMocks();
      clearThemePackDom();
    }
  });

  it('PT-15: failure after a successful apply resets the DOM and broadcasts a null pack change', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css')
        return toBytes(':root { --c-accent: #1122ff; }');
      return null;
    });

    await applyPluginTheme({ pluginId: 'acme.bill-audit', themeId: 'midnight-blue' });
    expect(document.getElementById('datazen-theme-pack')).not.toBeNull();
    mockEmitCrossWindow.mockClear();

    // Plugin got disabled / files removed → manifest lookup fails.
    mockGetPluginManifest.mockRejectedValue(new Error('plugin disabled or missing'));
    const result = await applyPluginTheme({
      pluginId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });

    expect(result.ok).toBe(false);
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('datazen:theme-pack-changed', null);
  });

  it('PT-16: broadcast=false suppresses the cross-window pack change event', async () => {
    mockReadPluginFile.mockResolvedValue(toBytes(':root { --c-accent: #1122ff; }'));

    const result = await applyPluginTheme(
      { pluginId: 'acme.bill-audit', themeId: 'midnight-blue' },
      { broadcast: false },
    );

    expect(result).toEqual({ ok: true });
    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
  });
});

describe('applyThemePack dispatches encoded plugin ids to the plugin path', () => {
  beforeEach(() => {
    clearThemePackDom();
    mockReadPluginFile.mockReset();
    mockGetPluginManifest.mockReset().mockResolvedValue(MANIFEST);
    mockReadThemePackFile.mockReset();
    mockEmitCrossWindow.mockClear();
  });

  it('applies a persisted `plugin:` packId without touching legacy packs', async () => {
    mockReadPluginFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css')
        return toBytes(':root { --c-accent: #00ff88; }');
      return null;
    });

    const result = await applyThemePack(
      encodePluginThemePackId('acme.bill-audit', 'midnight-blue'),
    );
    expect(result).toEqual({ ok: true });
    expect(mockReadPluginFile).toHaveBeenCalled();
    expect(mockReadThemePackFile).not.toHaveBeenCalled();
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
  });

  it('still resolves plain legacy pack ids through read_theme_pack_file', async () => {
    mockReadThemePackFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'tokens.css') return toBytes(':root { --c-accent: #abcdef; }');
      return null;
    });

    const result = await applyThemePack('classic-pack');
    expect(result).toEqual({ ok: true });
    expect(mockReadThemePackFile).toHaveBeenCalledWith('classic-pack', 'tokens.css');
    expect(mockReadPluginFile).not.toHaveBeenCalled();
  });
});
