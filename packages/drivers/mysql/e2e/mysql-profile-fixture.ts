import { browser } from '@wdio/globals';
import { invokeMonitorBackend } from '../../../../e2e/helpers/monitor-journey.js';
import { withFixtureCleanup } from '../../../../e2e/helpers/fixture-cleanup.js';

type Profile = { id: string; database: string };
type FixtureResult = { profile?: Profile; created?: boolean; error?: string };

/** Reuse saved profiles; clone multi-db configuration without exposing credentials. */
export async function withMysqlProfile<T>(journey: (profile: Profile) => Promise<T>): Promise<T> {
  const fixtureId = `mysql-driver-fixture-${crypto.randomUUID()}`;
  const environment = {
    host: process.env.E2E_MYSQL_HOST,
    port: process.env.E2E_MYSQL_PORT,
    username: process.env.E2E_MYSQL_USER,
    password: process.env.E2E_MYSQL_PASSWORD,
    // Same database selection as the Host createAndConnectMySQL helper.
    database: process.env.E2E_MYSQL_DB || 'datazen_test',
  };
  // A unique id can be cleaned after an ambiguous WebDriver/save timeout.
  let owned = true;
  return withFixtureCleanup(async () => {
    const result = await browser.executeAsync(async (id: string, env: typeof environment,
      done: (result: FixtureResult) => void) => {
      const api = window as unknown as { __TAURI_INTERNALS__: {
        invoke: <R>(command: string, args?: Record<string, unknown>) => Promise<R>;
      } };
      try {
        const profiles = await api.__TAURI_INTERNALS__.invoke<Array<Record<string, unknown>>>('get_connections');
        const existing = profiles.find(row => row.databaseType === 'mysql' &&
          typeof row.database === 'string' && row.database.trim().length > 0);
        if (existing) {
          done({ profile: { id: String(existing.id), database: String(existing.database) } });
          return;
        }
        const source = profiles.find(row => row.databaseType === 'mysql');
        const port = Number(env.port);
        if (!source && (!env.host || !env.username || env.password === undefined ||
          !Number.isInteger(port) || port < 1 || port > 65535)) {
          done({ error: 'MYSQL_FIXTURE_CONFIG_MISSING: configure E2E_MYSQL_HOST/PORT/USER/PASSWORD or save a MySQL profile' });
          return;
        }
        const config = source ? { ...source, id, name: id, database: env.database } : {
          id, name: id, databaseType: 'mysql', host: env.host, port,
          username: env.username, password: env.password, database: env.database,
        };
        // This id is fixture-owned even if saving returns an ambiguous failure.
        try {
          await api.__TAURI_INTERNALS__.invoke('save_connection', { config });
          done({ profile: { id, database: env.database }, created: true });
        } catch {
          done({ error: 'MYSQL_FIXTURE_SAVE_FAILED', created: true });
        }
      } catch {
        // Never return backend error text that could contain connection secrets.
        done({ error: 'MYSQL_FIXTURE_LOOKUP_FAILED' });
      }
    }, fixtureId, environment);
    owned = result.created === true;
    if (!result.profile) throw new Error(result.error || 'MYSQL_FIXTURE_UNAVAILABLE');
    return await journey(result.profile);
  }, [{ label: 'mysql-source-profile', run: async () => {
    if (owned) await invokeMonitorBackend('delete_connection', { id: fixtureId });
  } }]);
}
