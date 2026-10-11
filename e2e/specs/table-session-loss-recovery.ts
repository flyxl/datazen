import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton, clickTableInSidebar, closeExtraWindows, disconnectBackend,
  expandAllGroups, expandConnectedConnectionInNavigator, invokeBackend,
  openConnectionsWorkspace, parseQueryRows, queryScalar, waitForConnectionToolbar,
  withSafeModeOff, type QueryResultPayload,
} from '../helpers.js';
import {
  MYSQL_SYNC_DB, PG_SYNC_DB, mysqlConnectionConfig, pgConnectionConfig,
} from '../lib/schemaDiffFixtures.js';

// Unique databases isolate termination from every seeded/user connection.
for (const dialect of ['postgresql', 'mysql'] as const) {
  describe(`Table transaction explicit recovery (${dialect})`, function () {
    this.timeout(120000);

    it('offers a new session after transaction resource loss and can transact again', async () => {
      const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
      const database = `e2e_table_loss_${stamp}`;
      const adminId = `e2e_loss_admin_${stamp}`;
      const connectionId = `e2e_loss_table_${stamp}`;
      const name = `Table loss ${dialect} ${stamp}`;
      const mainWindow = await browser.getWindowHandle();
      const mainUrl = await browser.getUrl();
      const config = dialect === 'postgresql' ? pgConnectionConfig : mysqlConnectionConfig;
      const savedIds: string[] = [];
      let admin: string | undefined;
      let setup: string | undefined;
      let created = false;
      const run = (sql: string) => invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: admin, sql });
      const terminate = async (onlyTransaction: boolean) => {
        if (dialect === 'postgresql') {
          const result = await run(`SELECT pg_terminate_backend(pid) AS terminated FROM pg_stat_activity
            WHERE datname = '${database}' AND pid <> pg_backend_pid()
            ${onlyTransaction ? "AND state = 'idle in transaction'" : ''}`);
          if (onlyTransaction) expect(parseQueryRows(result).length).toBeGreaterThan(0);
        } else {
          const result = await run(`SELECT ID FROM information_schema.PROCESSLIST WHERE DB = '${database}'`);
          const ids = parseQueryRows(result).map((row) => Number(row[0]));
          if (onlyTransaction) expect(ids.length).toBeGreaterThan(0);
          // MySQL exposes no idle BEGIN without an InnoDB statement in innodb_trx;
          // terminate only connections to this freshly-created fixture database.
          for (const id of ids) {
            if (!Number.isSafeInteger(id) || id <= 0) throw new Error('Invalid fixture connection identity');
            await run(`KILL CONNECTION ${id}`);
          }
        }
      };
      try {
        await invokeBackend('save_connection', {
          config: config(adminId, `Loss cleanup ${stamp}`, dialect === 'postgresql' ? PG_SYNC_DB : MYSQL_SYNC_DB),
        });
        savedIds.push(adminId);
        admin = await invokeBackend<string>('connect', { connectionId: adminId });
        await withSafeModeOff(async () => {
          await run(`CREATE DATABASE ${database}`);
          created = true;
        });
        await invokeBackend('save_connection', { config: config(connectionId, name, database) });
        savedIds.push(connectionId);
        setup = await invokeBackend<string>('connect', { connectionId });
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', { dbSessionId: setup, sql: 'CREATE TABLE recovery_probe (id INT PRIMARY KEY)' });
          await invokeBackend('execute_query', { dbSessionId: setup, sql: 'INSERT INTO recovery_probe VALUES (1)' });
        });
        await disconnectBackend(setup);
        setup = undefined;
        await openConnectionsWorkspace();
        await expandAllGroups();
        await clickCardConnectButton(name);
        await waitForConnectionToolbar();
        await expandConnectedConnectionInNavigator(name);
        await clickTableInSidebar('recovery_probe');
        const begin = await $('[data-testid="table-tx-begin"]');
        await begin.waitForEnabled({ timeout: 20000 });
        await begin.click();
        const rollback = await $('[data-testid="table-tx-rollback"]');
        await rollback.waitForEnabled({ timeout: 20000 });
        await withSafeModeOff(() => terminate(true));
        await rollback.click();
        const recovery = await $('[data-testid="table-session-open-new"]');
        await recovery.waitForDisplayed({ timeout: 20000 });
        // The original failure remains visible until the user explicitly recovers.
        expect(await recovery.parentElement().getText()).not.toBe('');
        await recovery.click();
        await browser.waitUntil(async () => !(await $('[data-testid="table-session-open-new"]').isExisting()), { timeout: 20000 });
        await $('[data-testid="table-tx-begin"]').waitForEnabled({ timeout: 20000 });
        await $('[data-testid="table-tx-begin"]').click();
        await $('[data-testid="table-tx-rollback"]').waitForEnabled({ timeout: 20000 });
        await $('[data-testid="table-tx-rollback"]').click();
        await $('[data-testid="table-tx-begin"]').waitForEnabled({ timeout: 20000 });
        expect(await $('[data-testid="table-session-open-new"]').isExisting()).toBe(false);
        const verification = await invokeBackend<string>('connect', { connectionId });
        try {
          const result = await invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: verification, sql: 'SELECT count(*) AS c FROM recovery_probe' });
          expect(queryScalar(result, 'c')).toBe(1);
        } finally {
          await disconnectBackend(verification);
        }
      } finally {
        try {
          await closeExtraWindows(mainWindow);
          await browser.switchToWindow(mainWindow);
          await browser.url(mainUrl);
        } finally {
          try {
            if (setup) await disconnectBackend(setup);
            if (created && admin) {
              await withSafeModeOff(async () => {
                await terminate(false);
                await run(`DROP DATABASE ${database}`);
              });
            }
          } finally {
            try {
              if (admin) await disconnectBackend(admin);
            } finally {
              const deleted = await Promise.allSettled(savedIds.map((id) => invokeBackend('delete_connection', { id })));
              if (deleted.some((result) => result.status === 'rejected')) throw new Error('Recovery fixture connection cleanup failed');
            }
          }
        }
      }
    });
  });
}
