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
  const connectionId = String(args.connectionId ?? '');
  const input = { ...args };
  delete input.connectionId;
  const result = await driverCommands.execute({ connectionId, command, input });
  return unwrapData(result.data) as T;
}

export async function invokeScanKeys(
  connectionId: string,
  dbIndex: number,
  pattern: string,
  cursor: number,
  count: number,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<KeyScanResult> {
  return (await invoke('redis', 'scan_keys', {
    connectionId,
    dbIndex,
    pattern,
    cursor,
    count,
  })) as KeyScanResult;
}

export async function invokeGetKey(
  connectionId: string,
  dbIndex: number,
  key: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<KeyDetail> {
  return (await invoke('redis', 'get_key', {
    connectionId,
    dbIndex,
    key,
  })) as KeyDetail;
}
