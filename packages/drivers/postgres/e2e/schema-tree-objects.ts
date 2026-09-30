/**
 * PostgreSQL Schema Tree journey for tables, views, routines, triggers,
 * sequences, and user-defined types. Every run owns a uniquely named schema
 * and removes it even when setup or a UI assertion fails.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  clickNavigatorRefresh,
  closeExtraWindows,
  connectConfig,
  disconnectBackend,
  expandConnectedConnectionInNavigator,
  expandSchemaCategory,
  invokeBackend,
  openConnectionsWorkspace,
  waitForConnectionToolbar,
  withSafeModeOff,
} from '../../../../e2e/helpers.js';

type QueryResultPayload = {
  rows?: unknown[][];
  results?: Array<{ rows?: unknown[][] }>;
  data?: unknown;
};

type ObjectDdlRequest = {
  dbSessionId: string;
  kind: string;
  name: string;
  schema: string;
  signature?: string;
  targetSchema?: string;
  targetName?: string;
};

function quoteIdent(identifier: string): string {
  return `"${identifier.replace(/"/g, '""')}"`;
}

function parseRows(payload: QueryResultPayload): unknown[][] {
  if (Array.isArray(payload.rows)) return payload.rows;
  const resultRows = payload.results?.[0]?.rows;
  return Array.isArray(resultRows) ? resultRows : [];
}

async function executeSql(dbSessionId: string, sql: string): Promise<QueryResultPayload> {
  return invokeBackend<QueryResultPayload>('execute_query', { dbSessionId, sql });
}

async function readObjectDdl(request: ObjectDdlRequest): Promise<string> {
  return invokeBackend<string>('get_object_ddl', request);
}

async function waitForEditorText(expected: string): Promise<void> {
  await browser.waitUntil(
    async () =>
      browser.execute((token: string) => {
        const editor = document.querySelector('[data-testid="sql-editor-content"]');
        return (editor?.textContent ?? '').toLowerCase().includes(token.toLowerCase());
      }, expected),
    {
      timeout: 15000,
      timeoutMsg: `Object definition editor did not contain ${expected}`,
    },
  );
}

describe('PostgreSQL Schema Tree objects', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('creates, browses, opens definitions, and removes an isolated object schema', async () => {
    const stamp = Date.now().toString(36);
    const schema = `e2e_schema_tree_${stamp}`;
    const connectionId = `e2e-schema-tree-${stamp}`;
    const connectionName = `E2E PostgreSQL Schema Tree ${stamp}`;
    const database = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';
    const dbSessionIdRef: { value?: string } = {};
    let primaryFailure: unknown;
    const cleanupFailures: string[] = [];

    const name = {
      groups: 'tree_groups',
      entries: 'tree_entries',
      view: 'tree_active_entries',
      function: 'tree_normalize_code',
      procedure: 'tree_archive_entry',
      triggerFunction: 'tree_touch_updated_at',
      trigger: 'tree_entries_touch_updated_at',
      sequence: 'tree_entry_id_seq',
      enum: 'tree_entry_state',
      domain: 'tree_entry_code',
      index: 'tree_entries_state_group_idx',
    };

    try {
      await invokeBackend('save_connection', {
        config: {
          id: connectionId,
          name: connectionName,
          databaseType: 'postgresql',
          host: process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1',
          port: Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432,
          username: process.env.E2E_PG_USER || process.env.PG_USER || 'postgres',
          password: process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '',
          database,
          group: 'E2E Schema Tree',
          sslMode: 'disable',
          options: {},
        },
      });

      await browser.refresh();
      await browser.pause(1000);
      await openConnectionsWorkspace(mainWindow);
      await clickCardConnectButton(connectionName);
      await waitForConnectionToolbar();
      await expandConnectedConnectionInNavigator(connectionName);
      dbSessionIdRef.value = await connectConfig(connectionId);
      const dbSessionId = dbSessionIdRef.value;

      const qSchema = quoteIdent(schema);
      const qName = (object: string) => `${qSchema}.${quoteIdent(object)}`;
      const statements = [
        `CREATE SCHEMA ${qSchema}`,
        `CREATE TYPE ${qName(name.enum)} AS ENUM ('active', 'archived')`,
        `CREATE DOMAIN ${qName(name.domain)} AS text CHECK (VALUE ~ '^[A-Z0-9-]{1,24}$')`,
        `CREATE SEQUENCE ${qName(name.sequence)} START WITH 1 INCREMENT BY 1`,
        `CREATE TABLE ${qName(name.groups)} (
          group_id integer PRIMARY KEY,
          group_name text NOT NULL UNIQUE
        )`,
        `CREATE TABLE ${qName(name.entries)} (
          entry_id integer PRIMARY KEY DEFAULT nextval('${schema}.${name.sequence}'),
          group_id integer NOT NULL REFERENCES ${qName(name.groups)} (group_id),
          code ${qName(name.domain)} NOT NULL UNIQUE,
          title text NOT NULL,
          state ${qName(name.enum)} NOT NULL DEFAULT 'active',
          updated_at timestamptz NOT NULL DEFAULT now()
        )`,
        `ALTER SEQUENCE ${qName(name.sequence)} OWNED BY ${qName(name.entries)}.entry_id`,
        `CREATE INDEX ${quoteIdent(name.index)} ON ${qName(name.entries)} (state, group_id)`,
        `CREATE VIEW ${qName(name.view)} AS
          SELECT e.entry_id, g.group_name, e.code, e.title, e.state, e.updated_at
          FROM ${qName(name.entries)} AS e
          JOIN ${qName(name.groups)} AS g USING (group_id)
          WHERE e.state = 'active'`,
        `CREATE FUNCTION ${qName(name.function)}(input_code text)
          RETURNS text LANGUAGE sql IMMUTABLE
          AS $$ SELECT upper(btrim(input_code)) $$`,
        `CREATE PROCEDURE ${qName(name.procedure)}(p_entry_id integer)
          LANGUAGE plpgsql AS $$
          BEGIN
            UPDATE ${qName(name.entries)}
            SET state = 'archived', updated_at = now()
            WHERE entry_id = p_entry_id;
          END;
          $$`,
        `CREATE FUNCTION ${qName(name.triggerFunction)}()
          RETURNS trigger LANGUAGE plpgsql AS $$
          BEGIN
            NEW.updated_at := now();
            RETURN NEW;
          END;
          $$`,
        `CREATE TRIGGER ${quoteIdent(name.trigger)}
          BEFORE UPDATE ON ${qName(name.entries)}
          FOR EACH ROW EXECUTE FUNCTION ${qName(name.triggerFunction)}()`,
        `INSERT INTO ${qName(name.groups)} (group_id, group_name)
          VALUES (101, 'Schema Tree group')`,
        `INSERT INTO ${qName(name.entries)} (entry_id, group_id, code, title, state)
          VALUES (501, 101, 'TREE-001', 'Active Schema Tree fixture', 'active'),
                 (502, 101, 'TREE-002', 'Archived Schema Tree fixture', 'archived')`,
        `SELECT setval('${schema}.${name.sequence}', 502, true)`,
      ];

      for (const sql of statements) {
        await executeSql(dbSessionId, sql);
      }

      // A deliberately invalid read checks that PostgreSQL errors surface to
      // the E2E caller; the finally block must still drop the partial schema.
      await expect(
        executeSql(dbSessionId, `SELECT * FROM ${qName('tree_missing_relation')}`),
      ).rejects.toThrow();

      await clickNavigatorRefresh();
      await expandConnectedConnectionInNavigator(connectionName);
      for (const category of [
        'tables',
        'views',
        'function',
        'procedure',
        'trigger',
        'sequence',
        'type',
      ]) {
        await expandSchemaCategory(category, schema, database);
      }

      const treeNode = (kind: string, objectName: string) =>
        $(
          `[data-testid="schema-tree-node"][data-tree-node="${kind}"][data-item-name="${objectName}"]`,
        );
      for (const [kind, objectName] of [
        ['table', name.groups],
        ['table', name.entries],
        ['view', name.view],
        ['function', name.function],
        ['function', name.triggerFunction],
        ['procedure', name.procedure],
        ['trigger', name.trigger],
        ['sequence', name.sequence],
        ['type', name.enum],
        ['type', name.domain],
      ]) {
        const node = treeNode(kind, objectName);
        await node.waitForDisplayed({
          timeout: 15000,
          timeoutMsg: `Schema Tree did not show ${kind} ${schema}.${objectName}`,
        });
        await expect(node).toBeDisplayed();
      }

      const schemaVisible = await browser.execute(
        (targetSchema: string) =>
          Array.from(document.querySelectorAll('[data-tree-node="schema"]')).some(
            (node) => node.getAttribute('data-schema-name') === targetSchema,
          ),
        schema,
      );
      expect(schemaVisible).toBe(true);

      const tableNode = treeNode('table', name.entries);
      await tableNode.click();
      await $('[data-testid="sub-tab-data"]').waitForDisplayed({ timeout: 15000 });
      await browser.waitUntil(async () => (await $('body').getText()).includes('TREE-001'), {
        timeout: 15000,
        timeoutMsg: 'The opened PostgreSQL table did not show its seeded row',
      });

      const viewDdl = await readObjectDdl({
        dbSessionId,
        kind: 'view',
        name: name.view,
        schema,
      });
      expect(viewDdl.toLowerCase()).toContain(name.entries);

      const sequenceDdl = await readObjectDdl({
        dbSessionId,
        kind: 'sequence',
        name: name.sequence,
        schema,
      });
      expect(sequenceDdl).toContain('CREATE SEQUENCE');
      expect(sequenceDdl).toContain(name.sequence);

      const enumDdl = await readObjectDdl({
        dbSessionId,
        kind: 'type',
        name: name.enum,
        schema,
      });
      expect(enumDdl).toContain('CREATE TYPE');
      expect(enumDdl).toContain(name.enum);
      expect(enumDdl).toContain("'active'");

      const domainDdl = await readObjectDdl({
        dbSessionId,
        kind: 'type',
        name: name.domain,
        schema,
      });
      expect(domainDdl).toContain('CREATE DOMAIN');
      expect(domainDdl).toContain(name.domain);

      for (const [kind, objectName] of [
        ['function', name.function],
        ['procedure', name.procedure],
        ['trigger', name.trigger],
      ]) {
        const node = treeNode(kind, objectName);
        await node.click();
        await waitForEditorText(objectName);
        const editorText = await browser.execute(
          () => document.querySelector('[data-testid="sql-editor-content"]')?.textContent ?? '',
        );
        expect(editorText.toLowerCase()).toContain(objectName.toLowerCase());
      }

      const counts = parseRows(
        await executeSql(
          dbSessionId,
          `SELECT
          (SELECT count(*) FROM ${qName(name.entries)})::int,
          (SELECT count(*) FROM ${qName(name.view)})::int`,
        ),
      );
      expect(counts[0]?.[0]).toBe(2);
      expect(counts[0]?.[1]).toBe(1);
    } catch (error) {
      primaryFailure = error;
    } finally {
      const dbSessionId = dbSessionIdRef.value;
      if (dbSessionId) {
        try {
          await withSafeModeOff(() =>
            executeSql(dbSessionId, `DROP SCHEMA IF EXISTS ${quoteIdent(schema)} CASCADE`),
          );
          const remaining = parseRows(
            await executeSql(
              dbSessionId,
              `SELECT count(*)::int FROM information_schema.schemata WHERE schema_name = '${schema}'`,
            ),
          );
          if (Number(remaining[0]?.[0]) !== 0) {
            cleanupFailures.push(`temporary schema ${schema} remained after teardown`);
          }
        } catch (error) {
          cleanupFailures.push(
            `could not remove temporary schema ${schema}: ${error instanceof Error ? error.name : 'database error'}`,
          );
        }
        try {
          await disconnectBackend(dbSessionId);
        } catch (error) {
          cleanupFailures.push(
            `could not disconnect temporary PostgreSQL session: ${error instanceof Error ? error.name : 'session error'}`,
          );
        }
      }

      try {
        await invokeBackend('delete_connection', { id: connectionId });
      } catch (error) {
        cleanupFailures.push(
          `could not remove temporary connection config: ${error instanceof Error ? error.name : 'config error'}`,
        );
      }
    }

    if (primaryFailure) throw primaryFailure;
    if (cleanupFailures.length > 0) throw new Error(cleanupFailures.join('; '));
  });
});
