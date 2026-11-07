import { aiCommands } from '../commands/ai';
import { parseSchemaDropTargets } from './sqlSchemaDropTargets';
import type { AiMemoryEntry } from '../types';

export interface MemoryCorrectionSubmission {
  originalSql: string;
  currentSql: string;
  connectionId?: string;
  database?: string;
}

/**
 * Submit a correction candidate after a successful execution. The host owns
 * normalization, the auto-learn switch, conflict handling and candidate status;
 * the UI only supplies the original AI SQL and the SQL that actually ran.
 *
 * Learning must never make a successful query look failed, so transport errors
 * are intentionally swallowed here.
 */
export async function submitMemoryCorrectionIfNeeded(
  submission: MemoryCorrectionSubmission,
): Promise<AiMemoryEntry | null> {
  const originalSql = submission.originalSql.trim();
  const currentSql = submission.currentSql.trim();
  if (!originalSql || !currentSql || originalSql === currentSql) return null;
  try {
    return await aiCommands.memoryCorrectionCandidate({
      originalSql,
      newSql: currentSql,
      connectionId: submission.connectionId,
      database: submission.database,
    });
  } catch {
    return null;
  }
}

export interface SchemaStaleSubmission {
  sql: string;
  connectionId?: string;
  database?: string;
}

/** Ask the host to mark memories attached to dropped tables/columns stale. */
export async function markStaleForSchemaDrops(submission: SchemaStaleSubmission): Promise<void> {
  const targets = parseSchemaDropTargets(submission.sql);
  if (targets.length === 0) return;
  await Promise.all(
    targets.map(async (target) => {
      try {
        await aiCommands.memoryMarkStale({
          connectionId: submission.connectionId,
          database: submission.database,
          schema: target.schema,
          table: target.table,
          column: target.column,
        });
      } catch {
        // Staleness is a maintenance hint; never fail the user's DDL.
      }
    }),
  );
}
