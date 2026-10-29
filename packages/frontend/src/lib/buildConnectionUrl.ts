import type { ConnectionConfig } from '../types';
import { DB_REGISTRY } from './databaseTypes';

/**
 * Build a standard database connection URL from a ConnectionConfig.
 * Returns null if the driver type is unknown or cannot be represented as a URL.
 */
export function buildConnectionUrl(config: ConnectionConfig): string | null {
  const meta = DB_REGISTRY[config.databaseType];
  if (!meta) return null;

  const scheme = meta.clipboardSchemes?.[0];
  if (!scheme) return null;

  if (meta.connectionMode === 'file') {
    const path = config.database ?? '';
    if (!path) return null;
    return `${scheme}://${path}`;
  }

  const host = config.host || 'localhost';
  const port = config.port ?? meta.defaultPort;

  let userInfo = '';
  if (config.username) {
    userInfo = encodeURIComponent(config.username);
    if (config.password) {
      userInfo += ':' + encodeURIComponent(config.password);
    }
    userInfo += '@';
  } else if (config.password) {
    userInfo = ':' + encodeURIComponent(config.password) + '@';
  }

  const hostPort = port && port !== meta.defaultPort ? `${host}:${port}` : host;

  let path = '';
  if (config.database) {
    path = '/' + encodeURIComponent(config.database);
  }

  const params = new URLSearchParams();
  if (config.sslMode && config.sslMode !== 'disable') {
    params.set('sslmode', config.sslMode);
  }

  const query = params.toString();
  return `${scheme}://${userInfo}${hostPort}${path}${query ? '?' + query : ''}`;
}
