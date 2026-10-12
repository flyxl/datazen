import { expect, browser, $ } from '@wdio/globals';
import {
  clickNavigatorRefresh, closeExtraWindows, disconnectBackend, expandSchemaTableCategory,
  expandAllGroups, expandConnectedConnectionInNavigator, invokeBackend,
  openConnectionsWorkspace, parseQueryRows, queryScalar, waitForConnectionToolbar,
  withSafeModeOff, type QueryResultPayload,
} from '../helpers.js';
import {
  MYSQL_SYNC_DB, PG_SYNC_DB, mysqlConnectionConfig, pgConnectionConfig,
} from '../lib/schemaDiffFixtures.js';

async function setFixtureNavigatorSearch(query: string): Promise<void> {
  // React may replace the input between WebDriver's synthetic keystrokes.
  // Reacquire it in one DOM operation and dispatch the actual controlled-input event.
  await browser.waitUntil(async () => browser.execute((value: string) => {
    const input = document.querySelector<HTMLInputElement>('[data-testid="connection-navigator-aside"] input');
    if (!input) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    if (!setter) return false;
    setter.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }, query), { timeout: 15000, timeoutMsg: 'Recovery fixture navigator search unavailable' });
}

// Unique databases isolate termination from every seeded/user connection.
for (const dialect of ['postgresql', 'mysql'] as const) {
  describe(`Table transaction explicit recovery (${dialect})`, function () {
    this.timeout(120000);

    it('offers a new session after transaction resource loss and can transact again', async () => {
      const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
      const database = `e2e_table_loss_${stamp}`;
      const tableName = `recovery_probe_${stamp}`;
      let tableSchema: string | undefined;
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
      let stage = 'fixture-setup';
      let receiptMarkers = { receiptPresent: false, resourceLost: false, unknown: false };
      const run = (sql: string) => invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: admin, sql });
      const terminate = async (onlyTransaction: boolean) => {
        if (dialect === 'postgresql') {
          const result = await run(`SELECT pg_terminate_backend(pid) AS terminated FROM pg_stat_activity
            WHERE datname = '${database}' AND pid <> pg_backend_pid()
            ${onlyTransaction ? "AND state = 'idle in transaction'" : ''}`);
          if (onlyTransaction) {
            stage = 'terminate-response';
            expect(parseQueryRows(result).length).toBeGreaterThan(0);
          }
        } else {
          const result = await run(`SELECT ID FROM information_schema.PROCESSLIST WHERE DB = '${database}'`);
          const ids = parseQueryRows(result).map((row) => Number(row[0]));
          if (onlyTransaction) {
            stage = 'terminate-response';
            expect(ids.length).toBeGreaterThan(0);
            stage = 'terminate-connection';
          }
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
          await invokeBackend('execute_query', { dbSessionId: setup, sql: `CREATE TABLE ${tableName} (id INT PRIMARY KEY)` });
          await invokeBackend('execute_query', { dbSessionId: setup, sql: `INSERT INTO ${tableName} VALUES (1)` });
        });
        const fixture = await invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: setup, sql: `SELECT count(*) AS c FROM ${tableName}` });
        expect(queryScalar(fixture, 'c')).toBe(1);
        const metadata = await invokeBackend<{ data: { tables: Array<{ name: string; schema?: string | null }> } }>('execute_driver_command', {
          request: { dbSessionId: setup, command: 'list_tables', input: { database, schema: null } },
        });
        const relation = metadata.data.tables.find((table) => table.name === tableName);
        expect(relation).toBeDefined();
        tableSchema = relation?.schema ?? undefined;
        await disconnectBackend(setup);
        setup = undefined;
        const persisted = await invokeBackend<Array<{ id: string; name: string; database?: string }>>('get_connections');
        expect(persisted.some((connection) => connection.id === connectionId && connection.name === name && connection.database === database)).toBe(true);
        // Raw save_connection does not perform the frontend store's refresh or
        // emit datazen:connections-changed. Mount MainPage to load saved configs.
        await browser.url('tauri://localhost');
        stage = 'workspace-mount';
        await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 20000 });
        await openConnectionsWorkspace();
        await expandAllGroups();
        // Filter the virtualized navigator so the exact fixture card is mounted.
        await setFixtureNavigatorSearch(name);
        stage = 'fixture-card-selection';
        await browser.waitUntil(async () => browser.execute((wanted: string) => {
          const card = Array.from(document.querySelectorAll('[data-conn-item]')).find((node) => node.getAttribute('data-conn-name') === wanted);
          if (!card) return false;
          card.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
          return true;
        }, name), { timeout: 15000, timeoutMsg: 'Fresh recovery fixture connection unavailable' });
        stage = 'connection-toolbar';
        await waitForConnectionToolbar();
        stage = 'fixture-catalog-navigation';
        await setFixtureNavigatorSearch('');
        await expandConnectedConnectionInNavigator(name);
        await clickNavigatorRefresh();
        await expandSchemaTableCategory(tableSchema, database);
        const table = await $(`[data-testid="schema-tree-node"][data-tree-node="table"][data-item-name="${tableName}"]`);
        await table.waitForDisplayed({ timeout: 20000 });
        await table.scrollIntoView({ block: 'center' });
        await table.click();
        stage = 'transaction-begin';
        const begin = await $('[data-testid="table-tx-begin"]');
        await begin.waitForEnabled({ timeout: 20000 });
        await begin.click();
        const rollback = await $('[data-testid="table-tx-rollback"]');
        await rollback.waitForEnabled({ timeout: 20000 });
        stage = 'terminate-fixture-session';
        await withSafeModeOff(() => terminate(true));
        stage = 'rollback-click';
        await rollback.click();
        stage = 'recovery-button-wait';
        const recovery = await $('[data-testid="table-session-open-new"]');
        await recovery.waitForDisplayed({ timeout: 20000 });
        // The original failure remains visible until the user explicitly recovers.
        stage = 'receipt-observation';
        receiptMarkers = await browser.execute(() => {
          const receipt = document.querySelector('[data-testid="table-transaction-error"]');
          const text = receipt?.textContent ?? '';
          return { receiptPresent: Boolean(receipt), resourceLost: text.includes('resourceLost'), unknown: text.includes('unknown') };
        });
        stage = 'receipt-resource-lost-check';
        expect(receiptMarkers.receiptPresent).toBe(true);
        expect(receiptMarkers.resourceLost).toBe(true);
        stage = 'receipt-unknown-check';
        expect(receiptMarkers.unknown).toBe(true);
        stage = 'recovery-button-click';
        await recovery.click();
        stage = 'explicit-recovery';
        await browser.waitUntil(async () => !(await $('[data-testid="table-session-open-new"]').isExisting()), { timeout: 20000 });
        // Successful explicit recovery clears the stale toolbar error.
        expect(await browser.execute(() => document.body.textContent?.includes('resourceLost') ?? false)).toBe(false);
        await $('[data-testid="table-tx-begin"]').waitForEnabled({ timeout: 20000 });
        await $('[data-testid="table-tx-begin"]').click();
        stage = 'reopened-transaction';
        await $('[data-testid="table-tx-rollback"]').waitForEnabled({ timeout: 20000 });
        await $('[data-testid="table-tx-rollback"]').click();
        await $('[data-testid="table-tx-begin"]').waitForEnabled({ timeout: 20000 });
        expect(await $('[data-testid="table-session-open-new"]').isExisting()).toBe(false);
        const verification = await invokeBackend<string>('connect', { connectionId });
        try {
          const result = await invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: verification, sql: `SELECT count(*) AS c FROM ${tableName}` });
          expect(queryScalar(result, 'c')).toBe(1);
        } finally {
          await disconnectBackend(verification);
        }
      } catch (error) {
        const category = error instanceof Error && ['WebDriverError', 'TimeoutError', 'AssertionError', 'Error'].includes(error.name)
          ? error.name : 'Error';
        // Even a button wait failure records the known receipt markers, without
        // returning its text, SQL, connection identity or WebDriver message.
        try {
          receiptMarkers = await browser.execute(() => {
            const receipt = document.querySelector('[data-testid="table-transaction-error"]');
            const text = receipt?.textContent ?? '';
            return { receiptPresent: Boolean(receipt), resourceLost: text.includes('resourceLost'), unknown: text.includes('unknown') };
          });
        } catch { /* Keep the last safe observation if the window is unavailable. */ }
        throw new Error(`P4 table recovery failed at ${stage} (${category}); receiptPresent=${receiptMarkers.receiptPresent}; resourceLost=${receiptMarkers.resourceLost}; unknown=${receiptMarkers.unknown}`);
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
