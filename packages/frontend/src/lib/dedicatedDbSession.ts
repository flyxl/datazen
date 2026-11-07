import { connectionCommands } from '../commands/connection';

export type DedicatedSideSession = {
  connectionId: string;
  database: string;
  dbSessionId: string;
};

/** Release a dedicated sub-window session (ref-counted; tears down at zero). */
export async function releaseDedicatedSession(dbSessionId: string | undefined): Promise<void> {
  if (!dbSessionId) return;
  try {
    await connectionCommands.releaseConnection(dbSessionId);
  } catch {
    // Best-effort on window teardown.
  }
}

/**
 * Ensure a dedicated db session for a Transfer/Sync/Schema-Diff endpoint.
 * Reconnects when `connectionId` or `database` changes so the live session
 * catalog matches the UI selection without calling `use_database`.
 */
export async function ensureDedicatedSession(
  current: DedicatedSideSession | null,
  connectionId: string,
  database: string,
  databaseOverride: string | null = database,
): Promise<DedicatedSideSession | null> {
  if (!connectionId || !database) return null;
  if (
    current &&
    current.connectionId === connectionId &&
    current.database === database &&
    current.dbSessionId
  ) {
    try {
      const alive = await connectionCommands.pingConnection(current.dbSessionId);
      if (alive) return current;
    } catch {
      // Session gone — reconnect below.
    }
  }
  await releaseDedicatedSession(current?.dbSessionId);
  const { dbSessionId } = await connectionCommands.connectDedicated(
    connectionId,
    databaseOverride ?? undefined,
  );
  return { connectionId, database, dbSessionId };
}

/**
 * Acquire or reuse the session for a React effect and release a newly-created
 * session if that effect no longer owns the result when the request completes.
 */
export async function ensureDedicatedSessionForEffect(
  current: DedicatedSideSession | null,
  connectionId: string,
  database: string,
  isCurrent: () => boolean,
  databaseOverride: string | null = database,
): Promise<DedicatedSideSession | null> {
  const next = await ensureDedicatedSession(current, connectionId, database, databaseOverride);
  if (isCurrent()) return next;
  if (next?.dbSessionId && next.dbSessionId !== current?.dbSessionId) {
    await releaseDedicatedSession(next.dbSessionId);
  }
  return null;
}

/** List databases using a short-lived dedicated session (released after listing). */
export async function listDatabasesDedicated(
  connectionId: string,
  preferredDatabase?: string,
): Promise<{ databases: string[]; dbSessionId: string | null }> {
  const { dbSessionId } = await connectionCommands.connectDedicated(
    connectionId,
    preferredDatabase || undefined,
  );
  try {
    const { databaseCommands } = await import('../commands/database');
    const databases = await databaseCommands.getDatabases(dbSessionId, connectionId);
    return { databases, dbSessionId: null };
  } finally {
    await releaseDedicatedSession(dbSessionId);
  }
}
