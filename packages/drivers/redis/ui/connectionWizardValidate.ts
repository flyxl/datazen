import {
  readRedisOptions,
  type RedisConnectionOptions,
} from './connectionOptions';

export interface RedisWizardFields {
  host: string;
  port: string;
  database: string;
  username: string;
  password: string;
  schema: string;
  options?: Record<string, unknown>;
}

export function validateRedisConnection(
  fields: RedisWizardFields,
  t: (key: string) => string,
): Record<string, string> {
  const errors: Record<string, string> = {};
  const opts = readRedisOptions(fields.options);
  const topology = opts.topology ?? 'standalone';

  if (topology === 'standalone') {
    if (!fields.host.trim()) errors.host = t('newConn.required');
    if (!fields.port.trim() || Number.isNaN(Number(fields.port))) {
      errors.port = t('newConn.required');
    }
    return errors;
  }

  if (topology === 'cluster') {
    const nodes = opts.clusterNodes ?? [];
    if (nodes.length === 0) {
      if (!fields.host.trim()) errors.host = t('newConn.required');
      if (!fields.port.trim() || Number.isNaN(Number(fields.port))) {
        errors.port = t('newConn.required');
      }
    } else if (nodes.some((node) => !isHostPort(node))) {
      errors.clusterNodes = t('redis.wizard.invalidNodeFormat');
    }
    return errors;
  }

  if (topology === 'sentinel') {
    if (!opts.sentinelMasterName) {
      errors.sentinelMasterName = t('redis.wizard.sentinelMasterRequired');
    }
    const nodes = opts.sentinelNodes ?? [];
    if (nodes.length === 0) {
      errors.sentinelNodes = t('redis.wizard.sentinelNodesRequired');
    } else if (nodes.some((node) => !isHostPort(node))) {
      errors.sentinelNodes = t('redis.wizard.invalidNodeFormat');
    }
  }

  return errors;
}

function isHostPort(node: string): boolean {
  const trimmed = node.trim();
  if (!trimmed) return false;
  const idx = trimmed.lastIndexOf(':');
  if (idx <= 0 || idx === trimmed.length - 1) return false;
  const port = trimmed.slice(idx + 1);
  const portNum = Number(port);
  return Number.isInteger(portNum) && portNum > 0 && portNum <= 65535;
}

export function mergeRedisOptions(
  current: Record<string, unknown> | undefined,
  patch: Partial<RedisConnectionOptions>,
): Record<string, unknown> {
  const merged = { ...(current ?? {}), ...patch };
  if (patch.tls) {
    merged.tls = { ...(readRedisOptions(current).tls ?? {}), ...patch.tls };
  }
  return merged;
}
