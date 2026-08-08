import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  applyThemePack,
  clearThemePack,
  clearThemePackDom,
  injectThemePackCss,
  rewriteFontUrls,
} from '../themePackApply';
import { parsePackEditorOverlay } from '../themeEditorColors';

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
    await applyThemePack(null);
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
  it('rejects remote http font URLs', async () => {
    const css = '@font-face { src: url("https://evil.example/font.woff2"); }';
    await expect(rewriteFontUrls(css, 'pack-1')).rejects.toThrow(/Remote font URL not allowed/);
  });
});
