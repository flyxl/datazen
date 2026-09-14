import { driverCommands } from '../../../../src/commands/driver';
import type { KeyDetail, KeyScanResult } from '../../../../src/types';

/** Test-injectable invoke used by Redis UI helpers. */
export type RedisInvokeFn = (
  pluginId: string,
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

function unwrapData(data: unknown): unknown {
  if (
    data &&
    typeof data === 'object' &&
    !Array.isArray(data) &&
    Object.keys(data as object).length === 1 &&
    (data as { ok?: unknown }).ok === true
  ) {
    return undefined;
  }
  return data;
}

/** Run a Redis Driver Command through the generic `execute_driver_command` IPC. */
export async function redisCommandInvoke<T = unknown>(
  _pluginId: string,
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const dbSessionId = String(args.dbSessionId ?? '');
  const input = { ...args };
  delete input.dbSessionId;
  const result = await driverCommands.execute({ dbSessionId, command, input });
  return unwrapData(result.data) as T;
}

export type ScanKeysOptions = {
  /** Redis TYPE filter (string / hash / list / set / zset / stream). Empty = all. */
  keyType?: string;
  /** When true, size column uses MEMORY USAGE (bytes). */
  withMemory?: boolean;
};

/**
 * Scan keys. The 6th argument may be either `ScanKeysOptions` or a test `invoke`
 * function (backward compatible with older call sites).
 */
export async function invokeScanKeys(
  dbSessionId: string,
  dbIndex: number,
  pattern: string,
  cursor: number,
  count: number,
  optionsOrInvoke: ScanKeysOptions | RedisInvokeFn = {},
  maybeInvoke?: RedisInvokeFn,
): Promise<KeyScanResult> {
  let options: ScanKeysOptions = {};
  let invoke: RedisInvokeFn = redisCommandInvoke;
  if (typeof optionsOrInvoke === 'function') {
    invoke = optionsOrInvoke;
  } else {
    options = optionsOrInvoke ?? {};
    if (maybeInvoke) invoke = maybeInvoke;
  }

  const args: Record<string, unknown> = {
    dbSessionId,
    dbIndex,
    pattern,
    cursor,
    count,
  };
  if (options.keyType && options.keyType !== 'all' && options.keyType !== '*') {
    args.keyType = options.keyType;
  }
  if (options.withMemory) {
    args.withMemory = true;
  }
  return (await invoke('redis', 'scan_keys', args)) as KeyScanResult;
}

export async function invokeGetKey(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<KeyDetail> {
  return (await invoke('redis', 'get_key', {
    dbSessionId,
    dbIndex,
    key,
  })) as KeyDetail;
}
