import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  applyThemePack,
  clearThemePack,
  clearThemePackDom,
  injectThemePackCss,
  syncWebviewBackgroundFromTokens,
} from '../themePackApply';
import { parsePackEditorOverlay } from '../themeEditorColors';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
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

describe('applyThemePack with legacy packId', () => {
  beforeEach(() => {
    clearThemePackDom();
  });

  it('returns error for non-plugin pack ids', async () => {
    const result = await applyThemePack('classic-pack');
    expect(result).toEqual({ ok: false, error: 'unknown theme pack: classic-pack' });
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
