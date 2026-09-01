import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import { PRESET_GROUPS } from '../../../lib/connectionGroups';
import { buildConnectionConfig } from '../../../lib/connectionFormModel';
import { useConnectionForm, type ConnectionFormState } from '../useConnectionForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const { testConnectionMock, saveConnectionMock } = vi.hoisted(() => ({
  testConnectionMock: vi
    .fn()
    .mockResolvedValue({ serverVersion: '16.0', serverType: 'postgresql' }),
  saveConnectionMock: vi.fn(),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    testConnection: testConnectionMock,
    saveConnection: saveConnectionMock,
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: typeof saveConnectionMock }) => unknown) =>
      selector({ saveConnection: saveConnectionMock }),
    ),
    { getState: () => ({ saveConnection: saveConnectionMock }) },
  ),
}));

function previewConfig(form: ConnectionFormState, editId?: string | null) {
  return buildConnectionConfig({
    editId,
    newId: () => 'conn_test',
    unnamedLabel: 'newConn.unnamed',
    name: form.name,
    databaseType: form.databaseType,
    host: form.host,
    port: form.port,
    database: form.database,
    schema: form.schema,
    username: form.username,
    password: form.password,
    sslMode: form.sslMode,
    group: form.group,
    colorTag: form.colorTag,
    readOnly: form.readOnly,
    connectionOptions: form.options,
    sshTunnel: form.sshEnabled
      ? {
          enabled: true,
          host: form.sshHost,
          port: Number(form.sshPort) || 22,
          username: form.sshUsername,
          authMethod: form.sshAuthMethod,
          password: form.sshAuthMethod === 'password' ? form.sshPassword || undefined : undefined,
          privateKeyPath:
            form.sshAuthMethod === 'private_key' ? form.sshKeyPath || undefined : undefined,
          passphrase:
            form.sshAuthMethod === 'private_key' ? form.sshPassphrase || undefined : undefined,
          jump: form.sshJumpEnabled
            ? {
                enabled: true,
                host: form.sshJumpHost,
                port: Number(form.sshJumpPort) || 22,
                username: form.sshJumpUsername,
                authMethod: form.sshJumpAuthMethod,
                password:
                  form.sshJumpAuthMethod === 'password'
                    ? form.sshJumpPassword || undefined
                    : undefined,
                privateKeyPath:
                  form.sshJumpAuthMethod === 'private_key'
                    ? form.sshJumpKeyPath || undefined
                    : undefined,
                passphrase:
                  form.sshJumpAuthMethod === 'private_key'
                    ? form.sshJumpPassphrase || undefined
                    : undefined,
              }
            : undefined,
        }
      : undefined,
  });
}

describe('useConnectionForm', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    testConnectionMock.mockResolvedValue({ serverVersion: '16.0', serverType: 'postgresql' });
  });

  it('does not expose draft or meta on the form API surface', () => {
    const { result } = renderHook(() => useConnectionForm());
    expect('draft' in result.current).toBe(false);
    expect('meta' in result.current).toBe(false);
    expect(result.current.supportsSSH).toBe(true);
    expect(result.current.supportsSSL).toBe(true);
  });

  it('defaults group to stable development preset key', () => {
    const { result } = renderHook(() => useConnectionForm());
    expect(result.current.group).toBe(PRESET_GROUPS.development);
    expect(previewConfig(result.current).group).toBe(PRESET_GROUPS.development);
  });

  it('uses defaultGroup when creating a new connection', () => {
    const { result } = renderHook(() => useConnectionForm({ defaultGroup: 'prod' }));
    expect(result.current.group).toBe('prod');
    expect(previewConfig(result.current).group).toBe('prod');
  });

  it('keeps stored group name as-is when loading an existing connection', () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c1',
        existingConnections: [
          {
            id: 'c1',
            name: 'legacy',
            databaseType: 'postgresql',
            host: '127.0.0.1',
            port: 5432,
            sslMode: 'prefer',
            group: '开发环境',
          },
        ],
      }),
    );
    expect(result.current.group).toBe('开发环境');
    expect(previewConfig(result.current, 'c1').group).toBe('开发环境');
  });

  it('detects formVariant from DB_REGISTRY', () => {
    const { result, rerender } = renderHook(() => useConnectionForm());

    expect(result.current.formVariant).toBe('standard');

    if (DB_REGISTRY.redis) {
      act(() => result.current.handleDatabaseTypeChange('redis'));
      rerender();
      expect(result.current.formVariant).toBe('redis');
      expect(DB_REGISTRY.redis.connectionForm).toBe('redis');
    }

    act(() => result.current.handleDatabaseTypeChange('sqlite'));
    rerender();
    expect(result.current.formVariant).toBe('file');
    expect(DB_REGISTRY.sqlite.connectionForm).toBe('file');

    if (DB_REGISTRY.kiwi) {
      act(() => result.current.handleDatabaseTypeChange('kiwi'));
      rerender();
      expect(result.current.formVariant).toBe('kiwi');
      expect(DB_REGISTRY.kiwi.connectionForm).toBe('kiwi');
    }
  });

  it('resets database and username when switching types (BUG-001)', () => {
    const { result } = renderHook(() => useConnectionForm());

    expect(result.current.database).toBe('postgres');
    expect(result.current.username).toBe('postgres');

    act(() => result.current.setName('My PG Name'));

    act(() => result.current.handleDatabaseTypeChange('mysql'));
    expect(result.current.database).toBe('');
    expect(result.current.username).toBe('root');
    expect(result.current.port).toBe('3306');
    expect(result.current.name).toBe('');

    act(() => {
      result.current.setName('My MySQL Name');
      result.current.setHost('mysql.example.com');
    });

    act(() => result.current.handleDatabaseTypeChange('sqlite'));
    expect(result.current.database).toBe('');
    expect(result.current.username).toBe('');
    expect(result.current.name).toBe('');

    act(() => result.current.handleDatabaseTypeChange('mysql'));
    expect(result.current.name).toBe('My MySQL Name');
    expect(result.current.host).toBe('mysql.example.com');
    expect(result.current.port).toBe('3306');

    act(() => result.current.handleDatabaseTypeChange('postgresql'));
    expect(result.current.name).toBe('My PG Name');
    expect(result.current.database).toBe('postgres');
    expect(result.current.username).toBe('postgres');
    expect(result.current.port).toBe('5432');

    if (DB_REGISTRY.redis) {
      act(() => result.current.handleDatabaseTypeChange('redis'));
      expect(result.current.database).toBe('0');
      expect(result.current.sslMode).toBe('disable');
    }

    act(() => result.current.handleDatabaseTypeChange('postgresql'));
    expect(result.current.database).toBe('postgres');
    expect(result.current.username).toBe('postgres');
    expect(result.current.port).toBe('5432');
    expect(result.current.name).toBe('My PG Name');
  });

  it('normalizes legacy redis prefer SSL to disabled when TLS is unchecked', () => {
    if (!DB_REGISTRY.redis) return;

    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'redis-old',
        existingConnections: [
          {
            id: 'redis-old',
            name: 'redis-old',
            databaseType: 'redis',
            host: '127.0.0.1',
            port: 6379,
            database: '0',
            sslMode: 'prefer',
            options: { topology: 'standalone' },
          },
        ],
      }),
    );

    expect(result.current.sslMode).toBe('prefer');
    expect(previewConfig(result.current, 'redis-old').sslMode).toBe('disable');
  });

  it('includes username in kiwi connection config', () => {
    if (!DB_REGISTRY.kiwi) return;

    const { result } = renderHook(() => useConnectionForm());

    act(() => {
      result.current.handleDatabaseTypeChange('kiwi');
      result.current.setHost('https://kiwi.example.com');
      result.current.setUsername('kiwi-user');
      result.current.setPassword('secret');
      result.current.setDatabase('instance.example.com');
    });

    const config = previewConfig(result.current);
    expect(config.databaseType).toBe('kiwi');
    expect(config.username).toBe('kiwi-user');
    expect(result.current.hasUsername).toBe(true);
  });

  it('ignores mistaken DOM event passed to setGroup', async () => {
    const { result } = renderHook(() => useConnectionForm());

    act(() => {
      result.current.setGroup({
        target: document.createElement('select'),
      } as unknown as string);
    });

    expect(result.current.group).toBe('');
    expect(previewConfig(result.current).group).toBeUndefined();

    act(() => {
      result.current.setName('Test PG');
      result.current.setHost('127.0.0.1');
      result.current.setPort('5432');
    });

    await act(async () => {
      await result.current.onTest();
    });

    expect(testConnectionMock).toHaveBeenCalledTimes(1);
    const [config] = testConnectionMock.mock.calls[0] as [Record<string, unknown>];
    expect(() => JSON.stringify(config)).not.toThrow();
  });

  it('onTest passes JSON-serializable config to testConnection', async () => {
    const { result } = renderHook(() => useConnectionForm());

    act(() => {
      result.current.setName('Test PG');
      result.current.setHost('127.0.0.1');
      result.current.setPort('5432');
    });

    await act(async () => {
      await result.current.onTest();
    });

    expect(testConnectionMock).toHaveBeenCalledTimes(1);
    const [config] = testConnectionMock.mock.calls[0] as [{ databaseType: string; name: string }];
    expect(config.databaseType).toBe('postgresql');
    expect(config.name).toBe('Test PG');
    expect(() => JSON.stringify({ config })).not.toThrow();
    expect('setName' in config).toBe(false);
  });

  it('buildConnectionConfig from form fields is JSON-serializable for mysql/postgresql', () => {
    const { result } = renderHook(() => useConnectionForm());

    act(() => {
      result.current.setName('Test');
      result.current.handleDatabaseTypeChange('mysql');
    });
    expect(() => JSON.stringify(previewConfig(result.current))).not.toThrow();

    act(() => result.current.handleDatabaseTypeChange('postgresql'));
    expect(() => JSON.stringify(previewConfig(result.current))).not.toThrow();
  });

  it('builds ssh agent + jump tunnel into connection config', () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setSshEnabled(true);
      result.current.setSshHost('bastion');
      result.current.setSshPort('22');
      result.current.setSshUsername('ops');
      result.current.setSshAuthMethod('agent');
      result.current.setSshJumpEnabled(true);
      result.current.setSshJumpHost('inner');
      result.current.setSshJumpUsername('db');
      result.current.setReadOnly(true);
    });
    const config = previewConfig(result.current);
    expect(config.sshTunnel?.authMethod).toBe('agent');
    expect(config.sshTunnel?.jump?.host).toBe('inner');
    expect(config.readOnly).toBe(true);
  });
});
