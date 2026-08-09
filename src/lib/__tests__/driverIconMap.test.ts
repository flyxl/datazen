import { describe, expect, it } from 'vitest';
import { DRIVER_ICON_ENTRIES, DRIVER_ICON_PARENTS } from '../../plugins/generated';
import { getDriverIconMap, getDriverIconParents } from '../databaseTypes';

const BASIC_DRIVER_ICON_KEYS = [
  'db.postgresql',
  'db.mysql',
  'db.mariadb',
  'db.sqlite',
  'db.redis',
] as const;

describe('getDriverIconParents', () => {
  it('returns generated DRIVER_ICON_PARENTS', () => {
    expect(getDriverIconParents()).toEqual({ ...DRIVER_ICON_PARENTS });
  });
});

describe('getDriverIconMap', () => {
  it('returns generated DRIVER_ICON_ENTRIES', () => {
    expect(getDriverIconMap()).toEqual({ ...DRIVER_ICON_ENTRIES });
  });

  it('includes basic driver icons from active build', () => {
    const map = getDriverIconMap();

    for (const key of BASIC_DRIVER_ICON_KEYS) {
      expect(map[key], key).toBeTruthy();
    }
  });

  it('does not silently alias reuse types onto parent SVG paths', () => {
    const map = getDriverIconMap();
    const parents = getDriverIconParents();

    if (map['db.doris']) {
      expect(map['db.doris']).toMatch(/doris/i);
      expect(parents.doris).toBeUndefined();
    } else {
      expect(parents.doris).toBe('mysql');
      expect(map['db.doris']).toBeUndefined();
    }

    if (map['db.questdb']) {
      expect(map['db.questdb']).toMatch(/questdb/i);
      expect(parents.questdb).toBeUndefined();
    } else {
      expect(parents.questdb).toBe('postgresql');
      expect(map['db.questdb']).toBeUndefined();
    }
  });
});
