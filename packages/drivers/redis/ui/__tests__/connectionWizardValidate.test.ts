import { describe, expect, it } from 'vitest';
import { validateRedisConnection } from '../connectionWizardValidate';

const t = (key: string) => key;

describe('validateRedisConnection', () => {
  it('requires host and port for standalone topology', () => {
    const errors = validateRedisConnection(
      {
        host: '',
        port: '',
        database: '0',
        username: '',
        password: '',
        schema: '',
        options: { topology: 'standalone' },
      },
      t,
    );
    expect(errors.host).toBe('newConn.required');
    expect(errors.port).toBe('newConn.required');
  });

  it('requires sentinel master name and at least one sentinel node', () => {
    const missingMaster = validateRedisConnection(
      {
        host: '127.0.0.1',
        port: '26379',
        database: '0',
        username: '',
        password: '',
        schema: '',
        options: {
          topology: 'sentinel',
          sentinelNodes: ['127.0.0.1:26379'],
        },
      },
      t,
    );
    expect(missingMaster.sentinelMasterName).toBe('redis.wizard.sentinelMasterRequired');

    const missingNodes = validateRedisConnection(
      {
        host: '127.0.0.1',
        port: '26379',
        database: '0',
        username: '',
        password: '',
        schema: '',
        options: {
          topology: 'sentinel',
          sentinelMasterName: 'mymaster',
        },
      },
      t,
    );
    expect(missingNodes.sentinelNodes).toBe('redis.wizard.sentinelNodesRequired');
  });

  it('accepts valid sentinel configuration', () => {
    const errors = validateRedisConnection(
      {
        host: '127.0.0.1',
        port: '26379',
        database: '0',
        username: '',
        password: '',
        schema: '',
        options: {
          topology: 'sentinel',
          sentinelMasterName: 'mymaster',
          sentinelNodes: ['127.0.0.1:26379'],
        },
      },
      t,
    );
    expect(errors).toEqual({});
  });

  it('requires cluster nodes or host/port fallback', () => {
    const errors = validateRedisConnection(
      {
        host: '',
        port: '',
        database: '0',
        username: '',
        password: '',
        schema: '',
        options: {
          topology: 'cluster',
          clusterNodes: [],
        },
      },
      t,
    );
    expect(errors.host).toBe('newConn.required');
    expect(errors.port).toBe('newConn.required');
  });
});
