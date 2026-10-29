import { describe, expect, it } from 'vitest';
import type { TranslationKey } from '../../locales';
import {
  PRESET_GROUPS,
  PRESET_GROUP_OPTIONS,
  formatGroupLabel,
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
