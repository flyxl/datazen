import {
  redisCommandInvoke,
  type CountMatchingResult,
  type RedisInvokeFn,
} from '../shared/redisInvoke';

/**
 * Driver-command seam for the batch write operations of the key tree
 * (`delete_keys` / `batch_delete_pattern` / `count_matching`).
 *
 * Lives in its own module (D-1) so the row delete, the key dialogs and the
 * import/export pattern estimate all call one place, and so a test can
 * substitute `invoke` without pulling any UI into the graph. Every payload key
 * is camelCase — the Tauri layer maps to the Rust snake_case args (frontend
 * convention).
 */

export interface BatchOperationErrors {
  key: string;
  error: string;
}

export interface BatchDeleteResult {
  deleted: number;
  errors: BatchOperationErrors[];
}

export type PluginInvokeFn = RedisInvokeFn;

export async function invokeDeleteKeys(
  dbSessionId: string,
  dbIndex: number,
  keys: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<number> {
  return (await invoke('redis', 'delete_keys', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    keys,
  })) as number;
}

export async function invokeBatchDeletePattern(
  dbSessionId: string,
  dbIndex: number,
  pattern: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<BatchDeleteResult> {
  return (await invoke('redis', 'batch_delete_pattern', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    pattern,
  })) as BatchDeleteResult;
}

export async function invokeCountMatching(
  dbSessionId: string,
  dbIndex: number,
  pattern: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<CountMatchingResult> {
  return (await invoke('redis', 'count_matching', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    pattern,
  })) as CountMatchingResult;
}

/**
 * The count as the label wants it: a budget-truncated answer is a floor, so it
 * carries the `+` suffix that tells the user more keys exist uncounted.
 */
export function formatMatchCount(result: CountMatchingResult): string {
  return result.truncated ? `${result.count}+` : String(result.count);
}
