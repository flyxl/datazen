import type { DatabaseTypeMeta } from '@datazen/extension-points';
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
import { useTunnelFormState } from './useTunnelFormState';
import type {
  ConnectionConfig,
  DatabaseType,
  HttpProxyTunnelConfig,
  SslMode,
  SshTunnelConfig,
  TunnelKind,
  WebSocketTunnelConfig,
} from '../../types';
import { getDriverConnectionForm, getDriverValidator } from '../../extensions/generated';
import { pickPath } from '../../lib/pathPicker';
import type { ConnectionFormState } from '@datazen/driver-sdk';

/** Canonical shape lives in @datazen/driver-sdk; re-exported for existing host imports. */
export type { ConnectionFormState };

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

export function useConnectionForm(options: UseConnectionFormOptions = {}): ConnectionFormState {
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
  const [clearStoredCredentials, setClearStoredCredentials] = useState(false);
  useEffect(() => setClearStoredCredentials(false), [editId]);
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

  const meta = DB_REGISTRY[databaseType];
  const formVariant = meta?.connectionForm ?? 'standard';
  const isDriverForm = !!getDriverConnectionForm(formVariant);
  const hasUsername = !!meta?.defaultUser || !!meta?.requiresUsername || isDriverForm;
  const supportsSSL = !!meta?.supportsSSL;
  const supportsSSH = !!meta?.supportsSSH;

  // Tunnel source state machine + every inline tunnel field (see useTunnelFormState).
  const tunnel = useTunnelFormState({ supportsSSH });
  const {
    tunnelSource,
    tunnelKind,
    tunnelId,
    setTunnelSource,
    effectiveTunnelKind,
    hydrateTunnelRef,
    resetTunnel,
    sshEnabled,
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
  } = tunnel;

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

    // A stored `tunnelId` hydrates the `saved` state directly — never an unbind.
    hydrateTunnelRef(existing.tunnelId ?? null, resolveInitialTunnelKind(existing));

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
  }, [
    editId,
    loaded,
    existingConnections,
    setGroup,
    hydrateTunnelRef,
    setSshHost,
    setSshPort,
    setSshUsername,
    setSshAuthMethod,
    setSshPassword,
    setSshKeyPath,
    setSshPassphrase,
    setSshJumpEnabled,
    setSshJumpHost,
    setSshJumpPort,
    setSshJumpUsername,
    setSshJumpAuthMethod,
    setSshJumpPassword,
    setSshJumpKeyPath,
    setSshJumpPassphrase,
    setHttpProxyHost,
    setHttpProxyPort,
    setHttpProxyScheme,
    setHttpProxyUsername,
    setHttpProxyPassword,
    setHttpProxyTimeout,
    setWsUrl,
    setWsMode,
    setWsAuthToken,
    setWsTimeout,
  ]);

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

  const restoreSnapshot = useCallback(
    (snapshot: ConnectionFormSnapshot) => {
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
      hydrateTunnelRef(null, snapshot.sshEnabled ? 'ssh' : 'none');
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
    },
    [
      databaseType,
      setGroup,
      hydrateTunnelRef,
      setSshHost,
      setSshPort,
      setSshUsername,
      setSshAuthMethod,
      setSshPassword,
      setSshKeyPath,
      setSshPassphrase,
      setSshJumpEnabled,
      setSshJumpHost,
      setSshJumpPort,
      setSshJumpUsername,
      setSshJumpAuthMethod,
      setSshJumpPassword,
      setSshJumpKeyPath,
      setSshJumpPassphrase,
    ],
  );

  const applyTypeDefaults = useCallback(
    (newType: DatabaseType) => {
      const newMeta = (DB_REGISTRY as Record<string, DatabaseTypeMeta>)[newType];
      if (!newMeta) return;
      setName('');
      setHost(newMeta.defaultHost || '127.0.0.1');
      setPort(newMeta.defaultPort ? String(newMeta.defaultPort) : '');
      setUsername(newMeta.defaultUser || '');
      setSslMode(newMeta.defaultSslMode ?? 'prefer');
      resetTunnel();
      if (newMeta.databaseFieldType === 'index') {
        setDatabase(newMeta.defaultDatabase ?? '0');
      } else if (newMeta.connectionMode === 'file') {
        setDatabase('');
      } else {
        setDatabase(newMeta.defaultDatabase ?? '');
      }
      if (newMeta.connectionIncludesSchema) setSchema('default');
      if (newMeta.readOnly === true) setReadOnlyState(true);
      setConnectionOptions(sanitizeConnectionOptions({ ...(newMeta.defaultOptions ?? {}) }));
    },
    [resetTunnel],
  );

  const handleDatabaseTypeChange = useCallback(
    (newType: string) => {
      if (newType === databaseType) return;
      const newMeta = (DB_REGISTRY as Record<string, DatabaseTypeMeta>)[newType];
      if (!newMeta) return;
      if (!editId) typeSnapshotsRef.current.set(databaseType, captureSnapshot());
      setDatabaseType(newType as DatabaseType);
      if (!editId) {
        const saved = typeSnapshotsRef.current.get(newType as DatabaseType);
        if (saved) {
          restoreSnapshot(saved);
          return;
        }
        applyTypeDefaults(newType as DatabaseType);
      } else {
        setHost(newMeta.defaultHost || '127.0.0.1');
        setPort(newMeta.defaultPort ? String(newMeta.defaultPort) : '');
        setUsername(newMeta.defaultUser || '');
        setSslMode(newMeta.defaultSslMode ?? 'prefer');
        // Only an inline SSH tunnel is dropped here; a saved reference must never be.
        if (!newMeta.supportsSSH && tunnelSource === 'inline' && tunnelKind === 'ssh') {
          setTunnelSource('none');
        }
        if (newMeta.databaseFieldType === 'index') setDatabase(newMeta.defaultDatabase ?? '0');
        else if (newMeta.connectionMode === 'file') setDatabase('');
        else setDatabase(newMeta.defaultDatabase ?? '');
        if (newMeta.connectionIncludesSchema) setSchema('default');
        if (newMeta.readOnly === true) setReadOnlyState(true);
        setConnectionOptions(sanitizeConnectionOptions({ ...(newMeta.defaultOptions ?? {}) }));
      }
    },
    [
      applyTypeDefaults,
      captureSnapshot,
      databaseType,
      editId,
      restoreSnapshot,
      setTunnelSource,
      tunnelKind,
      tunnelSource,
    ],
  );

  const sshTunnel = useMemo((): SshTunnelConfig | undefined => {
    if (effectiveTunnelKind !== 'ssh' || !sshEnabled) return undefined;
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
    effectiveTunnelKind,
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
  ]);

  const httpProxyTunnel = useMemo((): HttpProxyTunnelConfig | undefined => {
    if (effectiveTunnelKind !== 'httpProxy') return undefined;
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
    effectiveTunnelKind,
    httpProxyHost,
    httpProxyPassword,
    httpProxyPort,
    httpProxyScheme,
    httpProxyTimeout,
    httpProxyUsername,
  ]);

  const websocketTunnel = useMemo((): WebSocketTunnelConfig | undefined => {
    if (effectiveTunnelKind !== 'websocket') return undefined;
    return {
      enabled: true,
      url: wsUrl,
      mode: wsMode,
      authToken: wsAuthToken || undefined,
      connectTimeoutSecs: Number(wsTimeout) || 30,
    };
  }, [effectiveTunnelKind, wsAuthToken, wsMode, wsTimeout, wsUrl]);

  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});

  const validate = useCallback((): boolean => {
    // Shared by every form variant, driver-validator forms included: a dangling
    // tunnel reference must refuse to save (G3 / P1-8). This used to sit after
    // the driver-validator early return, so `redis` forms could persist a
    // reference the backend is guaranteed to reject (tunnel-form-BUG-001).
    const errors: Record<string, string> = {};
    if (tunnel.tunnelRefMissing) errors.tunnelId = t('newConn.tunnelMissing');
    // Inline-tunnel requirements belong to the same shared prologue: leaving them
    // below the driver-validator early return enforced the tunnel domain for only
    // half the form variants (same refactor gap as BUG-001). The `!tunnelId` guard
    // keeps them scoped to the `inline` source — a `saved` reference is validated
    // by id, never by these fields.
    if (!tunnelId && effectiveTunnelKind === 'httpProxy') {
      if (!httpProxyHost.trim()) errors.httpProxyHost = t('newConn.required');
      if (!httpProxyPort.trim() || isNaN(Number(httpProxyPort)))
        errors.httpProxyPort = t('newConn.required');
    }
    if (!tunnelId && effectiveTunnelKind === 'websocket') {
      if (!wsUrl.trim()) errors.wsUrl = t('newConn.required');
    }
    if (!tunnelId && effectiveTunnelKind === 'ssh' && sshEnabled) {
      if (!sshHost.trim()) errors.sshHost = t('newConn.required');
      if (!sshUsername.trim()) errors.sshUsername = t('newConn.required');
    }

    const driverValidator = getDriverValidator(formVariant);
    if (driverValidator) {
      // Driver validators only report their own connection fields; the tunnel
      // domain keys above belong to this hook, so merging cannot clobber them.
      Object.assign(
        errors,
        driverValidator(
          { host, port, database, username, password, schema, options: connectionOptions },
          t as (key: string) => string,
        ),
      );
      setValidationErrors(errors);
      return Object.keys(errors).length === 0;
    }

    // Generic (non-driver) connection fields keep their original scope: driver
    // validators own their own host/port/database rules.
    if (meta?.connectionMode === 'file') {
      if (!database.trim()) errors.database = t('newConn.required');
    } else if (!isDriverForm) {
      if (!host.trim()) errors.host = t('newConn.required');
      if (!port.trim() || isNaN(Number(port))) errors.port = t('newConn.required');
    }
    setValidationErrors(errors);
    return Object.keys(errors).length === 0;
  }, [
    connectionOptions,
    database,
    effectiveTunnelKind,
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
    tunnel.tunnelRefMissing,
    tunnelId,
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
      sshTunnel: tunnelId ? undefined : effectiveTunnelKind === 'ssh' ? sshTunnel : undefined,
      tunnelKind: effectiveTunnelKind === 'none' ? undefined : effectiveTunnelKind,
      tunnelId: tunnelId || undefined,
      httpProxyTunnel: tunnelId
        ? undefined
        : effectiveTunnelKind === 'httpProxy'
          ? httpProxyTunnel
          : undefined,
      websocketTunnel: tunnelId
        ? undefined
        : effectiveTunnelKind === 'websocket'
          ? websocketTunnel
          : undefined,
    });
  }, [
    colorTag,
    connectionOptions,
    database,
    databaseType,
    editId,
    effectiveTunnelKind,
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
    tunnelId,
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
    await saveConnection(buildIpcConfig(), clearStoredCredentials);
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
    ...tunnel,
    isEditing: Boolean(editId),
    clearStoredCredentials,
    setClearStoredCredentials,
    name,
    setName,
    databaseType,
    setDatabaseType: handleDatabaseTypeChange,
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
    // Driver connection fields render `PathInput`, which cannot open a native
    // dialog by itself; the picker travels through the form-state contract.
    pickPath,
  };
}
