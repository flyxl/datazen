export type RedisTopology = 'standalone' | 'cluster' | 'sentinel';

export interface RedisTlsOptions {
  enabled?: boolean;
  caPath?: string;
  certPath?: string;
  keyPath?: string;
  keyPassphrase?: string;
  insecureSkipVerify?: boolean;
}

export interface RedisConnectionOptions {
  topology?: RedisTopology;
  clusterNodes?: string[];
  sentinelNodes?: string[];
  sentinelMasterName?: string;
  sentinelNodePassword?: string;
  tls?: RedisTlsOptions;
  pinnedNodeAddr?: string;
}

export function readRedisOptions(raw: Record<string, unknown> | undefined): RedisConnectionOptions {
  if (!raw) return {};
  const tlsRaw = raw.tls;
  const tls = tlsRaw && typeof tlsRaw === 'object' ? (tlsRaw as RedisTlsOptions) : undefined;
  return {
    topology: readTopology(raw.topology),
    clusterNodes: readStringArray(raw.clusterNodes),
    sentinelNodes: readStringArray(raw.sentinelNodes),
    sentinelMasterName: readString(raw.sentinelMasterName),
    sentinelNodePassword: readString(raw.sentinelNodePassword),
    tls,
    pinnedNodeAddr: readString(raw.pinnedNodeAddr),
  };
}

function readTopology(value: unknown): RedisTopology {
  if (value === 'cluster' || value === 'sentinel') return value;
  return 'standalone';
}

function readString(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function readStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const items = value
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  return items.length > 0 ? items : undefined;
}

export function parseNodeLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function formatNodeLines(nodes: string[] | undefined): string {
  return nodes?.join('\n') ?? '';
}
