import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY, normalizeIndexDatabaseField } from '../../lib/databaseTypes';
import { PRESET_GROUPS } from '../../lib/connectionGroups';
import { newId } from './shared';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../../types';
import { getPluginConnectionForm, getPluginValidator } from '../../plugins/generated';

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
  const [schema, setSchema] = useState('default');
  const [username, setUsername] = useState('postgres');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroup] = useState<string>(() => PRESET_GROUPS.development);
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

  const [connectionOptions, setConnectionOptions] = useState<Record<string, unknown>>({});

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
    setSchema(existing.schema ?? 'default');
    setUsername(existing.username ?? '');
    setPassword(existing.password ?? '');
    setSslMode(existing.sslMode);
    setGroup(existing.group ?? '');
    setColorTag(existing.colorTag ?? '#3b82f6');
    setConnectionOptions(existing.options ?? {});
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
    const meta = DB_REGISTRY[newType];
    if (!meta) return;

    setDatabaseType(newType);

    setHost(meta.defaultHost || '127.0.0.1');
    setPort(meta.defaultPort ? String(meta.defaultPort) : '');
    setUsername(meta.defaultUser || '');

    if (!meta.supportsSSH) setSshEnabled(false);

    if (meta.databaseFieldType === 'index') {
      setDatabase(meta.defaultDatabase ?? '0');
    } else if (meta.connectionMode === 'file') {
      setDatabase('');
    } else {
      setDatabase(meta.defaultDatabase ?? '');
    }

    if (meta.connectionIncludesSchema) {
      setSchema('default');
    }

    setConnectionOptions({ ...(meta.defaultOptions ?? {}) });
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
  const formVariant = meta?.connectionForm ?? 'standard';
  const isPluginForm = !!getPluginConnectionForm(formVariant);
  const hasUsername = !!meta?.defaultUser || !!meta?.requiresUsername || isPluginForm;

  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    const pluginValidator = getPluginValidator(formVariant);
    if (pluginValidator) {
      const errors = pluginValidator(
        { host, port, database, username, password, schema, options: connectionOptions },
        t as (key: string) => string,
      );
      setValidationErrors(errors);
      return Object.keys(errors).length === 0;
    }

    const errors: Record<string, string> = {};

    if (meta?.connectionMode === 'file') {
      if (!database.trim()) errors.database = t('newConn.required');
    } else if (!isPluginForm) {
      if (!host.trim()) errors.host = t('newConn.required');
      if (!port.trim() || isNaN(Number(port))) errors.port = t('newConn.required');
    }

    setValidationErrors(errors);
    return Object.keys(errors).length === 0;
  }, [connectionOptions, database, formVariant, host, isPluginForm, meta?.connectionMode, password, port, schema, t, username]);

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
    if (!draftMeta || draftMeta.connectionMode === 'file') {
      return { ...base, database };
    }

    const conn: ConnectionConfig = {
      ...base,
      host: host || draftMeta.defaultHost || undefined,
      port: Number(port) || draftMeta.defaultPort || undefined,
      database:
        draftMeta.databaseFieldType === 'index'
          ? normalizeIndexDatabaseField(database, draftMeta.maxDatabaseIndex ?? 15)
          : database || undefined,
      password: password || undefined,
    };
    const pluginForm = !!getPluginConnectionForm(draftMeta.connectionForm);
    if (draftMeta.defaultUser || draftMeta.requiresUsername || pluginForm) {
      conn.username = username || draftMeta.defaultUser || undefined;
    }
    if (draftMeta.connectionIncludesSchema) {
      conn.schema = schema.trim() || 'default';
    }
    if (Object.keys(connectionOptions).length > 0) {
      conn.options = connectionOptions;
    }
    return conn;
  }, [colorTag, connectionOptions, database, databaseType, editId, group, host, name, password, port, schema, sslMode, sshTunnel, t, username]);

  async function onTest() {
    if (!validate()) return;
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
    if (!validate()) return;
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
    schema,
    setSchema,
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
    meta,
    formVariant,
    hasUsername,
    sslOptions,
    draft,
    handleDatabaseTypeChange,
    onTest,
    onSave,
    testing,
    testOk,
    setTestOk,
    testErr,
    setTestErr,
    testResultRef,
    showAdvanced,
    setShowAdvanced,
    tabFill,
    validationErrors,
    validate,
    options: connectionOptions,
    setOptions: setConnectionOptions,
  };
}

export type ConnectionFormState = ReturnType<typeof useConnectionForm>;
