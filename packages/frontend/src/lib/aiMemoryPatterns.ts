import { aiCommands } from '../commands/ai';
import { queryCommands } from '../commands/query';
import type { AiMemorySqlSample } from '../types';

/** The host applies the same window; keeping it here makes the request explicit. */
export const MEMORY_PATTERN_WINDOW_DAYS = 14;
export const MEMORY_PATTERN_MIN_OCCURRENCES = 3;
const HISTORY_LIMIT = 100;

export interface MemoryPatternExtractionInput {
  sql: string;
  connectionId?: string | null;
  database?: string | null;
  /** Mirrors the two T13 memory switches; off means no extraction at all. */
  autoLearn: boolean;
  now?: number;
}

const inFlight = new Map<string, Promise<void>>();
const pending = new Map<string, MemoryPatternExtractionInput>();

function scopeKey(input: MemoryPatternExtractionInput): string {
  return `${input.connectionId ?? ''}\u0000${input.database ?? ''}`;
}

async function extractOnce(input: MemoryPatternExtractionInput): Promise<void> {
  if (
    typeof queryCommands?.getQueryHistoryPage !== 'function' ||
    typeof aiCommands?.memoryExtractCandidates !== 'function'
  ) {
    return;
  }
  const now = input.now ?? Date.now();
  const since = new Date(now - MEMORY_PATTERN_WINDOW_DAYS * 24 * 60 * 60 * 1000).toISOString();
  const page = await queryCommands.getQueryHistoryPage({
    limit: HISTORY_LIMIT,
    connectionId: input.connectionId ?? null,
    database: input.database ?? null,
    since,
    order: 'recent',
  });
  const history: AiMemorySqlSample[] = page.entries
    .filter((entry) => entry.success && entry.sql.trim().length > 0)
    .map((entry) => ({ sql: entry.sql, observedAt: entry.executedAt }));
  await aiCommands.memoryExtractCandidates({
    sql: input.sql,
    ...(input.connectionId ? { connectionId: input.connectionId } : {}),
    ...(input.database ? { database: input.database } : {}),
    history,
    windowDays: MEMORY_PATTERN_WINDOW_DAYS,
    minOccurrences: MEMORY_PATTERN_MIN_OCCURRENCES,
  });
}

/**
 * Feed successful query history to the T14 extractor.
 *
 * Concurrent executions for one connection/database are coalesced into one
 * in-flight pass, and a newer execution queues exactly one follow-up pass so
 * the third occurrence in a burst is not lost. Errors stay local to memory
 * learning and never affect the query that just succeeded.
 */
export function extractMemoryPatternsIfNeeded(input: MemoryPatternExtractionInput): Promise<void> {
  if (!input.autoLearn || input.sql.trim().length === 0) return Promise.resolve();
  const key = scopeKey(input);
  const current = inFlight.get(key);
  if (current) {
    pending.set(key, input);
    return current;
  }
  let next = input;
  const task = (async () => {
    while (true) {
      try {
        await extractOnce(next);
      } catch {
        // Extraction is advisory; a history read or IPC failure must not
        // surface as a query failure or block the next attempt.
      }
      const queued = pending.get(key);
      if (!queued) {
        inFlight.delete(key);
        return;
      }
      pending.delete(key);
      next = queued;
    }
  })();
  inFlight.set(key, task);
  return task;
}
