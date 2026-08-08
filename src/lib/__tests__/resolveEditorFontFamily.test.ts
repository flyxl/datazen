import { describe, expect, it } from 'vitest';
import { resolveEditorFontFamily, HOST_DEFAULT_EDITOR_FONT } from '../resolveEditorFontFamily';

describe('resolveEditorFontFamily', () => {
  it('prefers explicit user setting over theme', () => {
    expect(
      resolveEditorFontFamily('Comic Sans MS', '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT),
    ).toBe('Comic Sans MS');
  });

  it('uses theme when user setting is host default or empty', () => {
    expect(resolveEditorFontFamily(HOST_DEFAULT_EDITOR_FONT, '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT)).toBe(
      '"Theme Mono"',
    );
    expect(resolveEditorFontFamily('', '"Theme Mono"', HOST_DEFAULT_EDITOR_FONT)).toBe('"Theme Mono"');
  });

  it('falls back to host default when theme empty', () => {
    expect(resolveEditorFontFamily(HOST_DEFAULT_EDITOR_FONT, '', HOST_DEFAULT_EDITOR_FONT)).toBe(
      HOST_DEFAULT_EDITOR_FONT,
    );
  });
});
