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

  it('maps chinese variants', () => {
    expect(resolveUiLanguage('zh-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh_CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hans')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-Hans-CN')).toBe('zh-CN');
    expect(resolveUiLanguage('zh')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-SG')).toBe('zh-CN');
    expect(resolveUiLanguage('zh-TW')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-Hant')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-Hant-TW')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-HK')).toBe('zh-TW');
    expect(resolveUiLanguage('zh-MO')).toBe('zh-TW');
  });

  it('maps european and asian languages', () => {
    expect(resolveUiLanguage('es')).toBe('es');
    expect(resolveUiLanguage('es-ES')).toBe('es');
    expect(resolveUiLanguage('es-MX')).toBe('es');
    expect(resolveUiLanguage('fr')).toBe('fr');
    expect(resolveUiLanguage('fr-FR')).toBe('fr');
    expect(resolveUiLanguage('fr-CA')).toBe('fr');
    expect(resolveUiLanguage('de')).toBe('de');
    expect(resolveUiLanguage('de-DE')).toBe('de');
    expect(resolveUiLanguage('de-AT')).toBe('de');
    expect(resolveUiLanguage('ja')).toBe('ja');
    expect(resolveUiLanguage('ja-JP')).toBe('ja');
    expect(resolveUiLanguage('ko')).toBe('ko');
    expect(resolveUiLanguage('ko-KR')).toBe('ko');
    expect(resolveUiLanguage('ru')).toBe('ru');
    expect(resolveUiLanguage('ru-RU')).toBe('ru');
  });

  it('maps portuguese to pt-BR', () => {
    expect(resolveUiLanguage('pt')).toBe('pt-BR');
    expect(resolveUiLanguage('pt-BR')).toBe('pt-BR');
    expect(resolveUiLanguage('pt_BR')).toBe('pt-BR');
    expect(resolveUiLanguage('pt-PT')).toBe('pt-BR');
    expect(resolveUiLanguage('PT-br')).toBe('pt-BR');
  });

  it('preserves exact supported codes', () => {
    for (const code of SUPPORTED_UI_LANGUAGES) {
      expect(resolveUiLanguage(code)).toBe(code);
    }
  });

  it('exposes exactly ten supported languages', () => {
    expect(SUPPORTED_UI_LANGUAGES).toHaveLength(10);
  });

  it('is case-insensitive for exact supported codes', () => {
    expect(resolveUiLanguage('KO')).toBe('ko');
    expect(resolveUiLanguage('JA')).toBe('ja');
    expect(resolveUiLanguage('RU')).toBe('ru');
  });
});
