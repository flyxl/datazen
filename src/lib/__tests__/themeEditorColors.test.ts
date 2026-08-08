import { describe, expect, it } from 'vitest';
import { editorColorsFromJson, readEditorColors } from '../themeEditorColors';

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
});
