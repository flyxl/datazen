import { describe, expect, it } from 'vitest';
import { DRIVER_ICON_ENTRIES } from '../../plugins/generated';
import { getDriverIconMap } from '../databaseTypes';

const BASIC_DRIVER_ICON_KEYS = [
  'db.postgresql',
  'db.mysql',
  'db.mariadb',
  'db.sqlite',
  'db.redis',
] as const;

describe('getDriverIconMap', () => {
  it('returns generated DRIVER_ICON_ENTRIES', () => {
    expect(getDriverIconMap()).toEqual({ ...DRIVER_ICON_ENTRIES });
  });

  it('includes basic driver icons and protocol alias reuse from active build', () => {
    const map = getDriverIconMap();

    for (const key of BASIC_DRIVER_ICON_KEYS) {
      expect(map[key], key).toBeTruthy();
    }

    expect(map['db.doris']).toMatch(/mysql/i);
    expect(map['db.questdb']).toMatch(/postgresql/i);
  });
});
