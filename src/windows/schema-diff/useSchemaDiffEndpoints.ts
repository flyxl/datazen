import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { databaseCommands } from '../../commands/database';
import { useI18n } from '../../hooks/useI18n';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../lib/dedicatedDbSession';
import { pickDefaultSchema, uniqueSchemasFromTables } from '../data-sync/utils';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ConnectionConfig } from '../../types';

export interface SchemaDiffEndpointSessions {
  source: DedicatedSideSession | null;
  target: DedicatedSideSession | null;
}

export interface UseSchemaDiffEndpointsOptions {
  onError?: (message: string) => void;
}

export function useSchemaDiffEndpoints(options: UseSchemaDiffEndpointsOptions = {}) {
  const { onError } = options;
  const { t } = useI18n();

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

  const reportError = useCallback(
    (message: string) => {
      onError?.(message);
    },
    [onError],
  );

  const loadConnections = useCallback(() => {
    void invoke<ConnectionConfig[]>('get_connections')
      .then(setConnections)
      .catch(() => setConnections([]));
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:connections-changed', loadConnections).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [loadConnections]);

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

  const targetOptions = connOptions;

  const isCrossDialect = useMemo(() => {
    if (!sourceConn || !targetConn) return false;
    return sourceConn.databaseType !== targetConn.databaseType;
  }, [sourceConn, targetConn]);

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

  useEffect(() => {
    if (!sourceId) {
      setSourceDatabases([]);
      setSourceSchemas([]);
      setSourceSchema('');
      setSourceDatabase('');
      setSourceSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === sourceId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(sourceId, cfg?.database);
        if (cancelled) return;
        setSourceDatabases(databases);
        const preferred = cfg?.database ?? '';
        setSourceDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
        );
      } catch {
        if (!cancelled) setSourceDatabases([]);
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
      setTargetDatabase('');
      setTargetSession(null);
      return;
    }
    let cancelled = false;
    const cfg = connections.find((c) => c.id === targetId);
    (async () => {
      try {
        const { databases } = await listDatabasesDedicated(targetId, cfg?.database);
        if (cancelled) return;
        setTargetDatabases(databases);
        const preferred = cfg?.database ?? '';
        setTargetDatabase((prev) =>
          databases.includes(preferred)
            ? preferred
            : prev && databases.includes(prev)
              ? prev
              : (databases[0] ?? ''),
        );
      } catch {
        if (!cancelled) setTargetDatabases([]);
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
      return;
    }
    let cancelled = false;
    (async () => {
      const next = await ensureDedicatedSession(sourceSession, sourceId, sourceDatabase);
      if (!cancelled) setSourceSession(next);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [sourceId, sourceDatabase]);

  useEffect(() => {
    if (!targetId || !targetDatabase) {
      setTargetSession(null);
      return;
    }
    let cancelled = false;
    (async () => {
      const next = await ensureDedicatedSession(targetSession, targetId, targetDatabase);
      if (!cancelled) setTargetSession(next);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect when endpoint or catalog changes
  }, [targetId, targetDatabase]);

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
        const tables = await databaseCommands.getTables(connId, sourceDatabase);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setSourceSchemas(schemas);
        setSourceSchema((prev) => pickDefaultSchema(schemas, prev));
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
        const tables = await databaseCommands.getTables(connId, targetDatabase);
        if (cancelled) return;
        const schemas = uniqueSchemasFromTables(tables);
        setTargetSchemas(schemas);
        setTargetSchema((prev) => pickDefaultSchema(schemas, prev));
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
    setSourceSession(targetSession);
    setTargetSession(sourceSession);
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
    sourceSession,
    targetSession,
  ]);

  const refreshEndpointSessions = useCallback(async (): Promise<SchemaDiffEndpointSessions> => {
    if (!sourceId || !targetId || !sourceDatabase || !targetDatabase) {
      return { source: null, target: null };
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
      reportError(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
      return { source: null, target: null };
    }
  }, [
    sourceSession,
    targetSession,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    reportError,
    t,
  ]);

  const ensureConnected = useCallback(
    async (side: 'source' | 'target'): Promise<string | null> => {
      const connectionId = side === 'source' ? sourceId : targetId;
      const database = side === 'source' ? sourceDatabase : targetDatabase;
      const current = side === 'source' ? sourceSession : targetSession;
      if (!connectionId || !database) return null;
      if (
        current &&
        current.connectionId === connectionId &&
        current.database === database &&
        current.dbSessionId
      ) {
        try {
          const { connectionCommands } = await import('../../commands/connection');
          const alive = await connectionCommands.pingConnection(current.dbSessionId);
          if (alive) return current.dbSessionId;
        } catch {
          // Reconnect below.
        }
      }
      try {
        const next = await ensureDedicatedSession(current, connectionId, database);
        if (side === 'source') setSourceSession(next);
        else setTargetSession(next);
        return next?.dbSessionId ?? null;
      } catch (e) {
        reportError(`${t('sync.connectFailed')} ${e instanceof Error ? e.message : String(e)}`);
        return null;
      }
    },
    [sourceId, targetId, sourceDatabase, targetDatabase, sourceSession, targetSession, reportError, t],
  );

  const validateEndpoints = useCallback((): boolean => {
    if (!sourceId || !targetId) {
      reportError(t('sync.selectBoth'));
      return false;
    }
    if (sourceId === targetId) {
      reportError(t('sync.cannotSame'));
      return false;
    }
    if (isSameEndpoint()) {
      reportError(t('sync.cannotSameDb'));
      return false;
    }
    if (!sourceDatabase || !targetDatabase) {
      reportError(t('sync.selectDbRequired'));
      return false;
    }
    return true;
  }, [sourceId, targetId, sourceDatabase, targetDatabase, isSameEndpoint, reportError, t]);

  return {
    connections,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    sourceDatabases,
    targetDatabases,
    sourceSchemas,
    targetSchemas,
    sourceSession,
    targetSession,
    sourceConn,
    targetConn,
    connOptions,
    targetOptions,
    isCrossDialect,
    setSourceId,
    setTargetId,
    setSourceDatabase,
    setTargetDatabase,
    setSourceSchema,
    setTargetSchema,
    handleSwap,
    ensureConnected,
    refreshEndpointSessions,
    validateEndpoints,
    isSameEndpoint,
  };
}

export type UseSchemaDiffEndpointsReturn = ReturnType<typeof useSchemaDiffEndpoints>;
