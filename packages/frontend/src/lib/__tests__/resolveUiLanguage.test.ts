import { describe, expect, it } from 'vitest';
import { resolveUiLanguage, SUPPORTED_UI_LANGUAGES } from '../resolveUiLanguage';

describe('resolveUiLanguage', () => {
  it('falls back to en for empty or unknown locales', () => {
    expect(resolveUiLanguage('')).toBe('en');
    expect(resolveUiLanguage('   ')).toBe('en');
    expect(resolveUiLanguage('xx')).toBe('en');
    expect(resolveUiLanguage('it-IT')).toBe('en');
    expect(resolveUiLanguage('nl-NL')).toBe('en');
    expect(resolveUiLanguage('ar-SA')).toBe('en');
    expect(resolveUiLanguage('hi-IN')).toBe('en');
  });

  it('maps english variants', () => {
    expect(resolveUiLanguage('en')).toBe('en');
    expect(resolveUiLanguage('en-US')).toBe('en');
    expect(resolveUiLanguage('en_GB')).toBe('en');
    expect(resolveUiLanguage('EN-au')).toBe('en');
    expect(resolveUiLanguage('  en-IN  ')).toBe('en');
  });

  it('maps Chinese variants to Simplified Chinese', () => {
    expect(resolveUiLanguage('zh-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh_CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hans')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hans-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-SG')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-TW')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hant')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-HK')).toBe('zh-CN');
  });

  it('falls back to English for every other language', () => {
    for (const locale of ['es-ES', 'fr-FR', 'de-DE', 'ja-JP', 'ko-KR', 'ru-RU', 'pt-BR']) {
      expect(resolveUiLanguage(locale), locale).toBe('en');
    }
  });

  it('preserves exact supported codes', () => {
    for (const code of SUPPORTED_UI_LANGUAGES) {
      expect(resolveUiLanguage(code)).toBe(code);
    }
  });

  it('exposes only English and Simplified Chinese', () => {
    expect(SUPPORTED_UI_LANGUAGES).toEqual(['en', 'zh-CN']);
  });

  it('is case-insensitive for supported codes', () => {
    expect(resolveUiLanguage('EN')).toBe('en');
    expect(resolveUiLanguage('ZH-CN')).toBe('zh-CN');
  });
});
