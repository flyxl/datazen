import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';
import {
  pickPrefillDatabase,
  pickPrefillSchema,
  resolveDefaultDatabase,
  useMigrationEndpointPrefill,
} from '../../lib/migrationWindowPrefill';
import { isVerifiedMigrationPair } from '../../lib/migrationVerification';
import { useSyncPairingState } from '../../lib/syncPairing';
import type { ConnectionConfig } from '../../types';
import type { DataSyncFeedback } from './useDataSyncFeedback';
import type { DataSyncSharedRefs } from './useDataSyncRefs';
import { pickDefaultSchema, uniqueSchemasFromTables } from './utils';

export interface UseDataSyncEndpointsOptions {
  refs: DataSyncSharedRefs;
  feedback: DataSyncFeedback;
  /** Clears the comparison state when the endpoint scope changes. */
  resetCompareState: () => void;
}

/**
 * Endpoint selection of the Data Sync wizard: connections, databases, schemas
 * and the dedicated database sessions that back them.
 *
 * Every change of the endpoint scope invalidates the comparison, so the
 * change handlers reset the compare state and drop pending profile mappings.
 */
export function useDataSyncEndpoints({
  refs,
  feedback,
  resetCompareState,
}: UseDataSyncEndpointsOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen } = feedback;
  const { pendingProfileMappingsRef } = refs;

  const [connections, setConnections] = useState<ConnectionConfig[]>([]);
  const [sourceSession, setSourceSession] = useState<DedicatedSideSession | null>(null);
  const [targetSession, setTargetSession] = useState<DedicatedSideSession | null>(null);
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [sourceDatabases, setSourceDatabases] = useState<string[]>([]);
  const [targetDatabases, setTargetDatabases] = useState<string[]>([]);
  const [sourceDatabase, setSourceDatabase] = useState('');
  const [targetDatabase, setTargetDatabase] = useState('');
  const [sourceSchemas, setSourceSchemas] = useState<string[]>([]);
  const [targetSchemas, setTargetSchemas] = useState<string[]>([]);
  const [sourceSchema, setSourceSchema] = useState('');
  const [targetSchema, setTargetSchema] = useState('');
  const [sourceSessionError, setSourceSessionError] = useState('');
  const [targetSessionError, setTargetSessionError] = useState('');

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const { dbSessionId } = (payload ?? {}) as { dbSessionId?: string };
      if (!dbSessionId) return;
      setSourceSession((prev) => (prev?.dbSessionId === dbSessionId ? null : prev));
      setTargetSession((prev) => (prev?.dbSessionId === dbSessionId ? null : prev));
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch((e) => console.error('Failed to load', e));
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  const migrationPrefillRef = useMigrationEndpointPrefill(connections, setSourceId, setTargetId);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connections-changed', loadConnections).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [loadConnections]);

  const sourceConn = useMemo(
    () => connections.find((c) => c.id === sourceId),
    [connections, sourceId],
  );
  const targetConn = useMemo(
    () => connections.find((c) => c.id === targetId),
    [connections, targetId],
  );

  const connOptions = useMemo(
    () =>
      connections.map((c) => ({
        value: c.id,
        label: `${c.name} (${c.databaseType})`,
      })),
    [connections],
  );

  const { targetSupport, activePairing } = useSyncPairingState(
    sourceConn?.databaseType,
    connections,
    targetId,
  );

  const targetOptions = useMemo(() => {
    const hint = t('common.unsupportedPair');
    const experimentalHint = t('common.experimentalPairHint');
    const srcType = sourceConn?.databaseType;
    return connections.map((c) => {
      const unsupported = Boolean(
        srcType && Object.hasOwn(targetSupport, c.id) && !targetSupport[c.id],
      );
      const base = `${c.name} (${c.databaseType})`;
      const experimental = Boolean(
        srcType &&
          !unsupported &&
          Object.hasOwn(targetSupport, c.id) &&
          !isVerifiedMigrationPair(srcType, c.databaseType),
      );
      return {
        value: c.id,
        label: unsupported
          ? `${base} — ${hint}`
          : experimental
            ? `${base} — ${experimentalHint}`
            : base,
        disabled: unsupported,
        title: unsupported ? hint : experimental ? experimentalHint : undefined,
      };
    });
  }, [connections, sourceConn?.databaseType, targetSupport, t]);

  const targetReadOnly = targetConn?.readOnly === true;

  const experimental = Boolean(
    sourceConn &&
      targetConn &&
      activePairing?.supported &&
      !isVerifiedMigrationPair(sourceConn.databaseType, targetConn.databaseType),
  );

  useEffect(() => {
    const dbSessionId = sourceSession?.dbSessionId;
    return () => {
      void releaseDedicatedSession(dbSessionId);
    };
  }, [sourceSession?.dbSessionId]);

  useEffect(() => {
    const dbSessionId = targetSession?.dbSessionId;
    return () => {
      void releaseDedicatedSession(dbSessionId);
    };
  }, [targetSession?.dbSessionId]);

  const refreshEndpointSessions = useCallback(async () => {
    if (!sourceId || !targetId || !sourceDatabase || !targetDatabase) {
      return {
        source: null as DedicatedSideSession | null,
        target: null as DedicatedSideSession | null,
      };
    }
    try {
      const [source, target] = await Promise.all([
        ensureDedicatedSession(sourceSession, sourceId, sourceDatabase),
        ensureDedicatedSession(targetSession, targetId, targetDatabase),
      ]);
      setSourceSession(source);
      setTargetSession(target);
      return { source, target };
    } catch (e) {
      setErrorMsg(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
      setErrorOpen(true);
      return { source: null, target: null };
    }
  }, [sourceSession, targetSession, sourceId, targetId, sourceDatabase, targetDatabase, t]);

  useEffect(() => {
    if (!sourceId) {
      setSourceDatabases([]);
      setSourceSchemas([]);
      setSourceSchema('');
      setSourceSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === sourceId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(sourceId, cfg?.database);
        if (cancelled) return;
        setSourceDatabases(databases ?? []);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          pickPrefillDatabase(
            migrationPrefillRef,
            'source',
            databases,
            (current) => resolveDefaultDatabase(databases, preferred, current),
            prev,
          ),
        );
      } catch (e) {
        if (!cancelled) {
          setSourceDatabases([]);
          setErrorMsg(
            `${t('sync.loadDatabasesFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
          setErrorOpen(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceId, connections]);

  useEffect(() => {
    if (!targetId) {
      setTargetDatabases([]);
      setTargetSchemas([]);
      setTargetSchema('');
      setTargetSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === targetId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(targetId, cfg?.database);
        if (cancelled) return;
        setTargetDatabases(databases ?? []);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          pickPrefillDatabase(
            migrationPrefillRef,
            'target',
            databases,
            (current) => resolveDefaultDatabase(databases, preferred, current),
            prev,
          ),
        );
      } catch (e) {
        if (!cancelled) {
          setTargetDatabases([]);
          setErrorMsg(
            `${t('sync.loadDatabasesFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
          setErrorOpen(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetId, connections]);

  useEffect(() => {
    if (!sourceId || !sourceDatabase) {
      setSourceSession(null);
      setSourceSessionError('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const next = await ensureDedicatedSession(sourceSession, sourceId, sourceDatabase);
        if (!cancelled) {
          setSourceSession(next);
          setSourceSessionError('');
        }
      } catch (e) {
        if (!cancelled) {
          setSourceSession(null);
          setSourceSessionError(
            `${t('sync.sessionFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [sourceId, sourceDatabase, t]);

  useEffect(() => {
    if (!targetId || !targetDatabase) {
      setTargetSession(null);
      setTargetSessionError('');
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const next = await ensureDedicatedSession(targetSession, targetId, targetDatabase);
        if (!cancelled) {
          setTargetSession(next);
          setTargetSessionError('');
        }
      } catch (e) {
        if (!cancelled) {
          setTargetSession(null);
          setTargetSessionError(
            `${t('sync.sessionFailed')}${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [targetId, targetDatabase, t]);

  useEffect(() => {
    const sourceMeta = sourceConn ? DB_REGISTRY[sourceConn.databaseType] : undefined;
    if (
      !sourceId ||
      !sourceDatabase ||
      sourceMeta?.supportsTables !== true ||
      sourceMeta.supportsSQL !== true
    ) {
      setSourceSchemas([]);
      setSourceSchema('');
      return;
    }
    const connId = sourceSession?.dbSessionId;
    if (
      !connId ||
      sourceSession.connectionId !== sourceId ||
      sourceSession.database !== sourceDatabase
    ) {
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const tables = await databaseCommands.listTables(connId, sourceDatabase, null, sourceId);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setSourceSchemas(schemas);
        setSourceSchema((prev) =>
          pickPrefillSchema(
            migrationPrefillRef,
            'source',
            schemas,
            (current) => pickDefaultSchema(schemas, current),
            prev,
          ),
        );
      } catch {
        if (!cancelled) {
          setSourceSchemas([]);
          setSourceSchema('');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceId, sourceDatabase, sourceConn?.databaseType, sourceSession]);

  useEffect(() => {
    const targetMeta = targetConn ? DB_REGISTRY[targetConn.databaseType] : undefined;
    if (
      !targetId ||
      !targetDatabase ||
      targetMeta?.supportsTables !== true ||
      targetMeta.supportsSQL !== true
    ) {
      setTargetSchemas([]);
      setTargetSchema('');
      return;
    }
    const connId = targetSession?.dbSessionId;
    if (
      !connId ||
      targetSession.connectionId !== targetId ||
      targetSession.database !== targetDatabase
    ) {
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const tables = await databaseCommands.listTables(connId, targetDatabase, null, targetId);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setTargetSchemas(schemas);
        setTargetSchema((prev) =>
          pickPrefillSchema(
            migrationPrefillRef,
            'target',
            schemas,
            (current) => pickDefaultSchema(schemas, current),
            prev,
          ),
        );
      } catch {
        if (!cancelled) {
          setTargetSchemas([]);
          setTargetSchema('');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [targetId, targetDatabase, targetConn?.databaseType, targetSession]);

  const isSameEndpoint = useCallback(() => {
    const norm = (s: string) => s.trim();
    return (
      sourceId === targetId &&
      sourceDatabase !== '' &&
      targetDatabase !== '' &&
      sourceDatabase === targetDatabase &&
      norm(sourceSchema) === norm(targetSchema)
    );
  }, [sourceId, targetId, sourceDatabase, targetDatabase, sourceSchema, targetSchema]);

  const handleSwap = useCallback(() => {
    pendingProfileMappingsRef.current = null;
    setSourceId(targetId);
    setTargetId(sourceId);
    setSourceDatabase(targetDatabase);
    setTargetDatabase(sourceDatabase);
    setSourceDatabases(targetDatabases);
    setTargetDatabases(sourceDatabases);
    setSourceSchemas(targetSchemas);
    setTargetSchemas(sourceSchemas);
    setSourceSchema(targetSchema);
    setTargetSchema(sourceSchema);
    resetCompareState();
  }, [
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceDatabases,
    targetDatabases,
    sourceSchemas,
    targetSchemas,
    sourceSchema,
    targetSchema,
    resetCompareState,
  ]);

  const handleSourceChange = useCallback(
    (id: string) => {
      pendingProfileMappingsRef.current = null;
      setSourceId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetChange = useCallback(
    (id: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetId(id);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleSourceDatabaseChange = useCallback(
    (db: string) => {
      pendingProfileMappingsRef.current = null;
      setSourceDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetDatabaseChange = useCallback(
    (db: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetDatabase(db);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleSourceSchemaChange = useCallback(
    (schema: string) => {
      pendingProfileMappingsRef.current = null;
      setSourceSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  const handleTargetSchemaChange = useCallback(
    (schema: string) => {
      pendingProfileMappingsRef.current = null;
      setTargetSchema(schema);
      resetCompareState();
    },
    [resetCompareState],
  );

  return {
    connections,
    setConnections,
    sourceConn,
    targetConn,
    connOptions,
    targetOptions,
    activePairing,
    targetReadOnly,
    experimental,
    sourceId,
    setSourceId,
    targetId,
    setTargetId,
    sourceDatabase,
    setSourceDatabase,
    targetDatabase,
    setTargetDatabase,
    sourceSchema,
    setSourceSchema,
    targetSchema,
    setTargetSchema,
    sourceDatabases,
    setSourceDatabases,
    targetDatabases,
    setTargetDatabases,
    sourceSchemas,
    setSourceSchemas,
    targetSchemas,
    setTargetSchemas,
    sourceSession,
    setSourceSession,
    targetSession,
    setTargetSession,
    sourceSessionError,
    targetSessionError,
    refreshEndpointSessions,
    isSameEndpoint,
    handleSwap,
    handleSourceChange,
    handleTargetChange,
    handleSourceDatabaseChange,
    handleTargetDatabaseChange,
    handleSourceSchemaChange,
    handleTargetSchemaChange,
  };
}

export type DataSyncEndpoints = ReturnType<typeof useDataSyncEndpoints>;
