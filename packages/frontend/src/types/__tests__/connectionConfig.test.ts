import { describe, it, expect } from 'vitest';
import type { ConnectionConfig } from '../index';

describe('ConnectionConfig options', () => {
  it('round-trips opaque options through JSON', () => {
    const config = {
      id: 'c1',
      name: 'Redis Cluster',
      databaseType: 'redis',
      sslMode: 'disable',
      options: {
        topology: 'cluster',
        clusterNodes: ['127.0.0.1:7000'],
        tls: { enabled: true },
      },
    } as ConnectionConfig;

    const parsed = JSON.parse(JSON.stringify(config)) as ConnectionConfig;
    expect(parsed.options?.topology).toBe('cluster');
    expect(parsed.options?.clusterNodes).toEqual(['127.0.0.1:7000']);
    expect((parsed.options?.tls as { enabled?: boolean })?.enabled).toBe(true);
  });
});
