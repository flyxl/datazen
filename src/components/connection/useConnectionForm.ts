import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { connectionCommands } from '../../commands/connection';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { PRESET_GROUPS } from '../../lib/connectionGroups';
import {
  buildConnectionConfig,
  coerceConnectionGroup,
  sanitizeConnectionOptions,
  type ConnectionFormSnapshot,
} from '../../lib/connectionFormModel';
import { newId } from './shared';
import type {
  ConnectionConfig,
  DatabaseType,
  HttpProxyTunnelConfig,
  SslMode,
  SshAuthMethod,
  SshTunnelConfig,
  TunnelKind,
  WebSocketTunnelConfig,
} from '../../types';
import { getDriverConnectionForm, getDriverValidator } from '../../extensions/generated';

export interface UseConnectionFormOptions {
  editId?: string | null;
  existingConnections?: ConnectionConfig[];
  defaultGroup?: string | null;
  onAfterSave?: () => void;
}

function resolveInitialTunnelKind(existing: ConnectionConfig): TunnelKind {
  if (existing.tunnelKind) return existing.tunnelKind;
  if (existing.sshTunnel?.enabled) return 'ssh';
  if (existing.httpProxyTunnel?.enabled) return 'httpProxy';
  if (existing.websocketTunnel?.enabled) return 'websocket';
  return 'none';
}

export function useConnectionForm(options: UseConnectionFormOptions = {}) {
  const { editId, existingConnections, defaultGroup, onAfterSave } = options;
  const { t } = useI18n();
  const saveConnection = useConnectionStore((s) => s.saveConnection);
  const typeSnapshotsRef = useRef(new Map<DatabaseType, ConnectionFormSnapshot>());

  const [name, setName] = useState('');
  const [databaseType, setDatabaseType] = useState<DatabaseType>('postgresql');
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('5432');
  const [database, setDatabase] = useState('postgres');
  const [schema, setSchema] = useState('default');
  const [username, setUsername] = useState('postgres');
  const [password, setPassword] = useState('');
  const [sslMode, setSslMode] = useState<SslMode>('prefer');
  const [group, setGroupState] = useState<string>(() =>
    coerceConnectionGroup(defaultGroup ?? PRESET_GROUPS.development),
  );
  const setGroup = useCallback((value: unknown) => {
    setGroupState(coerceConnectionGroup(value));
  }, []);
  const [colorTag, setColorTag] = useState<string>('#3b82f6');
  const [readOnly, setReadOnlyState] = useState<boolean>(
    () => DB_REGISTRY['postgresql']?.readOnly === true,
  );
  const driverReadOnly = DB_REGISTRY[databaseType]?.readOnly === true;

  const setReadOnly = useCallback(
    (value: boolean) => {
      if (DB_REGISTRY[databaseType]?.readOnly === true) {
        setReadOnlyState(true);
        return;
      }
      setReadOnlyState(value);
    },
    [databaseType],
  );

  const [showAdvanced, setShowAdvanced] = useState(false);

  const [tunnelKind, setTunnelKindState] = useState<TunnelKind>('none');
  const setTunnelKind = useCallback((kind: TunnelKind) => {
    setTunnelKindState(kind);
    setSshEnabled(kind === 'ssh');
  }, []);

  const [sshEnabled, setSshEnabled] = useState(false);
  const [sshHost, setSshHost] = useState('');
  const [sshPort, setSshPort] = useState('22');
  const [sshUsername, setSshUsername] = useState('');
  const [sshAuthMethod, setSshAuthMethod] = useState<SshAuthMethod>('password');
  const [sshPassword, setSshPassword] = useState('');
  const [sshKeyPath, setSshKeyPath] = useState('');
  const [sshPassphrase, setSshPassphrase] = useState('');
  const [sshJumpEnabled, setSshJumpEnabled] = useState(false);
  const [sshJumpHost, setSshJumpHost] = useState('');
  const [sshJumpPort, setSshJumpPort] = useState('22');
  const [sshJumpUsername, setSshJumpUsername] = useState('');
  const [sshJumpAuthMethod, setSshJumpAuthMethod] = useState<SshAuthMethod>('password');
  const [sshJumpPassword, setSshJumpPassword] = useState('');
  const [sshJumpKeyPath, setSshJumpKeyPath] = useState('');
  const [sshJumpPassphrase, setSshJumpPassphrase] = useState('');

  const [httpProxyHost, setHttpProxyHost] = useState('');
  const [httpProxyPort, setHttpProxyPort] = useState('8080');
  const [httpProxyScheme, setHttpProxyScheme] = useState<'http' | 'https'>('http');
  const [httpProxyUsername, setHttpProxyUsername] = useState('');
  const [httpProxyPassword, setHttpProxyPassword] = useState('');
  const [httpProxyTimeout, setHttpProxyTimeout] = useState('30');

  const [wsUrl, setWsUrl] = useState('');
  const [wsMode, setWsMode] = useState<'datazen_v1' | 'raw_binary'>('datazen_v1');
  const [wsAuthToken, setWsAuthToken] = useState('');
  const [wsTimeout, setWsTimeout] = useState('30');

  const [testing, setTesting] = useState(false);
  const [testOk, setTestOk] = useState<string | null>(null);
  const [testErr, setTestErr] = useState<string | null>(null);
  const testResultRef = useRef<HTMLDivElement>(null);

  const [connectionOptions, setConnectionOptions] = useState<Record<string, unknown>>({});

  const [loaded, setLoaded] = useState(false);

  const setOptions = useCallback(
    (
      next: Record<string, unknown> | ((prev: Record<string, unknown>) => Record<string, unknown>),
    ) => {
      setConnectionOptions((prev) => {
        const resolved = typeof next === 'function' ? next(prev) : next;
        return sanitizeConnectionOptions(resolved);
      });
    },
    [],
  );

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
    setGroup(existing.group);
    setColorTag(existing.colorTag ?? '#3b82f6');
    const isExistingDriverRo = DB_REGISTRY[existing.databaseType]?.readOnly === true;
    setReadOnlyState(isExistingDriverRo || existing.readOnly === true);
    setConnectionOptions(sanitizeConnectionOptions(existing.options ?? {}));

    const kind = resolveInitialTunnelKind(existing);
    setTunnelKindState(kind);
    setSshEnabled(kind === 'ssh');

    if (existing.sshTunnel?.enabled) {
      setSshHost(existing.sshTunnel.host);
      setSshPort(String(existing.sshTunnel.port));
      setSshUsername(existing.sshTunnel.username);
      setSshAuthMethod(existing.sshTunnel.authMethod);
      setSshPassword(existing.sshTunnel.password ?? '');
      setSshKeyPath(existing.sshTunnel.privateKeyPath ?? '');
      setSshPassphrase(existing.sshTunnel.passphrase ?? '');
      if (existing.sshTunnel.jump?.enabled) {
        const jump = existing.sshTunnel.jump;
        setSshJumpEnabled(true);
        setSshJumpHost(jump.host);
        setSshJumpPort(String(jump.port));
        setSshJumpUsername(jump.username);
        setSshJumpAuthMethod(jump.authMethod);
        setSshJumpPassword(jump.password ?? '');
        setSshJumpKeyPath(jump.privateKeyPath ?? '');
        setSshJumpPassphrase(jump.passphrase ?? '');
      }
    }

    if (existing.httpProxyTunnel?.enabled) {
      setHttpProxyHost(existing.httpProxyTunnel.host);
      setHttpProxyPort(String(existing.httpProxyTunnel.port));
      setHttpProxyScheme(existing.httpProxyTunnel.scheme === 'https' ? 'https' : 'http');
      setHttpProxyUsername(existing.httpProxyTunnel.username ?? '');
      setHttpProxyPassword(existing.httpProxyTunnel.password ?? '');
      setHttpProxyTimeout(String(existing.httpProxyTunnel.connectTimeoutSecs ?? 30));
    }

    if (existing.websocketTunnel?.enabled) {
      setWsUrl(existing.websocketTunnel.url);
      setWsMode(existing.websocketTunnel.mode === 'raw_binary' ? 'raw_binary' : 'datazen_v1');
      setWsAuthToken(existing.websocketTunnel.authToken ?? '');
      setWsTimeout(String(existing.websocketTunnel.connectTimeoutSecs ?? 30));
    }

    setShowAdvanced(true);
    setLoaded(true);
  }, [editId, loaded, existingConnections]);

  const tabFill = useCallback(
    (setter: (v: string) => void) => (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Tab' && !e.currentTarget.value && e.currentTarget.placeholder) {
        e.preventDefault();
        setter(e.currentTarget.placeholder);
      }
    },
    [],
  );

  const captureSnapshot = useCallback((): ConnectionFormSnapshot => {
    return {
      name,
      host,
      port,
      database,
      schema,
      username,
      password,
      sslMode,
      group,
      colorTag,
      readOnly,
      connectionOptions: sanitizeConnectionOptions(connectionOptions),
      showAdvanced,
      sshEnabled,
      sshHost,
      sshPort,
      sshUsername,
      sshAuthMethod,
      sshPassword,
      sshKeyPath,
      sshPassphrase,
      sshJumpEnabled,
      sshJumpHost,
      sshJumpPort,
      sshJumpUsername,
      sshJumpAuthMethod,
      sshJumpPassword,
      sshJumpKeyPath,
      sshJumpPassphrase,
    };
  }, [
    colorTag,
    connectionOptions,
    database,
    group,
    host,
    name,
    password,
    port,
    readOnly,
    schema,
    showAdvanced,
    sshAuthMethod,
    sshEnabled,
    sshHost,
    sshJumpAuthMethod,
    sshJumpEnabled,
    sshJumpHost,
    sshJumpKeyPath,
    sshJumpPassphrase,
    sshJumpPassword,
    sshJumpPort,
    sshJumpUsername,
    sshKeyPath,
    sshPassphrase,
    sshPassword,
    sshPort,
    sshUsername,
    sslMode,
    username,
  ]);

  const restoreSnapshot = useCallback((snapshot: ConnectionFormSnapshot) => {
    setName(snapshot.name);
    setHost(snapshot.host);
    setPort(snapshot.port);
    setDatabase(snapshot.database);
    setSchema(snapshot.schema);
    setUsername(snapshot.username);
    setPassword(snapshot.password);
    setSslMode(snapshot.sslMode);
    setGroup(snapshot.group);
    setColorTag(snapshot.colorTag);
    const isTargetDriverRo = DB_REGISTRY[databaseType]?.readOnly === true;
    setReadOnlyState(isTargetDriverRo || snapshot.readOnly);
    setConnectionOptions(sanitizeConnectionOptions(snapshot.connectionOptions));
    setShowAdvanced(snapshot.showAdvanced);
    setSshEnabled(snapshot.sshEnabled);
    setTunnelKindState(snapshot.sshEnabled ? 'ssh' : 'none');
    setSshHost(snapshot.sshHost);
    setSshPort(snapshot.sshPort);
    setSshUsername(snapshot.sshUsername);
    setSshAuthMethod(snapshot.sshAuthMethod);
    setSshPassword(snapshot.sshPassword);
    setSshKeyPath(snapshot.sshKeyPath);
    setSshPassphrase(snapshot.sshPassphrase);
    setSshJumpEnabled(snapshot.sshJumpEnabled);
    setSshJumpHost(snapshot.sshJumpHost);
    setSshJumpPort(snapshot.sshJumpPort);
    setSshJumpUsername(snapshot.sshJumpUsername);
    setSshJumpAuthMethod(snapshot.sshJumpAuthMethod);
    setSshJumpPassword(snapshot.sshJumpPassword);
    setSshJumpKeyPath(snapshot.sshJumpKeyPath);
    setSshJumpPassphrase(snapshot.sshJumpPassphrase);
  }, [databaseType, setGroup]);

  const applyTypeDefaults = useCallback((newType: DatabaseType) => {
    const meta = DB_REGISTRY[newType];
    if (!meta) return;

    setName('');
    setHost(meta.defaultHost || '127.0.0.1');
    setPort(meta.defaultPort ? String(meta.defaultPort) : '');
    setUsername(meta.defaultUser || '');
    setSslMode(meta.defaultSslMode ?? 'prefer');
    setTunnelKindState('none');
    setSshEnabled(false);
    setSshHost('');
    setSshPort('22');
    setSshUsername('');
    setSshAuthMethod('password');
    setSshPassword('');
    setSshKeyPath('');
    setSshPassphrase('');
    setSshJumpEnabled(false);
    setSshJumpHost('');
    setSshJumpPort('22');
    setSshJumpUsername('');
    setSshJumpAuthMethod('password');
    setSshJumpPassword('');
    setSshJumpKeyPath('');
    setSshJumpPassphrase('');
    setHttpProxyHost('');
    setHttpProxyPort('8080');
    setHttpProxyScheme('http');
    setHttpProxyUsername('');
    setHttpProxyPassword('');
    setHttpProxyTimeout('30');
    setWsUrl('');
    setWsMode('datazen_v1');
    setWsAuthToken('');
    setWsTimeout('30');

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

    if (meta.readOnly === true) {
      setReadOnlyState(true);
    }

    setConnectionOptions(sanitizeConnectionOptions({ ...(meta.defaultOptions ?? {}) }));
  }, []);

  const handleDatabaseTypeChange = useCallback(
    (newType: DatabaseType) => {
      if (newType === databaseType) return;
      const meta = DB_REGISTRY[newType];
      if (!meta) return;

      if (!editId) {
        typeSnapshotsRef.current.set(databaseType, captureSnapshot());
      }

      setDatabaseType(newType);

      if (!editId) {
        const saved = typeSnapshotsRef.current.get(newType);
        if (saved) {
          restoreSnapshot(saved);
          return;
        }
        applyTypeDefaults(newType);
      } else {
        setHost(meta.defaultHost || '127.0.0.1');
        setPort(meta.defaultPort ? String(meta.defaultPort) : '');
        setUsername(meta.defaultUser || '');
        setSslMode(meta.defaultSslMode ?? 'prefer');

        if (!meta.supportsSSH && tunnelKind === 'ssh') {
          setTunnelKindState('none');
          setSshEnabled(false);
        }

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

        if (meta.readOnly === true) {
          setReadOnlyState(true);
        }

        setConnectionOptions(sanitizeConnectionOptions({ ...(meta.defaultOptions ?? {}) }));
      }
    },
    [applyTypeDefaults, captureSnapshot, databaseType, editId, restoreSnapshot, tunnelKind],
  );

  const sshTunnel = useMemo((): SshTunnelConfig | undefined => {
    if (tunnelKind !== 'ssh' && !sshEnabled) return undefined;
    if (!sshEnabled) return undefined;
    return {
      enabled: true,
      host: sshHost,
      port: Number(sshPort) || 22,
      username: sshUsername,
      authMethod: sshAuthMethod,
      password: sshAuthMethod === 'password' ? sshPassword || undefined : undefined,
      privateKeyPath: sshAuthMethod === 'private_key' ? sshKeyPath || undefined : undefined,
      passphrase: sshAuthMethod === 'private_key' ? sshPassphrase || undefined : undefined,
      jump: sshJumpEnabled
        ? {
            enabled: true,
            host: sshJumpHost,
            port: Number(sshJumpPort) || 22,
            username: sshJumpUsername,
            authMethod: sshJumpAuthMethod,
            password: sshJumpAuthMethod === 'password' ? sshJumpPassword || undefined : undefined,
            privateKeyPath:
              sshJumpAuthMethod === 'private_key' ? sshJumpKeyPath || undefined : undefined,
            passphrase:
              sshJumpAuthMethod === 'private_key' ? sshJumpPassphrase || undefined : undefined,
          }
        : undefined,
    };
  }, [
    sshAuthMethod,
    sshEnabled,
    sshHost,
    sshJumpAuthMethod,
    sshJumpEnabled,
    sshJumpHost,
    sshJumpKeyPath,
    sshJumpPassphrase,
    sshJumpPassword,
    sshJumpPort,
    sshJumpUsername,
    sshKeyPath,
    sshPassphrase,
    sshPassword,
    sshPort,
    sshUsername,
    tunnelKind,
  ]);

  const httpProxyTunnel = useMemo((): HttpProxyTunnelConfig | undefined => {
    if (tunnelKind !== 'httpProxy') return undefined;
    return {
      enabled: true,
      host: httpProxyHost,
      port: Number(httpProxyPort) || 8080,
      scheme: httpProxyScheme,
      username: httpProxyUsername || undefined,
      password: httpProxyPassword || undefined,
      connectTimeoutSecs: Number(httpProxyTimeout) || 30,
    };
  }, [
    httpProxyHost,
    httpProxyPassword,
    httpProxyPort,
    httpProxyScheme,
    httpProxyTimeout,
    httpProxyUsername,
    tunnelKind,
  ]);

  const websocketTunnel = useMemo((): WebSocketTunnelConfig | undefined => {
    if (tunnelKind !== 'websocket') return undefined;
    return {
      enabled: true,
      url: wsUrl,
      mode: wsMode,
      authToken: wsAuthToken || undefined,
      connectTimeoutSecs: Number(wsTimeout) || 30,
    };
  }, [tunnelKind, wsAuthToken, wsMode, wsTimeout, wsUrl]);

  const meta = DB_REGISTRY[databaseType];
  const formVariant = meta?.connectionForm ?? 'standard';
  const isDriverForm = !!getDriverConnectionForm(formVariant);
  const hasUsername = !!meta?.defaultUser || !!meta?.requiresUsername || isDriverForm;
  const supportsSSL = !!meta?.supportsSSL;
  const supportsSSH = !!meta?.supportsSSH;

  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    const driverValidator = getDriverValidator(formVariant);
    if (driverValidator) {
      const errors = driverValidator(
        { host, port, database, username, password, schema, options: connectionOptions },
        t as (key: string) => string,
      );
      setValidationErrors(errors);
      return Object.keys(errors).length === 0;
    }

    const errors: Record<string, string> = {};

    if (meta?.connectionMode === 'file') {
      if (!database.trim()) errors.database = t('newConn.required');
    } else if (!isDriverForm) {
      if (!host.trim()) errors.host = t('newConn.required');
      if (!port.trim() || isNaN(Number(port))) errors.port = t('newConn.required');
    }

    if (tunnelKind === 'httpProxy') {
      if (!httpProxyHost.trim()) errors.httpProxyHost = t('newConn.required');
      if (!httpProxyPort.trim() || isNaN(Number(httpProxyPort))) {
        errors.httpProxyPort = t('newConn.required');
      }
    }
    if (tunnelKind === 'websocket') {
      if (!wsUrl.trim()) errors.wsUrl = t('newConn.required');
    }
    if (tunnelKind === 'ssh' && sshEnabled) {
      if (!sshHost.trim()) errors.sshHost = t('newConn.required');
      if (!sshUsername.trim()) errors.sshUsername = t('newConn.required');
    }

    setValidationErrors(errors);
    return Object.keys(errors).length === 0;
  }, [
    connectionOptions,
    database,
    formVariant,
    host,
    httpProxyHost,
    httpProxyPort,
    isDriverForm,
    meta?.connectionMode,
    password,
    port,
    schema,
    sshEnabled,
    sshHost,
    sshUsername,
    t,
    tunnelKind,
    username,
    wsUrl,
  ]);

  const buildIpcConfig = useCallback((): ConnectionConfig => {
    return buildConnectionConfig({
      editId,
      newId,
      unnamedLabel: t('newConn.unnamed'),
      name,
      databaseType,
      host,
      port,
      database,
      schema,
      username,
      password,
      sslMode,
      group,
      colorTag,
      readOnly,
      connectionOptions,
      sshTunnel: tunnelKind === 'ssh' ? sshTunnel : undefined,
      tunnelKind: tunnelKind === 'none' ? undefined : tunnelKind,
      httpProxyTunnel: tunnelKind === 'httpProxy' ? httpProxyTunnel : undefined,
      websocketTunnel: tunnelKind === 'websocket' ? websocketTunnel : undefined,
    });
  }, [
    colorTag,
    connectionOptions,
    database,
    databaseType,
    editId,
    group,
    host,
    httpProxyTunnel,
    name,
    password,
    port,
    readOnly,
    schema,
    sshTunnel,
    sslMode,
    t,
    tunnelKind,
    username,
    websocketTunnel,
  ]);

  async function onTest() {
    if (!validate()) return;
    const config = buildIpcConfig();
    setTesting(true);
    setTestOk(null);
    setTestErr(null);
    try {
      const info = await connectionCommands.testConnection(config);
      setTestOk(info.serverVersion);
    } catch (e) {
      setTestErr(
        typeof e === 'string' ? e : e instanceof Error ? e.message : t('newConn.testFailed'),
      );
    } finally {
      setTesting(false);
      setTimeout(() => {
        testResultRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
      }, 50);
    }
  }

  async function onSave() {
    if (!validate()) return;
    await saveConnection(buildIpcConfig());
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
    readOnly: driverReadOnly || readOnly,
    driverReadOnly,
    setReadOnly,
    tunnelKind,
    setTunnelKind,
    sshEnabled,
    setSshEnabled: (v: boolean) => {
      setSshEnabled(v);
      if (v) setTunnelKindState('ssh');
      else if (tunnelKind === 'ssh') setTunnelKindState('none');
    },
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
    sshJumpEnabled,
    setSshJumpEnabled,
    sshJumpHost,
    setSshJumpHost,
    sshJumpPort,
    setSshJumpPort,
    sshJumpUsername,
    setSshJumpUsername,
    sshJumpAuthMethod,
    setSshJumpAuthMethod,
    sshJumpPassword,
    setSshJumpPassword,
    sshJumpKeyPath,
    setSshJumpKeyPath,
    sshJumpPassphrase,
    setSshJumpPassphrase,
    httpProxyHost,
    setHttpProxyHost,
    httpProxyPort,
    setHttpProxyPort,
    httpProxyScheme,
    setHttpProxyScheme,
    httpProxyUsername,
    setHttpProxyUsername,
    httpProxyPassword,
    setHttpProxyPassword,
    httpProxyTimeout,
    setHttpProxyTimeout,
    wsUrl,
    setWsUrl,
    wsMode,
    setWsMode,
    wsAuthToken,
    setWsAuthToken,
    wsTimeout,
    setWsTimeout,
    formVariant,
    hasUsername,
    supportsSSL,
    supportsSSH,
    sslOptions,
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
    setOptions,
  };
}

export type ConnectionFormState = ReturnType<typeof useConnectionForm>;
