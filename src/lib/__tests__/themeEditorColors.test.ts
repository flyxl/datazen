import { describe, expect, it } from 'vitest';
import {
  buildEditorHighlightStyle,
  editorColorsFromJson,
  editorSyntaxHighlighting,
  readEditorColors,
  readEditorColorsFromElement,
  setPackEditorColorOverlay,
  applySqlSyntaxPreset,
  SQL_SYNTAX_PRESETS,
  isSqlSyntaxThemeId,
  type EditorColorContract,
} from '../themeEditorColors';

const BASE_DARK: EditorColorContract = {
  keyword: '#93c5fd',
  string: '#6ee7b7',
  number: '#fbbf24',
  comment: '#9ca3af',
  operator: '#67e8f9',
  punctuation: '#d1d5db',
  foreground: '#f3f4f6',
  background: '#0b1220',
  selection: 'rgba(59, 130, 246, 0.32)',
  cursor: '#f3f4f6',
};

const BASE_LIGHT: EditorColorContract = {
  keyword: '#2563eb',
  string: '#16a34a',
  number: '#d97706',
  comment: '#6b7280',
  operator: '#0891b2',
  punctuation: '#374151',
  foreground: '#111827',
  background: '#ffffff',
  selection: 'rgba(59, 130, 246, 0.16)',
  cursor: '#111827',
};

describe('readEditorColors', () => {
  it('reads cm vars with fallbacks', () => {
    const colors = readEditorColors((name) => (name === '--cm-keyword' ? ' #c678dd ' : ''));
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

describe('SQL syntax presets', () => {
  it('has at least the default preset', () => {
    expect(SQL_SYNTAX_PRESETS.length).toBeGreaterThanOrEqual(1);
    expect(SQL_SYNTAX_PRESETS[0].id).toBe('default');
  });

  it('every preset has both dark and light palettes', () => {
    for (const preset of SQL_SYNTAX_PRESETS) {
      expect(preset.dark).toBeDefined();
      expect(preset.light).toBeDefined();
      expect(preset.dark.keyword).toBeTruthy();
      expect(preset.light.keyword).toBeTruthy();
    }
  });

  it('isSqlSyntaxThemeId validates known ids', () => {
    expect(isSqlSyntaxThemeId('default')).toBe(true);
    expect(isSqlSyntaxThemeId('monokai')).toBe(true);
    expect(isSqlSyntaxThemeId('nonexistent')).toBe(false);
  });
});

describe('applySqlSyntaxPreset', () => {
  it('returns base colors with default extended accents when themeId is undefined', () => {
    const result = applySqlSyntaxPreset(BASE_DARK, undefined, true);
    expect(result.keyword).toBe(BASE_DARK.keyword);
    expect(result.typeName).toBe('#e5c07b'); // EXTENDED_DARK.typeName
    expect(result.propertyName).toBe('#61afef');
  });

  it('returns base colors when themeId is "default"', () => {
    const result = applySqlSyntaxPreset(BASE_LIGHT, 'default', false);
    expect(result.keyword).toBe(BASE_LIGHT.keyword);
    expect(result.typeName).toBe('#b45309'); // EXTENDED_LIGHT.typeName
  });

  it('overrides colors for a non-default preset (dark)', () => {
    const result = applySqlSyntaxPreset(BASE_DARK, 'monokai', true);
    expect(result.keyword).toBe('#f92672');
    expect(result.string).toBe('#e6db74');
    expect(result.number).toBe('#ae81ff');
    expect(result.comment).toBe('#75715e');
    // Extended colors also come from the preset
    expect(result.typeName).toBe('#66d9ef');
    expect(result.propertyName).toBe('#a6e22e');
  });

  it('applies light variant for light mode', () => {
    const result = applySqlSyntaxPreset(BASE_LIGHT, 'github-dark', false);
    // github-dark light palette
    expect(result.keyword).toBe('#cf222e');
    expect(result.string).toBe('#0a3069');
  });

  it('preserves non-overridden base colors (cursor, etc.)', () => {
    const result = applySqlSyntaxPreset(BASE_DARK, 'dracula', true);
    expect(result.cursor).toBe(BASE_DARK.cursor);
    // background comes from preset (Dracula dark has its own background)
    expect(result.background).toBe('#282a36');
  });

  it('produces valid highlight styles for every preset', () => {
    for (const preset of SQL_SYNTAX_PRESETS) {
      const darkColors = applySqlSyntaxPreset(BASE_DARK, preset.id, true);
      const lightColors = applySqlSyntaxPreset(BASE_LIGHT, preset.id, false);
      expect(buildEditorHighlightStyle(darkColors, true)).toBeTruthy();
      expect(buildEditorHighlightStyle(lightColors, false)).toBeTruthy();
    }
  });
});
