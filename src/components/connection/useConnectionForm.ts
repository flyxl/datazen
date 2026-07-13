import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY, normalizeRedisDatabaseField } from '../../lib/databaseTypes';
import { newId } from './shared';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../../types';

export interface UseConnectionFormOptions {
  editId?: string | null;
  existingConnections?: ConnectionConfig[];
  onAfterSave?: () => void;
}

export function useConnectionForm(options: UseConnectionFormOptions = {}) {
  const { editId, existingConnections, onAfterSave } = options;
  const { t } = useI18n();
  const saveConnection = useConnectionStore((s) => s.saveConnection);

  const [name, setName] = useState('');
  const [databaseType, setDatabaseType] = useState<DatabaseType>('postgresql');
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('5432');
  const [database, setDatabase] = useState('postgres');
  const [username, setUsername] = useState('postgres');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroup] = useState<string>(() => t('newConn.defaultGroup'));
  const [colorTag, setColorTag] = useState<string>('#3b82f6');

  const [showAdvanced, setShowAdvanced] = useState(false);

  const [sshEnabled, setSshEnabled] = useState(false);
  const [sshHost, setSshHost] = useState('');
  const [sshPort, setSshPort] = useState('22');
  const [sshUsername, setSshUsername] = useState('');
  const [sshAuthMethod, setSshAuthMethod] = useState<'password' | 'private_key'>('password');
  const [sshPassword, setSshPassword] = useState('');
  const [sshKeyPath, setSshKeyPath] = useState('');
  const [sshPassphrase, setSshPassphrase] = useState('');

  const [testing, setTesting] = useState(false);
  const [testOk, setTestOk] = useState<string | null>(null);
  const [testErr, setTestErr] = useState<string | null>(null);
  const testResultRef = useRef<HTMLDivElement>(null);

  const [kiwiInstances, setKiwiInstances] = useState<{ name: string; alias: string; short: string }[]>([]);
  const [loadingInstances, setLoadingInstances] = useState(false);
  const [kiwiToken, setKiwiToken] = useState('');
  const [kiwiLoggingIn, setKiwiLoggingIn] = useState(false);

  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!editId || loaded || !existingConnections?.length) return;
    const existing = existingConnections.find((c) => c.id === editId);
    if (!existing) return;
    setDatabaseType(existing.databaseType);
    setName(existing.name);
    setHost(existing.host ?? '127.0.0.1');
    setPort(String(existing.port ?? (DB_REGISTRY[existing.databaseType].defaultPort || '')));
    setDatabase(existing.database ?? '');
    setUsername(existing.username ?? '');
    setPassword(existing.password ?? '');
    setSslMode(existing.sslMode);
    setGroup(existing.group ?? '');
    setColorTag(existing.colorTag ?? '#3b82f6');
    if (existing.sshTunnel?.enabled) {
      setSshEnabled(true);
      setSshHost(existing.sshTunnel.host);
      setSshPort(String(existing.sshTunnel.port));
      setSshUsername(existing.sshTunnel.username);
      setSshAuthMethod(existing.sshTunnel.authMethod);
      setSshPassword(existing.sshTunnel.password ?? '');
      setSshKeyPath(existing.sshTunnel.privateKeyPath ?? '');
      setSshPassphrase(existing.sshTunnel.passphrase ?? '');
    }
    setShowAdvanced(true);
    setLoaded(true);
  }, [editId, loaded, existingConnections]);

  const tabFill = useCallback(
    (setter: (v: string) => void) =>
      (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Tab' && !e.currentTarget.value && e.currentTarget.placeholder) {
          e.preventDefault();
          setter(e.currentTarget.placeholder);
        }
      },
    [],
  );

  function handleDatabaseTypeChange(newType: DatabaseType) {
    setDatabaseType(newType);
    const meta = DB_REGISTRY[newType];
    setPort(meta.defaultPort ? String(meta.defaultPort) : '');
    if (!meta.supportsSSH) setSshEnabled(false);
    if (meta.databaseFieldType === 'index') setDatabase('0');
    if (!meta.defaultUser) setUsername('');
    if (meta.connectionForm === 'kiwi') {
      setHost(meta.defaultHost);
      setDatabase('');
      setPort(String(meta.defaultPort));
    }
  }

  async function loadKiwiInstances(baseUrl: string, token: string) {
    setLoadingInstances(true);
    try {
      const data = await (window as any).__TAURI_INTERNALS__?.invoke('kiwi_list_instances', {
        baseUrl,
        token,
        sourceType: Number(port) || 4,
        userName: username || undefined,
      });
      if (data?.code === 0 && Array.isArray(data.result)) {
        setKiwiInstances(data.result.map((r: any) => ({
          name: r.name,
          alias: r.alias_name || '',
          short: r.short_domain || '',
        })));
        if (data.result.length === 0) {
          setTestErr('未找到任何实例');
        }
      } else {
        setTestErr(`加载实例失败: ${data?.msg || JSON.stringify(data)}`);
      }
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || JSON.stringify(e);
      setTestErr(`加载实例出错: ${msg}`);
    } finally {
      setLoadingInstances(false);
    }
  }

  async function handleKiwiLogin() {
    if (!host || !username || !password) return;
    setKiwiLoggingIn(true);
    setTestErr(null);
    setTestOk(null);
    try {
      const result = await (window as any).__TAURI_INTERNALS__?.invoke('kiwi_login', {
        baseUrl: host,
        username,
        password,
      });
      if (result?.token) {
        setKiwiToken(result.token);
        setTestOk(`登录成功 (${result.username})`);
        void loadKiwiInstances(host, result.token);
      } else {
        setTestErr('登录失败：未获取到 token');
      }
    } catch (e) {
      setTestErr(typeof e === 'string' ? e : e instanceof Error ? e.message : '登录失败');
    } finally {
      setKiwiLoggingIn(false);
    }
  }

  const sshTunnel: SshTunnelConfig | undefined = sshEnabled
    ? {
        enabled: true,
        host: sshHost,
        port: Number(sshPort) || 22,
        username: sshUsername,
        authMethod: sshAuthMethod,
        password: sshAuthMethod === 'password' ? sshPassword || undefined : undefined,
        privateKeyPath: sshAuthMethod === 'private_key' ? sshKeyPath || undefined : undefined,
        passphrase: sshAuthMethod === 'private_key' ? sshPassphrase || undefined : undefined,
      }
    : undefined;

  const meta = DB_REGISTRY[databaseType];
  const formVariant = meta.connectionForm;
  const hasUsername = !!meta.defaultUser || formVariant === 'kiwi';

  const draft = useMemo((): ConnectionConfig => {
    const base: ConnectionConfig = {
      id: editId ?? newId(),
      name: name || t('newConn.unnamed'),
      databaseType,
      sslMode,
      group: group || undefined,
      colorTag: colorTag || undefined,
      sshTunnel,
    };

    const draftMeta = DB_REGISTRY[databaseType];
    if (draftMeta.connectionMode === 'file') {
      return { ...base, database };
    }

    const conn: ConnectionConfig = {
      ...base,
      host: host || draftMeta.defaultHost || undefined,
      port: Number(port) || draftMeta.defaultPort || undefined,
      database: draftMeta.databaseFieldType === 'index' ? normalizeRedisDatabaseField(database) : database || undefined,
      password: password || undefined,
    };
    if (draftMeta.defaultUser || draftMeta.connectionForm === 'kiwi') {
      conn.username = username || draftMeta.defaultUser || undefined;
    }
    return conn;
  }, [colorTag, database, databaseType, editId, group, host, name, password, port, sslMode, sshTunnel, t, username]);

  async function onTest() {
    setTesting(true);
    setTestOk(null);
    setTestErr(null);
    try {
      const info = await useConnectionStore.getState().testConnection(draft);
      setTestOk(info.serverVersion);
    } catch (e) {
      setTestErr(typeof e === 'string' ? e : e instanceof Error ? e.message : t('newConn.testFailed'));
    } finally {
      setTesting(false);
      setTimeout(() => {
        testResultRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
      }, 50);
    }
  }

  async function onSave() {
    await saveConnection(draft);
    onAfterSave?.();
  }

  const sslOptions = useMemo(
    () => [
      { value: 'disable', label: `Disable — ${t('newConn.sslNone')}` },
      { value: 'prefer', label: `Prefer — ${t('newConn.sslPrefer')}` },
      { value: 'require', label: `Require — ${t('newConn.sslRequire')}` },
    ],
    [t],
  );

  return {
    name,
    setName,
    databaseType,
    setDatabaseType,
    host,
    setHost,
    port,
    setPort,
    database,
    setDatabase,
    username,
    setUsername,
    password,
    setPassword,
    sslMode,
    setSslMode,
    group,
    setGroup,
    colorTag,
    setColorTag,
    sshEnabled,
    setSshEnabled,
    sshHost,
    setSshHost,
    sshPort,
    setSshPort,
    sshUsername,
    setSshUsername,
    sshAuthMethod,
    setSshAuthMethod,
    sshPassword,
    setSshPassword,
    sshKeyPath,
    setSshKeyPath,
    sshPassphrase,
    setSshPassphrase,
    kiwiInstances,
    loadingInstances,
    kiwiToken,
    kiwiLoggingIn,
    meta,
    formVariant,
    hasUsername,
    sslOptions,
    draft,
    handleDatabaseTypeChange,
    handleKiwiLogin,
    loadKiwiInstances,
    onTest,
    onSave,
    testing,
    testOk,
    testErr,
    testResultRef,
    showAdvanced,
    setShowAdvanced,
    tabFill,
  };
}

export type ConnectionFormState = ReturnType<typeof useConnectionForm>;
