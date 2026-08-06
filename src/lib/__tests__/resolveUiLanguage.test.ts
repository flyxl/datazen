import { describe, expect, it } from 'vitest';
import { resolveUiLanguage, SUPPORTED_UI_LANGUAGES } from '../resolveUiLanguage';

describe('resolveUiLanguage', () => {
  it('falls back to en for empty or unknown locales', () => {
    expect(resolveUiLanguage('')).toBe('en');
    expect(resolveUiLanguage('   ')).toBe('en');
    expect(resolveUiLanguage('xx')).toBe('en');
    expect(resolveUiLanguage('it-IT')).toBe('en');
  });

  it('maps english variants', () => {
    expect(resolveUiLanguage('en')).toBe('en');
    expect(resolveUiLanguage('en-US')).toBe('en');
    expect(resolveUiLanguage('en_GB')).toBe('en');
  });

  it('maps chinese variants', () => {
    expect(resolveUiLanguage('zh-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hans-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-TW')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-Hant-TW')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-HK')).toBe('zh-TW');
  });

  it('maps european and asian languages', () => {
    expect(resolveUiLanguage('es-ES')).toBe('es');
    expect(resolveUiLanguage('fr-FR')).toBe('fr');
    expect(resolveUiLanguage('de-DE')).toBe('de');
    expect(resolveUiLanguage('ja-JP')).toBe('ja');
    expect(resolveUiLanguage('ko-KR')).toBe('ko');
    expect(resolveUiLanguage('ru-RU')).toBe('ru');
  });

  it('maps portuguese to pt-BR', () => {
    expect(resolveUiLanguage('pt')).toBe('pt-BR');
    expect(resolveUiLanguage('pt-BR')).toBe('pt-BR');
    expect(resolveUiLanguage('pt-PT')).toBe('pt-BR');
  });

  it('preserves exact supported codes', () => {
    for (const code of SUPPORTED_UI_LANGUAGES) {
      expect(resolveUiLanguage(code)).toBe(code);
    }
  });
});
