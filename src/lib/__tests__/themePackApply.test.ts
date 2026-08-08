import { describe, expect, it, beforeEach } from 'vitest';
import { injectThemePackCss, clearThemePackDom, rewriteFontUrls } from '../themePackApply';
import { parsePackEditorOverlay } from '../themeEditorColors';

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
