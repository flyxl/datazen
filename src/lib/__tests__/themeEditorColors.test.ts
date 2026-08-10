import { describe, expect, it } from 'vitest';
import {
  buildEditorHighlightStyle,
  editorColorsFromJson,
  editorSyntaxHighlighting,
  readEditorColors,
  readEditorColorsFromElement,
  setPackEditorColorOverlay,
} from '../themeEditorColors';

describe('readEditorColors', () => {
  it('reads cm vars with fallbacks', () => {
    const colors = readEditorColors((name) =>
      name === '--cm-keyword' ? ' #c678dd ' : '',
    );
    expect(colors.keyword).toBe('#c678dd');
    expect(colors.string.length).toBeGreaterThan(0); // fallback hex
  });
});

describe('editorColorsFromJson', () => {
  it('overlays pack editor.json keys', () => {
    const base = readEditorColors(() => '');
    const next = editorColorsFromJson({ keyword: '#ff00ff' }, base);
    expect(next.keyword).toBe('#ff00ff');
    expect(next.string).toBe(base.string);
  });

  it('returns base when json is invalid', () => {
    const base = readEditorColors(() => '');
    expect(editorColorsFromJson(null, base)).toEqual(base);
  });
});

describe('readEditorColorsFromElement with pack overlay', () => {
  it('merges pack overlay onto computed colors', () => {
    document.documentElement.style.setProperty('--cm-keyword', '#111111');
    setPackEditorColorOverlay({ string: '#222222' });
    const colors = readEditorColorsFromElement();
    expect(colors.keyword).toBe('#111111');
    expect(colors.string).toBe('#222222');
    setPackEditorColorOverlay(null);
  });
});

describe('buildEditorHighlightStyle', () => {
  it('builds dark and light highlight styles', () => {
    const colors = readEditorColors(() => '');
    const dark = buildEditorHighlightStyle(colors, true);
    const light = buildEditorHighlightStyle(colors, false);
    expect(dark).toBeTruthy();
    expect(light).toBeTruthy();
    expect(editorSyntaxHighlighting(colors, true)).toBeTruthy();
  });
});
