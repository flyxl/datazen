import { redisCommandInvoke, type RedisInvokeFn } from '../shared/redisInvoke';

export type PluginInvokeFn = RedisInvokeFn;

export async function invokeSetString(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  value: string,
  keepTtl = false,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_string', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    value,
    keepTtl,
  });
}

export async function invokeSetExpireAt(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  expireAt: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_ttl', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    expireAt,
  });
}

export async function invokeHashSet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  field: string,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_set', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    field,
    value,
  });
}

export async function invokeHashDel(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  fields: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'hash_del', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    fields,
  });
}

export async function invokeListPush(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  values: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_push', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    side,
    values,
  });
}

export async function invokeListSet(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  index: number,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'list_set', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    index,
    value,
  });
}

export async function invokeListPop(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  side: 'left' | 'right',
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  return invoke('redis', 'list_pop', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    side,
  });
}

export async function invokeListIndex(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  index: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<string | null> {
  return (await invoke('redis', 'list_index', {
    dbSessionId,
    dbIndex,
    key,
    index,
  })) as string | null;
}

export async function invokeListRem(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  count: number,
  value: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<number> {
  return (await invoke('redis', 'list_rem', {
    dbSessionId,
    dbIndex,
    key,
    count,
    value,
  })) as number;
}

export async function invokeSetAdd(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_add', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeSetRemove(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_remove', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetAdd(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: { member: string; score: number }[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_add', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export async function invokeZsetRemove(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  members: string[],
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'zset_remove', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    members,
  });
}

export type HashScanEntry = { field: string; value: string };
export type HashScanResult = { cursor: number; entries: HashScanEntry[] };

export async function invokeHashScan(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  cursor: number,
  count: number,
  matchPattern?: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<HashScanResult> {
  const args: Record<string, unknown> = {
    dbSessionId,
    dbIndex,
    key,
    cursor,
    count,
  };
  if (matchPattern) args.matchPattern = matchPattern;
  return (await invoke('redis', 'hash_scan', args)) as HashScanResult;
}

export type ListRangeResult = { items: string[] };

export async function invokeListRange(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  start: number,
  stop: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<ListRangeResult> {
  return (await invoke('redis', 'list_range', {
    dbSessionId,
    dbIndex,
    key,
    start,
    stop,
  })) as ListRangeResult;
}

export type SetScanResult = { cursor: number; members: string[] };

export async function invokeSetScan(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  cursor: number,
  count: number,
  matchPattern?: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<SetScanResult> {
  const args: Record<string, unknown> = {
    dbSessionId,
    dbIndex,
    key,
    cursor,
    count,
  };
  if (matchPattern) args.matchPattern = matchPattern;
  return (await invoke('redis', 'set_scan', args)) as SetScanResult;
}

export type ZsetScanMember = { member: string; score: number };
export type ZsetScanResult = { cursor: number; members: ZsetScanMember[] };

export async function invokeZsetScan(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  cursor: number,
  count: number,
  matchPattern?: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
): Promise<ZsetScanResult> {
  const args: Record<string, unknown> = {
    dbSessionId,
    dbIndex,
    key,
    cursor,
    count,
  };
  if (matchPattern) args.matchPattern = matchPattern;
  return (await invoke('redis', 'zset_scan', args)) as ZsetScanResult;
}

export async function invokeRename(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  newKey: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'rename', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    newKey,
  });
}

export async function invokeSetTtl(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  ttlSeconds: number,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  await invoke('redis', 'set_ttl', {
    dbSessionId: dbSessionId,
    dbIndex: dbIndex,
    key,
    ttlSeconds,
  });
}

export async function invokeCreateKey(
  dbSessionId: string,
  dbIndex: number,
  key: string,
  keyType: string,
  initialValue: string,
  invoke: PluginInvokeFn = redisCommandInvoke,
) {
  switch (keyType) {
    case 'string':
      await invokeSetString(dbSessionId, dbIndex, key, initialValue, false, invoke);
      break;
    case 'hash':
      await invokeHashSet(dbSessionId, dbIndex, key, 'field', initialValue || '', invoke);
      break;
    case 'list':
      await invokeListPush(dbSessionId, dbIndex, key, 'right', [initialValue || ''], invoke);
      break;
    case 'set':
      await invokeSetAdd(dbSessionId, dbIndex, key, [initialValue || 'member'], invoke);
      break;
    case 'zset':
      await invokeZsetAdd(
        dbSessionId,
        dbIndex,
        key,
        [{ member: initialValue || 'member', score: 0 }],
        invoke,
      );
      break;
    case 'ReJSON': {
      const trimmed = initialValue.trim();
      let jsonValue = '{}';
      if (trimmed) {
        try {
          JSON.parse(trimmed);
          jsonValue = trimmed;
        } catch {
          jsonValue = JSON.stringify(trimmed);
        }
      }
      await invoke('redis', 'json_set', {
        dbSessionId: dbSessionId,
        dbIndex: dbIndex,
        key,
        path: '$',
        value: jsonValue,
      });
      break;
    }
    default:
      throw new Error(`Unsupported key type: ${keyType}`);
  }
}
