import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  applyWappTheme,
  applyThemePack,
  clearThemePackDom,
  encodeWappThemePackId,
  parseWappThemePackId,
} from '../themePackApply';
import { emitCrossWindow } from '../crossWindowBus';

const mockReadWappFile = vi.fn();
const mockGetWappManifest = vi.fn();

vi.mock('../../commands/wapps', () => ({
  wappCommands: {
    readWappFile: (...args: unknown[]) => mockReadWappFile(...args),
    getWappManifest: (...args: unknown[]) => mockGetWappManifest(...args),
  },
}));

vi.mock('../crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

// Spy wrappers over the legacy-parity sinks (real behavior preserved, calls
// recorded) so the optional-asset pipeline can be asserted.
vi.mock('../chart/colors', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../chart/colors')>();
  return { ...actual, setChartPaletteOverride: vi.fn(actual.setChartPaletteOverride) };
});
vi.mock('../themeEditorColors', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../themeEditorColors')>();
  return { ...actual, setPackEditorColorOverlay: vi.fn(actual.setPackEditorColorOverlay) };
});
vi.mock('../iconResolver', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../iconResolver')>();
  return { ...actual, setActiveIconResolver: vi.fn(actual.setActiveIconResolver) };
});

import { setChartPaletteOverride } from '../chart/colors';
import { setPackEditorColorOverlay } from '../themeEditorColors';
import { setActiveIconResolver } from '../iconResolver';

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

describe('wapp theme pack id codec', () => {
  beforeEach(() => {
    mockEmitCrossWindow.mockClear();
  });

  it('round-trips wappId and themeId', () => {
    const encoded = encodeWappThemePackId('acme.bill-audit', 'midnight-blue');
    expect(encoded).toBe('wapp:acme.bill-audit:midnight-blue');
    expect(parseWappThemePackId(encoded)).toEqual({
      wappId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
  });

  it('returns null for legacy theme pack ids and malformed input', () => {
    expect(parseWappThemePackId('legacy-pack')).toBeNull();
    expect(parseWappThemePackId(null)).toBeNull();
    expect(parseWappThemePackId(undefined)).toBeNull();
    expect(parseWappThemePackId('wapp:')).toBeNull();
    expect(parseWappThemePackId('wapp:only-wapp-id')).toBeNull();
    expect(parseWappThemePackId('wapp::theme')).toBeNull();
  });
});

describe('applyWappTheme', () => {
  beforeEach(() => {
    clearThemePackDom();
    mockReadWappFile.mockReset();
    mockGetWappManifest.mockReset().mockResolvedValue(MANIFEST);
    mockEmitCrossWindow.mockClear();
  });

  it('loads tokens.css from the wapp via readWappFile and injects it', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css') {
        return toBytes(':root { --c-accent: #1122ff; }');
      }
      return null;
    });

    const result = await applyWappTheme({
      wappId: 'acme.bill-audit',
      themeId: 'midnight-blue',
      name: 'Midnight Blue',
    });
    expect(result).toEqual({ ok: true });
    expect(mockGetWappManifest).toHaveBeenCalledWith('acme.bill-audit');
    expect(mockReadWappFile).toHaveBeenCalledWith(
      'acme.bill-audit',
      'themes/midnight-blue/tokens.css',
    );
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
  });

  it('rewrites local url(...) references to blob URLs via readWappFile', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("./fonts/custom.woff2"); }');
      }
      if (path === 'themes/solar/fonts/custom.woff2') return [0x77, 0x4f, 0x46, 0x46];
      return null;
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalledWith(
      'acme.bill-audit',
      'themes/solar/fonts/custom.woff2',
    );
    const css = document.getElementById('datazen-theme-pack')?.textContent ?? '';
    expect(css).toMatch(/url\("blob:/);
  });

  it('rejects remote http URLs referenced by the tokens css', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css') {
        return toBytes('@font-face { src: url("https://evil.example/font.woff2"); }');
      }
      return null;
    });

    const result = await applyWappTheme({
      wappId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/Remote font URL not allowed/);
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('returns an error when the tokens css file is missing', async () => {
    mockReadWappFile.mockResolvedValue(null);
    const result = await applyWappTheme({
      wappId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });
    expect(result).toEqual({ ok: false, error: 'tokens.css missing' });
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('returns an error when the theme id is not in the manifest', async () => {
    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'nope' });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Theme "nope" not found in wapp "acme\.bill-audit"/);
    }
    expect(mockReadWappFile).not.toHaveBeenCalled();
  });

  it('PT-10: resolves bare url() refs against the tokens.css directory', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') return toBytes('@font-face { src: url("bg.png"); }');
      if (path === 'themes/solar/bg.png') return [0x89, 0x50];
      return null;
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalledWith('acme.bill-audit', 'themes/solar/bg.png');
  });

  it('PT-11: resolves ../ url() refs against the tokens.css directory without escaping it lexically upward beyond root', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("../shared/accent.svg"); }');
      }
      if (path === 'themes/shared/accent.svg') return [0x3c, 0x73];
      return null;
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalledWith('acme.bill-audit', 'themes/shared/accent.svg');
  });

  it('PT-12: rejects ../../ traversal that would leave the wapp root (stays inside the sandbox)', async () => {
    const requestedPaths: string[] = [];
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      requestedPaths.push(path);
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("../../../../../etc/evil.woff2"); }');
      }
      return null;
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
    expect(result.ok).toBe(false);
    for (const path of requestedPaths) {
      expect(path.startsWith('..')).toBe(false);
      expect(path.includes('/../')).toBe(false);
      expect(path.startsWith('/')).toBe(false);
    }
    expect(requestedPaths).toContain('etc/evil.woff2');
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('PT-13: treats root-absolute url() refs as wapp-root relative paths', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/solar/tokens.css') {
        return toBytes('@font-face { src: url("/assets/logo.png"); }');
      }
      if (path === 'assets/logo.png') return [0x89, 0x50];
      return null;
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalledWith('acme.bill-audit', 'assets/logo.png');
  });

  it('PT-14: switching to another wapp theme replaces the injected css and revokes previous blob urls', async () => {
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
      mockGetWappManifest.mockResolvedValue({
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
      mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
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
      await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'solar' });
      expect(document.getElementById('datazen-theme-pack')?.textContent).toMatch(/url\("blob:/);
      expect(createdBlobUrls.length).toBe(1);
      expect(revoked).not.toContain(createdBlobUrls[0]);

      // Switching back revokes the previous blob url and replaces the injected css.
      await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'midnight-blue' });
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
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css')
        return toBytes(':root { --c-accent: #1122ff; }');
      return null;
    });

    await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'midnight-blue' });
    expect(document.getElementById('datazen-theme-pack')).not.toBeNull();
    mockEmitCrossWindow.mockClear();

    // Wapp got disabled / files removed → manifest lookup fails.
    mockGetWappManifest.mockRejectedValue(new Error('wapp disabled or missing'));
    const result = await applyWappTheme({
      wappId: 'acme.bill-audit',
      themeId: 'midnight-blue',
    });

    expect(result.ok).toBe(false);
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
    expect(mockEmitCrossWindow).toHaveBeenCalledWith('datazen:theme-pack-changed', null);
  });

  it('PT-16: broadcast=false suppresses the cross-window pack change event', async () => {
    mockReadWappFile.mockResolvedValue(toBytes(':root { --c-accent: #1122ff; }'));

    const result = await applyWappTheme(
      { wappId: 'acme.bill-audit', themeId: 'midnight-blue' },
      { broadcast: false },
    );

    expect(result).toEqual({ ok: true });
    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
  });
});

describe('applyThemePack dispatches encoded wapp ids to the wapp path', () => {
  beforeEach(() => {
    clearThemePackDom();
    mockReadWappFile.mockReset();
    mockGetWappManifest.mockReset().mockResolvedValue(MANIFEST);
    mockEmitCrossWindow.mockClear();
  });

  it('applies a persisted `wapp:` packId without touching legacy packs', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'themes/midnight-blue/tokens.css')
        return toBytes(':root { --c-accent: #00ff88; }');
      return null;
    });

    const result = await applyThemePack(encodeWappThemePackId('acme.bill-audit', 'midnight-blue'));
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalled();
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
  });

  it('rejects plain legacy pack ids', async () => {
    const result = await applyThemePack('classic-pack');
    expect(result).toEqual({ ok: false, error: 'unknown theme pack: classic-pack' });
    expect(mockReadWappFile).not.toHaveBeenCalled();
  });
});

describe('applyWappTheme legacy-parity assets', () => {
  const RICH_MANIFEST = {
    ...MANIFEST,
    contributes: {
      pages: [],
      themes: [
        ...(
          MANIFEST as {
            contributes: { themes: Array<Record<string, unknown>> };
          }
        ).contributes.themes,
        {
          id: 'night',
          name: 'Night',
          tokensCss: 'themes/night/tokens.css',
          modes: ['dark'],
          chartsJson: 'themes/night/charts.json',
          editorJson: 'themes/night/editor.json',
          iconsDir: 'themes/night/icons',
        },
      ],
    },
  };

  function nonNullCalls(fn: { mock: { calls: unknown[][] } }): unknown[] {
    return fn.mock.calls.map((c) => c[0]).filter((v) => v !== null);
  }

  beforeEach(() => {
    clearThemePackDom();
    mockReadWappFile.mockReset();
    mockGetWappManifest.mockReset().mockResolvedValue(RICH_MANIFEST);
    mockEmitCrossWindow.mockClear();
    vi.mocked(setChartPaletteOverride).mockClear();
    vi.mocked(setPackEditorColorOverlay).mockClear();
    vi.mocked(setActiveIconResolver).mockClear();
  });

  it('applies chart palette, icon overrides, and editor overlay', async () => {
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      switch (path) {
        case 'themes/night/tokens.css':
          return toBytes(':root { --c-accent: #101010; }');
        case 'themes/night/charts.json':
          return toBytes('{"default":["#111111","#222222"]}');
        case 'themes/night/editor.json':
          return toBytes('{"keyword":"#ff0000"}');
        case 'themes/night/icons/nav.settings.svg':
          return toBytes('<svg xmlns="http://www.w3.org/2000/svg"></svg>');
        default:
          return null;
      }
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'night' });
    expect(result).toEqual({ ok: true });
    expect(mockReadWappFile).toHaveBeenCalledWith(
      'acme.bill-audit',
      'themes/night/icons/nav.settings.svg',
    );
    expect(nonNullCalls(vi.mocked(setChartPaletteOverride))).toContainEqual({
      default: ['#111111', '#222222'],
    });
    expect(nonNullCalls(vi.mocked(setPackEditorColorOverlay))).toHaveLength(1);
    expect(vi.mocked(setActiveIconResolver)).toHaveBeenCalled();
  });

  it('soft-fails malformed optional assets without failing the theme', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mockReadWappFile.mockImplementation(async (_id: string, path: string) => {
      switch (path) {
        case 'themes/night/tokens.css':
          return toBytes(':root { --c-accent: #101010; }');
        case 'themes/night/charts.json':
          return toBytes('not-json{');
        default:
          return null;
      }
    });

    const result = await applyWappTheme({ wappId: 'acme.bill-audit', themeId: 'night' });
    expect(result).toEqual({ ok: true });
    // Unparseable palette resets the override (legacy semantics), theme stays on.
    expect(vi.mocked(setChartPaletteOverride)).toHaveBeenLastCalledWith(null);
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
    warn.mockRestore();
  });
});
