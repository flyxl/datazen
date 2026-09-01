import { describe, expect, it } from 'vitest';
import { namespaceLeafContext } from '../utils';

describe('namespaceLeafContext', () => {
  it('maps Superset-style segments to driver fetch path', () => {
    expect(
      namespaceLeafContext(['presto_afi_data', 'hive', 'snap', 'orders'], {
        presto_afi_data: '558',
      }),
    ).toEqual({
      tableName: 'orders',
      schema: 'snap',
      database: '558/hive/snap',
    });
  });

  it('returns table name only for single-segment leaves', () => {
    expect(namespaceLeafContext(['users'], {})).toEqual({ tableName: 'users' });
  });
});
