import { describe, it, expect } from 'vitest';
import type { ConnectionConfig } from '../../types';
import {
  assertPlainConnectionConfig,
  buildConnectionConfig,
  coerceConnectionGroup,
} from '../connectionFormModel';
import { cloneConnectionConfigForIpc, toIpcConnectionConfig } from '../connectionConfig';

describe('connectionFormModel', () => {
  it('buildConnectionConfig produces JSON-serializable mysql/postgresql configs', () => {
    for (const databaseType of ['mysql', 'postgresql'] as const) {
      const config = buildConnectionConfig({
        newId: () => 'conn_test',
        unnamedLabel: 'Unnamed',
        name: 'Local',
        databaseType,
        host: '127.0.0.1',
        port: databaseType === 'mysql' ? '3306' : '5432',
        database: databaseType === 'mysql' ? 'app' : 'postgres',
        schema: 'default',
        username: databaseType === 'mysql' ? 'root' : 'postgres',
        password: '',
        sslMode: 'prefer',
        group: 'development',
        colorTag: '#3b82f6',
        readOnly: false,
        connectionOptions: {},
      });
      expect(() => JSON.stringify(config)).not.toThrow();
      assertPlainConnectionConfig(config);
    }
  });

  it('assertPlainConnectionConfig rejects ConnectionFormState mistaken for config', () => {
    const fakeForm = {
      id: 'x',
      name: 'x',
      databaseType: 'mysql',
      sslMode: 'prefer',
      setName: () => {},
    };
    expect(() => assertPlainConnectionConfig(fakeForm)).toThrow(/ConnectionFormState/);
  });

  it('assertPlainConnectionConfig rejects cyclic options', () => {
    const cyclic: Record<string, unknown> = {
      id: 'c1',
      name: 'x',
      databaseType: 'mysql',
      sslMode: 'prefer',
    };
    cyclic.self = cyclic;
    expect(() => assertPlainConnectionConfig(cyclic)).toThrow(/cycle/);
  });

  it('coerceConnectionGroup rejects non-string values', () => {
    expect(coerceConnectionGroup('preset:development')).toBe('preset:development');
    expect(coerceConnectionGroup('')).toBe('');
    expect(coerceConnectionGroup(undefined)).toBe('');
    expect(coerceConnectionGroup({ target: { __reactFiber: {} } })).toBe('');
  });

  it('buildConnectionConfig drops non-string group values', () => {
    const config = buildConnectionConfig({
      newId: () => 'conn_test',
      unnamedLabel: 'Unnamed',
      name: 'Local',
      databaseType: 'postgresql',
      host: '127.0.0.1',
      port: '5432',
      database: 'postgres',
      schema: 'default',
      username: 'postgres',
      password: '',
      sslMode: 'prefer',
      group: { target: document.createElement('button') } as unknown as string,
      colorTag: '#3b82f6',
      readOnly: false,
      connectionOptions: {},
    });
    expect(config.group).toBeUndefined();
    expect(() => JSON.stringify(config)).not.toThrow();
  });
});

describe('connectionConfig IPC boundary', () => {
  it('cloneConnectionConfigForIpc clones nested ssh jump and options', () => {
    const source: ConnectionConfig = {
      id: 'c3',
      name: 'Bastion PG',
      databaseType: 'postgresql',
      host: '127.0.0.1',
      port: 5432,
      sslMode: 'require',
      sshTunnel: {
        enabled: true,
        host: 'bastion',
        port: 22,
        username: 'ops',
        authMethod: 'password',
        password: 'secret',
        jump: {
          enabled: true,
          host: 'jump',
          port: 22,
          username: 'jump-user',
          authMethod: 'agent',
        },
      },
      options: { foo: 'bar', nested: { n: 1 } },
    };

    const cloned = cloneConnectionConfigForIpc(source);
    expect(cloned.sshTunnel?.jump?.host).toBe('jump');
    expect(cloned.options).toEqual({ foo: 'bar', nested: { n: 1 } });
    expect(cloned).not.toBe(source);
  });

  it('toIpcConnectionConfig validates then clones', () => {
    const config: ConnectionConfig = {
      id: 'c1',
      name: 'Test',
      databaseType: 'postgresql',
      sslMode: 'prefer',
    };
    expect(toIpcConnectionConfig(config)).toEqual(config);
  });
});
