import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  applyThemePack,
  clearThemePack,
  clearThemePackDom,
  injectThemePackCss,
  rewriteFontUrls,
  syncWebviewBackgroundFromTokens,
} from '../themePackApply';
import { parsePackEditorOverlay } from '../themeEditorColors';

const mockReadThemePackFile = vi.fn();

vi.mock('../../commands/theme', () => ({
  themeCommands: {
    readThemePackFile: (...args: unknown[]) => mockReadThemePackFile(...args),
    setSurfaceBackground: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

const FIXTURE_TOKENS_CSS = readFileSync(
  resolve(__dirname, '../../../fixtures/themes/community.fixture-dark/tokens.css'),
  'utf8',
);

describe('injectThemePackCss', () => {
  beforeEach(() => {
    clearThemePackDom();
  });

  it('upserts style#datazen-theme-pack', () => {
    injectThemePackCss(':root { --c-accent: #ff00ff; }');
    const el = document.getElementById('datazen-theme-pack');
    expect(el?.tagName).toBe('STYLE');
    expect(el?.textContent).toContain('--c-accent: #ff00ff');
    injectThemePackCss(':root { --c-accent: #00ff00; }');
    expect(document.querySelectorAll('#datazen-theme-pack')).toHaveLength(1);
    expect(el?.textContent).toContain('#00ff00');
  });

  it('injects fixture tokens.css content into #datazen-theme-pack', () => {
    injectThemePackCss(FIXTURE_TOKENS_CSS);
    const el = document.getElementById('datazen-theme-pack');
    expect(el?.textContent).toContain('--c-accent: #7c3aed');
    expect(el?.textContent).toContain('Fixture Mono');
  });
});

describe('clearThemePackDom / clearThemePack', () => {
  beforeEach(() => {
    clearThemePackDom();
  });

  it('clearThemePackDom removes #datazen-theme-pack after inject', () => {
    injectThemePackCss(':root { --c-accent: #ff00ff; }');
    expect(document.getElementById('datazen-theme-pack')).not.toBeNull();
    clearThemePackDom();
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('clearThemePack removes #datazen-theme-pack after inject', () => {
    injectThemePackCss(':root { --c-accent: #ff00ff; }');
    expect(document.getElementById('datazen-theme-pack')).not.toBeNull();
    clearThemePack();
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });
});

describe('applyThemePack(null)', () => {
  beforeEach(() => {
    clearThemePackDom();
  });

  it('clears #datazen-theme-pack without Tauri IPC', async () => {
    injectThemePackCss(':root { --c-accent: #ff00ff; }');
    const result = await applyThemePack(null);
    expect(result).toEqual({ ok: true });
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });
});

describe('parsePackEditorOverlay', () => {
  it('accepts known editor.json keys', () => {
    expect(parsePackEditorOverlay({ keyword: '#ff00ff', string: '#00ff00' })).toEqual({
      keyword: '#ff00ff',
      string: '#00ff00',
    });
  });

  it('ignores unknown keys', () => {
    expect(parsePackEditorOverlay({ unknown: '#fff' })).toBeNull();
  });
});

describe('rewriteFontUrls', () => {
  beforeEach(() => {
    mockReadThemePackFile.mockReset();
  });

  it('rejects remote http font URLs', async () => {
    const css = '@font-face { src: url("https://evil.example/font.woff2"); }';
    await expect(rewriteFontUrls(css, 'pack-1')).rejects.toThrow(/Remote font URL not allowed/);
  });

  it('skips blob and data URLs', async () => {
    const css = '@font-face { src: url(blob:abc), url(data:font/woff2;base64,AA); }';
    const result = await rewriteFontUrls(css, 'pack-1');
    expect(result).toBe(css);
    expect(mockReadThemePackFile).not.toHaveBeenCalled();
  });

  it('rewrites local font paths to blob URLs', async () => {
    mockReadThemePackFile.mockResolvedValue([0x77, 0x4f, 0x46, 0x46]); // WOFF magic
    const css = '@font-face { src: url("./fonts/custom.woff2"); }';
    const result = await rewriteFontUrls(css, 'pack-1');
    expect(result).toMatch(/url\("blob:/);
    expect(mockReadThemePackFile).toHaveBeenCalledWith('pack-1', 'fonts/custom.woff2');
  });

  it('throws when font file missing in pack', async () => {
    mockReadThemePackFile.mockResolvedValue(null);
    const css = '@font-face { src: url("missing.woff2"); }';
    await expect(rewriteFontUrls(css, 'pack-1')).rejects.toThrow(/Font file not found/);
  });
});

describe('syncWebviewBackgroundFromTokens', () => {
  beforeEach(() => {
    document.documentElement.style.backgroundColor = '';
    document.documentElement.style.removeProperty('--c-surface');
    document.documentElement.classList.remove('dark');
  });

  it('uses --c-surface when set', () => {
    document.documentElement.style.setProperty('--c-surface', '#112233');
    syncWebviewBackgroundFromTokens();
    expect(document.documentElement.style.backgroundColor).toBe('rgb(17, 34, 51)');
  });

  it('falls back to dark/light defaults', () => {
    syncWebviewBackgroundFromTokens();
    expect(document.documentElement.style.backgroundColor).toBe('rgb(255, 255, 255)');
    document.documentElement.classList.add('dark');
    syncWebviewBackgroundFromTokens();
    expect(document.documentElement.style.backgroundColor).toBe('rgb(15, 23, 42)');
  });
});

function toBytes(text: string): number[] {
  return [...new TextEncoder().encode(text)];
}

describe('applyThemePack with packId', () => {
  beforeEach(() => {
    clearThemePackDom();
    mockReadThemePackFile.mockReset();
  });

  it('applies tokens, fonts, editor overlay, charts, and icons', async () => {
    mockReadThemePackFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'tokens.css') return toBytes(':root { --c-accent: #aabbcc; }');
      if (path === 'fonts.css') return toBytes('@font-face { src: url("./f.woff2"); }');
      if (path === 'f.woff2') return [0x77, 0x4f, 0x46, 0x46];
      if (path === 'editor.json') return toBytes('{"keyword":"#ff0000"}');
      if (path === 'charts.json') return toBytes('{"custom":["#111","#222"]}');
      if (path.startsWith('icons/')) return [0x3c, 0x73, 0x76, 0x67]; // <svg
      return null;
    });

    const result = await applyThemePack('fixture-pack');
    expect(result).toEqual({ ok: true });
    const el = document.getElementById('datazen-theme-pack');
    expect(el?.textContent).toContain('--c-accent');
    expect(el?.textContent).toContain('blob:');
  });

  it('returns error when tokens.css missing', async () => {
    mockReadThemePackFile.mockResolvedValue(null);
    const result = await applyThemePack('bad-pack');
    expect(result).toEqual({ ok: false, error: 'tokens.css missing' });
    expect(document.getElementById('datazen-theme-pack')).toBeNull();
  });

  it('warns and continues when editor.json is invalid', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mockReadThemePackFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'tokens.css') return toBytes(':root {}');
      if (path === 'editor.json') return toBytes('not-json');
      return null;
    });
    const result = await applyThemePack('pack-editor-bad');
    expect(result).toEqual({ ok: true });
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it('ignores invalid charts.json and readPackFile errors for optional assets', async () => {
    mockReadThemePackFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'tokens.css') return toBytes(':root { --c-accent: #aabbcc; }');
      if (path === 'charts.json') return toBytes('not-json');
      if (path.startsWith('icons/')) throw new Error('icon read failed');
      return null;
    });
    const result = await applyThemePack('pack-charts-bad');
    expect(result).toEqual({ ok: true });
    expect(document.getElementById('datazen-theme-pack')?.textContent).toContain('--c-accent');
  });

  it('returns error when rewriteFontUrls fails during apply', async () => {
    mockReadThemePackFile.mockImplementation(async (_id: string, path: string) => {
      if (path === 'tokens.css') {
        return toBytes('@font-face { src: url("https://evil.example/font.woff2"); }');
      }
      return null;
    });
    const result = await applyThemePack('pack-bad-font');
    expect(result.ok).toBe(false);
    expect(result).toMatchObject({ error: expect.stringContaining('Remote font URL') });
  });
});
