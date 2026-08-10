import { describe, expect, it } from 'vitest';
import type { TranslationKey } from '../../locales';
import {
  PRESET_GROUPS,
  PRESET_GROUP_OPTIONS,
  formatGroupLabel,
  normalizeGroupKey,
  normalizeGroupList,
} from '../connectionGroups';

describe('PRESET_GROUPS', () => {
  it('uses stable preset: keys', () => {
    expect(PRESET_GROUPS.production).toBe('preset:production');
    expect(PRESET_GROUPS.development).toBe('preset:development');
    expect(PRESET_GROUPS.testing).toBe('preset:testing');
  });

  it('exposes i18n options for all presets', () => {
    expect(PRESET_GROUP_OPTIONS.map((o) => o.key)).toEqual([
      PRESET_GROUPS.production,
      PRESET_GROUPS.development,
      PRESET_GROUPS.testing,
    ]);
    expect(PRESET_GROUP_OPTIONS.map((o) => o.i18nKey)).toEqual([
      'newConn.groupProd',
      'newConn.groupDev',
      'newConn.groupTest',
    ]);
  });
});

describe('normalizeGroupKey', () => {
  it('returns undefined for empty / nullish', () => {
    expect(normalizeGroupKey(undefined)).toBeUndefined();
    expect(normalizeGroupKey(null)).toBeUndefined();
    expect(normalizeGroupKey('')).toBeUndefined();
    expect(normalizeGroupKey('   ')).toBeUndefined();
  });

  it('maps Chinese literals to presets', () => {
    expect(normalizeGroupKey('生产环境')).toBe(PRESET_GROUPS.production);
    expect(normalizeGroupKey('开发环境')).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('测试环境')).toBe(PRESET_GROUPS.testing);
    expect(normalizeGroupKey('生產環境')).toBe(PRESET_GROUPS.production);
    expect(normalizeGroupKey('開發環境')).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('測試環境')).toBe(PRESET_GROUPS.testing);
  });

  it('maps English and locale defaultGroup strings', () => {
    expect(normalizeGroupKey('Production')).toBe(PRESET_GROUPS.production);
    expect(normalizeGroupKey('Development')).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('Testing')).toBe(PRESET_GROUPS.testing);
    expect(normalizeGroupKey('Entwicklung')).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('Desenvolvimento')).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('Разработка')).toBe(PRESET_GROUPS.development);
  });

  it('passes through preset keys and custom names', () => {
    expect(normalizeGroupKey(PRESET_GROUPS.development)).toBe(PRESET_GROUPS.development);
    expect(normalizeGroupKey('My Team')).toBe('My Team');
    expect(normalizeGroupKey('  staging  ')).toBe('staging');
  });
});

describe('formatGroupLabel', () => {
  const t = (k: TranslationKey) => `i18n:${k}`;

  it('localizes preset keys', () => {
    expect(formatGroupLabel(PRESET_GROUPS.production, t)).toBe('i18n:newConn.groupProd');
    expect(formatGroupLabel(PRESET_GROUPS.development, t)).toBe('i18n:newConn.groupDev');
    expect(formatGroupLabel(PRESET_GROUPS.testing, t)).toBe('i18n:newConn.groupTest');
  });

  it('returns custom keys as-is', () => {
    expect(formatGroupLabel('My Team', t)).toBe('My Team');
  });
});

describe('normalizeGroupList', () => {
  it('normalizes, dedupes, and preserves order', () => {
    expect(
      normalizeGroupList(['开发环境', 'My Team', 'Development', 'staging', 'preset:development']),
    ).toEqual([PRESET_GROUPS.development, 'My Team', 'staging']);
  });
});
