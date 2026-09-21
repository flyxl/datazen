import { driverCommands, type KeyScanResult } from '@datazen/driver-sdk';
import type { KeyDetail, ValueFrame } from './types';

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
  /** When true, only include keys without expiry (TTL == -1). */
  noTtlOnly?: boolean;
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
  if (options.noTtlOnly) {
    args.noTtlOnly = true;
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

/** Binary-safe key fetch — returns ValueFrame with base64 raw bytes. */
export async function invokeGetKeyRaw(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  withMemory = true,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<ValueFrame> {
  return (await invoke('redis', 'get_key_raw', {
    dbSessionId,
    dbIndex,
    key,
    withMemory,
  })) as ValueFrame;
}

/** A direct child under a key prefix: a leaf key or a virtual folder. */
export type ChildEntry =
  | {
      kind: 'key';
      key: string;
      keyType: string;
      ttl: number;
      logicalLen: number;
      memBytes: number | null;
    }
  | { kind: 'folder'; prefix: string; count: number };

export interface ListChildrenResult {
  children: ChildEntry[];
  cursor: number;
}

/** List direct children under a prefix (leaf keys + virtual folders), one tree level. */
export async function invokeListChildren(
  dbSessionId: string,
  dbIndex: number,
  prefix: string,
  cursor = 0,
  count = 200,
  options: { sep?: string; noTtlOnly?: boolean; keyType?: string } = {},
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<ListChildrenResult> {
  const args: Record<string, unknown> = { dbSessionId, dbIndex, prefix, cursor, count };
  if (options.sep) args.sep = options.sep;
  if (options.noTtlOnly) args.noTtlOnly = true;
  if (options.keyType && options.keyType !== 'all') args.keyType = options.keyType;
  return (await invoke('redis', 'list_children', args)) as ListChildrenResult;
}

export interface DbSize {
  db: number;
  keys: number;
}

/** Fetch key counts for every database. */
export async function invokeDbSizes(
  dbSessionId: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<DbSize[]> {
  return (await invoke('redis', 'db_sizes', { dbSessionId })) as DbSize[];
}

/** A single value-search hit. */
export interface ValueMatchHit {
  key: string;
  matchedIn: 'key' | 'value';
  preview: string;
}

export interface ScanValuesParams {
  pattern: string;
  query: string;
  mode: 'key' | 'value' | 'all';
  cursor: number;
  taskId?: string;
  maxKeys?: number;
  byteBudget?: number;
  perValuePeek?: number;
  count?: number;
}

/** One incremental batch of the guarded value search. */
export async function invokeScanValues(
  dbSessionId: string,
  dbIndex: number,
  params: ScanValuesParams,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<ScanValuesBatch> {
  const args: Record<string, unknown> = { dbSessionId, dbIndex, ...params };
  return (await invoke('redis', 'scan_values', args)) as ScanValuesBatch;
}

export interface ScanValuesBatch {
  taskId: string;
  done: boolean;
  cancelled: boolean;
  limitHit: boolean;
  scannedKeys: number;
  matched: ValueMatchHit[];
  cursor: number;
}

/** Signal the active value-search task to abort (idempotent). */
export async function invokeScanAbort(
  dbSessionId: string,
  taskId: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<void> {
  await invoke('redis', 'scan_abort', { dbSessionId, taskId });
}

export type BackendCodec = 'msgpack' | 'pickle' | 'php' | 'java';

export interface DecodeValueResult {
  ok: boolean;
  json?: string;
}

/** Parse-only decode of a base64 payload into a JSON tree text (R8). Throws on reject. */
export async function invokeDecodeValue(
  dbSessionId: string,
  codec: BackendCodec,
  dataB64: string,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<DecodeValueResult> {
  return (await invoke('redis', 'decode_value', {
    dbSessionId,
    codec,
    data: dataB64,
  })) as DecodeValueResult;
}

/** Binary-safe SET from base64 raw bytes (R9 write-back channel). */
export async function invokeSetStringRaw(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  dataB64: string,
  keepTtl = false,
  invoke: RedisInvokeFn = redisCommandInvoke,
): Promise<void> {
  await invoke('redis', 'set_string_raw', {
    dbSessionId,
    dbIndex,
    key,
    dataB64,
    keepTtl,
  });
}
