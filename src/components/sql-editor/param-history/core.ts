export const PARAM_HISTORY_VERSION = 1 as const;
export const MAX_PARAM_HISTORY = 5;

const SENSITIVE_SUBSTRINGS = [
  'password',
  'passwd',
  'token',
  'secret',
  'credential',
  'apikey',
  'api_key',
] as const;

export interface ParamHistoryStoreV1 {
  version: typeof PARAM_HISTORY_VERSION;
  /** Key: `${connectionId}::${normalizedDescriptorKey}` */
  entries: Record<string, string[]>;
}

export interface ParamHistoryStorage {
  read(): string | null;
  write(raw: string): boolean;
}

export function normalizeParamHistoryKey(connectionId: string, descriptorKey: string): string {
  return `${connectionId}::${descriptorKey.trim().toLowerCase()}`;
}

export function isSensitiveParamName(name: string): boolean {
  const normalized = name.trim().toLowerCase();
  return SENSITIVE_SUBSTRINGS.some((part) => normalized.includes(part));
}

export function emptyParamHistoryStore(): ParamHistoryStoreV1 {
  return { version: PARAM_HISTORY_VERSION, entries: {} };
}

export function parseParamHistoryStore(raw: string | null): ParamHistoryStoreV1 {
  if (!raw) return emptyParamHistoryStore();
  try {
    const parsed = JSON.parse(raw) as Partial<ParamHistoryStoreV1>;
    if (
      parsed.version !== PARAM_HISTORY_VERSION ||
      typeof parsed.entries !== 'object' ||
      !parsed.entries
    ) {
      return emptyParamHistoryStore();
    }
    const entries: Record<string, string[]> = {};
    for (const [key, values] of Object.entries(parsed.entries)) {
      if (!Array.isArray(values)) continue;
      entries[key] = values
        .filter((v): v is string => typeof v === 'string')
        .map((v) => v.trim())
        .filter((v) => v.length > 0)
        .slice(0, MAX_PARAM_HISTORY);
    }
    return { version: PARAM_HISTORY_VERSION, entries };
  } catch {
    return emptyParamHistoryStore();
  }
}

export function loadParamHistory(
  store: ParamHistoryStoreV1,
  connectionId: string,
  descriptorKey: string,
): string[] {
  const key = normalizeParamHistoryKey(connectionId, descriptorKey);
  return (store.entries[key] ?? []).slice(0, MAX_PARAM_HISTORY);
}

export function rememberParamValue(
  store: ParamHistoryStoreV1,
  connectionId: string,
  descriptorKey: string,
  value: string,
): ParamHistoryStoreV1 {
  const trimmed = value.trim();
  if (!trimmed || isSensitiveParamName(descriptorKey)) {
    return store;
  }
  const key = normalizeParamHistoryKey(connectionId, descriptorKey);
  const previous = store.entries[key] ?? [];
  const next = [trimmed, ...previous.filter((entry) => entry !== trimmed)].slice(
    0,
    MAX_PARAM_HISTORY,
  );
  return {
    version: PARAM_HISTORY_VERSION,
    entries: { ...store.entries, [key]: next },
  };
}

export function rememberSubmittedValues(
  store: ParamHistoryStoreV1,
  connectionId: string,
  values: Record<string, string>,
): ParamHistoryStoreV1 {
  let next = store;
  for (const [descriptorKey, value] of Object.entries(values)) {
    next = rememberParamValue(next, connectionId, descriptorKey, value);
  }
  return next;
}

export function clearParamHistoryEntry(
  store: ParamHistoryStoreV1,
  connectionId: string,
  descriptorKey: string,
): ParamHistoryStoreV1 {
  const key = normalizeParamHistoryKey(connectionId, descriptorKey);
  if (!store.entries[key]) return store;
  const entries = { ...store.entries };
  delete entries[key];
  return { version: PARAM_HISTORY_VERSION, entries };
}

export class ParamHistoryCore {
  private store: ParamHistoryStoreV1;

  constructor(private readonly storage: ParamHistoryStorage) {
    this.store = parseParamHistoryStore(storage.read());
  }

  load(connectionId: string, descriptorKey: string): string[] {
    return loadParamHistory(this.store, connectionId, descriptorKey);
  }

  remember(connectionId: string, descriptorKey: string, value: string): string[] {
    this.store = rememberParamValue(this.store, connectionId, descriptorKey, value);
    this.persist();
    return this.load(connectionId, descriptorKey);
  }

  rememberSubmitted(connectionId: string, values: Record<string, string>): void {
    this.store = rememberSubmittedValues(this.store, connectionId, values);
    this.persist();
  }

  clear(connectionId: string, descriptorKey: string): void {
    this.store = clearParamHistoryEntry(this.store, connectionId, descriptorKey);
    this.persist();
  }

  snapshot(): ParamHistoryStoreV1 {
    return this.store;
  }

  private persist(): void {
    this.storage.write(JSON.stringify(this.store));
  }
}
