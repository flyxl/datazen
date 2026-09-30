/** MySQL Schema Tree journey: visible object categories, object definitions and teardown. */
import { expect, browser, $ } from '@wdio/globals';
import {
  closeExtraWindows,
  connectBackend,
  dblclickConnByExactName,
  disconnectBackend,
  expandConnectedConnectionInNavigator,
  expandSchemaCategory,
  findCardByName,
  invokeBackend,
  openConnectionsWorkspace,
  parseQueryRows,
  queryScalar,
  waitForConnectionToolbar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../../../e2e/helpers.js';

const ADMIN_DATABASE = process.env.E2E_MYSQL_DB || 'datazen_test';

function mysqlConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'mysql',
    host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port: Number(process.env.E2E_MYSQL_PORT) || 3306,
    username: process.env.E2E_MYSQL_USER || 'root',
    password: process.env.E2E_MYSQL_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

describe('MySQL Schema Tree object journey', function () {
  this.timeout(180_000);

  const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
  const database = `e2e_temp_schema_tree_${stamp}`;
  const prefix = `e2e_tree_${stamp}`;
  const parentTable = `${prefix}_parents`;
  const table = `${prefix}_items`;
  const view = `${prefix}_active_items`;
  const functionName = `${prefix}_normalize_code`;
  const procedureName = `${prefix}_list_children`;
  const triggerName = `${prefix}_touch_updated_at`;
  const adminId = `e2e_mysql_schema_tree_admin_${stamp}`;
  const connectionId = `e2e_mysql_schema_tree_${stamp}`;
  const connectionName = `E2E MySQL Schema Tree ${stamp}`;

  let mainWindow: string | undefined;
  let adminConfigSaved = false;
  let treeConfigSaved = false;
  let adminSession: string | undefined;
  let treeSession: string | undefined;
  let databaseCreated = false;

  async function execute(session: string, sql: string): Promise<QueryResultPayload> {
    return invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: session, sql });
  }

  async function waitForNode(kind: string, name: string): Promise<void> {
    const selector =
      `[data-testid="schema-tree-node"][data-tree-node="${kind}"]` + `[data-item-name="${name}"]`;
    await browser.waitUntil(
      async () =>
        await $(selector)
          .isDisplayed()
          .catch(() => false),
      {
        timeout: 20_000,
        timeoutMsg: `MySQL Schema Tree did not show ${kind} ${name}`,
      },
    );
  }

  async function assertObjectDefinition(
    kind: 'function' | 'procedure' | 'trigger',
    name: string,
    expectedText: string,
  ): Promise<void> {
    const selector =
      `[data-testid="schema-tree-node"][data-tree-node="${kind}"]` + `[data-item-name="${name}"]`;
    const node = await $(selector);
    await node.waitForDisplayed({ timeout: 20_000 });
    await node.click();

    let definition = '';
    await browser.waitUntil(
      async () => {
        definition = await browser.execute(() => {
          const visibleEditors = Array.from(
            document.querySelectorAll<HTMLElement>('[data-testid="sql-editor-content"]'),
          ).filter((editor) => editor.getClientRects().length > 0);
          return visibleEditors.at(-1)?.textContent ?? '';
        });
        return (
          definition.includes(name) && definition.toLowerCase().includes(expectedText.toLowerCase())
        );
      },
      {
        timeout: 20_000,
        timeoutMsg: `Opening MySQL ${kind} ${name} did not show its expected definition`,
      },
    );
    expect(definition).toContain(name);
    expect(definition.toLowerCase()).toContain(expectedText.toLowerCase());
  }

  async function cleanup(): Promise<void> {
    const cleanupErrors: string[] = [];
    if (treeSession) {
      try {
        await disconnectBackend(treeSession);
      } catch {
        cleanupErrors.push('could not disconnect the Schema Tree session');
      }
      treeSession = undefined;
    }

    try {
      if (!adminSession && adminConfigSaved) adminSession = await connectBackend(adminId);
      if (adminSession && databaseCreated) {
        await withSafeModeOff(async () => {
          await execute(adminSession!, `DROP DATABASE IF EXISTS \`${database}\``);
        });
        databaseCreated = false;
        const remaining = await execute(
          adminSession,
          `SELECT COUNT(*) AS value FROM information_schema.SCHEMATA ` +
            `WHERE SCHEMA_NAME = '${database}'`,
        );
        if (queryScalar(remaining, 'value') !== 0) {
          cleanupErrors.push(`temporary database ${database} still exists after teardown`);
        }
      }
    } catch {
      cleanupErrors.push(`could not drop temporary database ${database}`);
    } finally {
      if (adminSession) {
        await disconnectBackend(adminSession).catch(() => undefined);
        adminSession = undefined;
      }
    }

    for (const [id, saved] of [
      [connectionId, treeConfigSaved],
      [adminId, adminConfigSaved],
    ] as const) {
      if (saved) {
        try {
          await invokeBackend('delete_connection', { id });
        } catch {
          cleanupErrors.push(`could not delete temporary connection ${id}`);
        }
      }
    }
    if (mainWindow) {
      await closeExtraWindows(mainWindow).catch(() => undefined);
      await browser.switchToWindow(mainWindow).catch(() => undefined);
    }

    if (cleanupErrors.length > 0) throw new Error(cleanupErrors.join('; '));
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();

    const adminConfig = mysqlConfig(adminId, `Schema Tree admin ${stamp}`, ADMIN_DATABASE);
    await invokeBackend('save_connection', { config: adminConfig });
    adminConfigSaved = true;
    adminSession = await connectBackend(adminId);
    await withSafeModeOff(async () => {
      databaseCreated = true;
      await execute(
        adminSession!,
        `CREATE DATABASE \`${database}\` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci`,
      );
    });

    const treeConfig = mysqlConfig(connectionId, connectionName, database);
    await invokeBackend('save_connection', { config: treeConfig });
    treeConfigSaved = true;
    treeSession = await connectBackend(connectionId);

    await withSafeModeOff(async () => {
      await execute(
        treeSession!,
        `CREATE TABLE \`${parentTable}\` (` +
          '`id` INT NOT NULL PRIMARY KEY, `code` VARCHAR(64) NOT NULL UNIQUE' +
          ') ENGINE=InnoDB',
      );
      await execute(
        treeSession!,
        `CREATE TABLE \`${table}\` (` +
          '`id` BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY, ' +
          '`parent_id` INT NOT NULL, `code` VARCHAR(64) NOT NULL, ' +
          '`status` VARCHAR(16) NOT NULL, `updated_at` TIMESTAMP NULL DEFAULT NULL, ' +
          `KEY \`idx_${prefix}_parent_status\` (\`parent_id\`, \`status\`), ` +
          `CONSTRAINT \`fk_${prefix}_parent\` FOREIGN KEY (\`parent_id\`) ` +
          `REFERENCES \`${parentTable}\` (\`id\`)` +
          ') ENGINE=InnoDB',
      );
      await execute(
        treeSession!,
        `INSERT INTO \`${parentTable}\` (id, code) VALUES (1, 'journey-parent')`,
      );
      await execute(
        treeSession!,
        `INSERT INTO \`${table}\` (id, parent_id, code, status) VALUES ` +
          `(11, 1, 'journey-active', 'active'), (12, 1, 'journey-inactive', 'inactive')`,
      );
      await execute(
        treeSession!,
        `CREATE VIEW \`${view}\` AS SELECT id, parent_id, code, status ` +
          `FROM \`${table}\` WHERE status = 'active'`,
      );
      await execute(
        treeSession!,
        `CREATE FUNCTION \`${functionName}\` (p_value VARCHAR(64)) ` +
          `RETURNS VARCHAR(64) DETERMINISTIC RETURN UPPER(TRIM(p_value))`,
      );
      await execute(
        treeSession!,
        `CREATE PROCEDURE \`${procedureName}\` (IN p_parent_id INT) ` +
          `UPDATE \`${table}\` SET status = 'processed' ` +
          `WHERE parent_id = p_parent_id AND status = 'inactive'`,
      );
      await execute(
        treeSession!,
        `CREATE TRIGGER \`${triggerName}\` BEFORE UPDATE ON \`${table}\` ` +
          'FOR EACH ROW SET NEW.`updated_at` = CURRENT_TIMESTAMP',
      );
    });

    await browser.refresh();
    await browser.pause(1_500);
    await openConnectionsWorkspace(mainWindow);
    await browser.waitUntil(async () => Boolean(await findCardByName(connectionName)), {
      timeout: 20_000,
      timeoutMsg: `Saved MySQL connection card ${connectionName} did not appear after refresh`,
    });
    if (!(await dblclickConnByExactName(connectionName))) {
      throw new Error(`Saved MySQL connection card ${connectionName} could not be opened`);
    }
    await waitForConnectionToolbar();
    await expandConnectedConnectionInNavigator(connectionName);
    for (const category of ['tables', 'views', 'function', 'procedure', 'trigger']) {
      await expandSchemaCategory(category, database, database);
    }
  });

  after(async () => {
    await cleanup();
  });

  it('shows tables, relationships, views and every supported MySQL routine category', async () => {
    await waitForNode('table', parentTable);
    await waitForNode('table', table);
    await waitForNode('view', view);
    await waitForNode('function', functionName);
    await waitForNode('procedure', procedureName);
    await waitForNode('trigger', triggerName);

    await $(
      `[data-testid="schema-tree-node"][data-tree-node="table"][data-item-name="${table}"]`,
    ).click();
    await browser.waitUntil(async () => (await $('body').getText()).includes('journey-active'), {
      timeout: 20_000,
      timeoutMsg: `Opening MySQL table ${table} did not show its seeded row`,
    });

    await $(
      `[data-testid="schema-tree-node"][data-tree-node="view"][data-item-name="${view}"]`,
    ).click();
    await browser.waitUntil(async () => (await $('body').getText()).includes('journey-active'), {
      timeout: 20_000,
      timeoutMsg: `Opening MySQL view ${view} did not show its active row`,
    });

    const tableCount = await execute(treeSession!, `SELECT COUNT(*) AS value FROM \`${table}\``);
    expect(queryScalar(tableCount, 'value')).toBe(2);

    const foreignKeyCount = await execute(
      treeSession!,
      `SELECT COUNT(*) AS value FROM information_schema.KEY_COLUMN_USAGE ` +
        `WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '${table}' ` +
        `AND CONSTRAINT_NAME = 'fk_${prefix}_parent'`,
    );
    expect(queryScalar(foreignKeyCount, 'value')).toBe(1);

    const indexCount = await execute(
      treeSession!,
      `SELECT COUNT(*) AS value FROM information_schema.STATISTICS ` +
        `WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '${table}' ` +
        `AND INDEX_NAME = 'idx_${prefix}_parent_status'`,
    );
    expect(queryScalar(indexCount, 'value')).toBe(2);

    const viewRows = await execute(treeSession!, `SELECT id, code FROM \`${view}\``);
    const rows = parseQueryRows(viewRows);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.[1]).toBe('journey-active');

    const functionRows = parseQueryRows(
      await execute(treeSession!, `SELECT \`${functionName}\`(' lower ') AS value`),
    );
    expect(functionRows[0]?.[0]).toBe('LOWER');

    await withSafeModeOff(async () => {
      await execute(treeSession!, `CALL \`${procedureName}\`(1)`);
    });
    const procedureEffect = await execute(
      treeSession!,
      `SELECT COUNT(*) AS value FROM \`${table}\` ` +
        `WHERE parent_id = 1 AND status = 'processed'`,
    );
    expect(queryScalar(procedureEffect, 'value')).toBe(1);

    await execute(treeSession!, `UPDATE \`${table}\` SET code = code WHERE id = 11`);
    const triggerRows = parseQueryRows(
      await execute(
        treeSession!,
        `SELECT updated_at IS NOT NULL AS value FROM \`${table}\` WHERE id = 11`,
      ),
    );
    expect(Number(triggerRows[0]?.[0])).toBe(1);

    let missingTableRejected = false;
    try {
      await execute(treeSession!, `SELECT * FROM \`${prefix}_missing\``);
    } catch {
      missingTableRejected = true;
    }
    expect(missingTableRejected).toBe(true);
  });

  it('opens function, procedure and trigger definitions from the Schema Tree', async () => {
    await assertObjectDefinition('function', functionName, 'UPPER(TRIM');
    await assertObjectDefinition('procedure', procedureName, 'UPDATE');
    await assertObjectDefinition('trigger', triggerName, 'CURRENT_TIMESTAMP');
  });

  it('returns the view definition through the MySQL object DDL command', async () => {
    const response = await invokeBackend<{ data: { ddl: string } }>('execute_driver_command', {
      request: {
        dbSessionId: treeSession,
        command: 'get_object_ddl',
        input: { kind: 'view', name: view, schema: database },
      },
    });
    expect(response.data.ddl.trim().length).toBeGreaterThan(0);
    expect(response.data.ddl).toContain(table);
    expect(response.data.ddl.toLowerCase()).toContain("status = 'active'");
  });
});
