/** Unified schema-object planner journeys over live PostgreSQL and MySQL fixtures. */
import { expect, browser, $ } from '@wdio/globals';
import {
  advanceSchemaDiffToReview,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  clickSchemaDiffNext,
  closeExtraWindows,
  disconnectBackend,
  deploySchemaDiffPlan,
  invokeBackend,
  openSchemaDiffWindow,
  parseQueryRows,
  queryScalar,
  selectSchemaDiffEndpoints,
  setSchemaDiffTables,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';
import {
  MYSQL_SYNC_DB,
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  mysqlConnectionConfig,
  pgConnectionConfig,
  teardownSchemaDiffFixture,
} from '../lib/schemaDiffFixtures.js';

type Dialect = 'postgresql' | 'mysql';

const MYSQL_SOURCE_DB = 'datazen_sync_mysql_src';

function sanitizeFixtureDiagnostic(message: string): string {
  return message
    .replace(/((?:password|passwd|pwd)\s*[:=]\s*)[^\s,;]*/gi, '$1[redacted]')
    .replace(/\/\/([^/:\s]+):([^@\s]+)@/g, '//$1:[redacted]@')
    .replace(/\s+/g, ' ')
    .slice(0, 240);
}

type Fixture = {
  dialect: Dialect;
  sourceId: string;
  targetId: string;
  sourceName: string;
  targetName: string;
  parentTable: string;
  table: string;
  foreignKey: string;
  view: string;
  type: string | null;
  sequence: string | null;
  routine: string | null;
  trigger: string | null;
  sourceConfig: ReturnType<typeof pgConnectionConfig> | ReturnType<typeof mysqlConnectionConfig>;
  targetConfig: ReturnType<typeof pgConnectionConfig> | ReturnType<typeof mysqlConnectionConfig>;
};

type MysqlCatalogFixture = {
  sourceId: string;
  targetId: string;
  sourceName: string;
  targetName: string;
  functionName: string;
  procedureName: string;
  triggerName: string;
  triggerTable: string;
  viewName: string;
  sourceConfig: ReturnType<typeof mysqlConnectionConfig>;
  targetConfig: ReturnType<typeof mysqlConnectionConfig>;
};

function createMysqlCatalogFixture(): MysqlCatalogFixture {
  const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
  const sourceId = `e2e_unified_mysql_catalog_src_${stamp}`;
  const targetId = `e2e_unified_mysql_catalog_tgt_${stamp}`;
  const sourceName = `SD-UNIFIED-MYSQL-CATALOG-SRC-${stamp}`;
  const targetName = `SD-UNIFIED-MYSQL-CATALOG-TGT-${stamp}`;
  return {
    sourceId,
    targetId,
    sourceName,
    targetName,
    functionName: `sd_u_${stamp}_fn`,
    procedureName: `sd_u_${stamp}_proc`,
    triggerName: `sd_u_${stamp}_trg`,
    triggerTable: `sd_u_${stamp}_table`,
    viewName: `sd_u_${stamp}_view`,
    sourceConfig: mysqlConnectionConfig(sourceId, sourceName, 'datazen_sync_mysql_src'),
    targetConfig: mysqlConnectionConfig(targetId, targetName, MYSQL_SYNC_DB),
  };
}

async function mysqlCatalogFixtureCount(
  connectionId: string,
  fixture: MysqlCatalogFixture,
): Promise<number> {
  const result = await executeOn(
    connectionId,
    `SELECT (
       (SELECT count(*) FROM information_schema.routines
        WHERE routine_schema=DATABASE() AND routine_name IN ('${fixture.functionName}','${fixture.procedureName}')) +
       (SELECT count(*) FROM information_schema.triggers
        WHERE trigger_schema=DATABASE() AND trigger_name='${fixture.triggerName}') +
       (SELECT count(*) FROM information_schema.views
        WHERE table_schema=DATABASE() AND table_name='${fixture.viewName}') +
       (SELECT count(*) FROM information_schema.tables
        WHERE table_schema=DATABASE() AND table_name='${fixture.triggerTable}' AND table_type='BASE TABLE')
     ) AS c`,
  );
  return queryScalar(result, 'c');
}

async function dropMysqlCatalogFixture(session: string, fixture: MysqlCatalogFixture) {
  for (const sql of [
    `DROP VIEW IF EXISTS ${fixture.viewName}`,
    `DROP TRIGGER IF EXISTS ${fixture.triggerName}`,
    `DROP PROCEDURE IF EXISTS ${fixture.procedureName}`,
    `DROP FUNCTION IF EXISTS ${fixture.functionName}`,
    `DROP TABLE IF EXISTS ${fixture.triggerTable}`,
  ]) {
    await dropExactFixtureObject(session, sql);
  }
}

async function runMysqlCatalogSmokeJourney(fixture: MysqlCatalogFixture) {
  try {
    await Promise.all([
      invokeBackend('save_connection', { config: fixture.sourceConfig }),
      invokeBackend('save_connection', { config: fixture.targetConfig }),
    ]);
    const source = await invokeBackend<string>('connect', { connectionId: fixture.sourceId });
    try {
      await withSafeModeOff(async () => {
        await dropMysqlCatalogFixture(source, fixture);
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TABLE ${fixture.triggerTable} (id INT PRIMARY KEY) ENGINE=InnoDB`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE FUNCTION ${fixture.functionName}() RETURNS INT DETERMINISTIC RETURN 1`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE PROCEDURE ${fixture.procedureName}() SELECT 1`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TRIGGER ${fixture.triggerName} BEFORE INSERT ON ${fixture.triggerTable} FOR EACH ROW SET NEW.id = NEW.id`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE VIEW ${fixture.viewName} AS SELECT 1 AS id`,
        });
      });
    } finally {
      await disconnectBackend(source);
    }

    const expected = [
      ['function', fixture.functionName],
      ['procedure', fixture.procedureName],
      ['trigger', fixture.triggerName],
      ['view', fixture.viewName],
    ] as const;
    for (const [kind, name] of expected) {
      const session = await invokeBackend<string>('connect', { connectionId: fixture.sourceId });
      try {
        const objects = await invokeBackend<{
          data: {
            objects: Array<{
              schema: string | null;
              name: string;
              targetSchema?: string | null;
              targetName?: string | null;
            }>;
          };
        }>('execute_driver_command', {
          request: { dbSessionId: session, command: 'list_objects', input: { kind } },
        });
        const matches = objects.data.objects.filter((object) => object.name === name);
        expect(matches).toHaveLength(1);
        // MySQL has no schema layer inside a database: `information_schema.*_SCHEMA`
        // columns name databases, and the driver lists only the current database
        // (`WHERE ROUTINE_SCHEMA = DATABASE()`), projecting `NULL AS schema`.
        // Object ownership therefore lives in the selected database, and the
        // unified planner resolves the configured scope from it — an object is
        // never addressed as `db.object` through this field.
        expect(matches[0]?.schema).toBeNull();
        if (kind === 'trigger') {
          expect([null, 'datazen_sync_mysql_src']).toContain(matches[0]?.targetSchema ?? null);
          expect(matches[0]?.targetName).toBe(fixture.triggerTable);
        }
        console.log(
          `[SD-UNIFIED] mysql catalog kind=${kind} schema=${matches[0]?.schema} name=${name}`,
        );
      } finally {
        await disconnectBackend(session);
      }
    }
  } finally {
    for (const connectionId of [fixture.sourceId, fixture.targetId]) {
      try {
        const session = await invokeBackend<string>('connect', { connectionId });
        try {
          await withSafeModeOff(async () => dropMysqlCatalogFixture(session, fixture));
        } finally {
          await disconnectBackend(session);
        }
      } catch {
        // A partial fixture setup may leave one connection unavailable.
      }
    }
    try {
      const remaining = await Promise.all(
        [fixture.sourceId, fixture.targetId].map((id) => mysqlCatalogFixtureCount(id, fixture)),
      );
      expect(remaining).toEqual([0, 0]);
      console.log(
        `[SD-UNIFIED] exact MySQL catalog cleanup source_remaining=${remaining[0]} target_remaining=${remaining[1]}`,
      );
    } finally {
      await teardownSchemaDiffFixture([fixture.sourceId, fixture.targetId], [fixture.triggerTable]);
    }
  }
}

function createFixture(dialect: Dialect): Fixture {
  const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
  const sourceId = `e2e_unified_${dialect}_src_${stamp}`;
  const targetId = `e2e_unified_${dialect}_tgt_${stamp}`;
  const sourceName = `SD-UNIFIED-${dialect}-SRC-${stamp}`;
  const targetName = `SD-UNIFIED-${dialect}-TGT-${stamp}`;
  const database = dialect === 'postgresql' ? PG_SYNC_DB : MYSQL_SOURCE_DB;
  const targetDatabase = dialect === 'postgresql' ? PG_SYNC_TGT_DB : MYSQL_SYNC_DB;
  const parentTable = `sd_unified_${dialect}_${stamp}_parent`;
  const table = `sd_unified_${dialect}_${stamp}_child`;
  const foreignKey = `fk_unified_${dialect}_${stamp}`;
  const view = `sd_unified_${dialect}_${stamp}_view`;
  const type = dialect === 'postgresql' ? `sd_unified_${stamp}_state` : null;
  const sequence = dialect === 'postgresql' ? `${table}_id_seq` : null;
  const routine = dialect === 'postgresql' ? `sd_unified_${stamp}_audit` : null;
  const trigger = dialect === 'postgresql' ? `sd_unified_${stamp}_audit_trg` : null;
  return {
    dialect,
    sourceId,
    targetId,
    sourceName,
    targetName,
    parentTable,
    table,
    foreignKey,
    view,
    type,
    sequence,
    routine,
    trigger,
    sourceConfig:
      dialect === 'postgresql'
        ? pgConnectionConfig(sourceId, sourceName, database)
        : mysqlConnectionConfig(sourceId, sourceName, database),
    targetConfig:
      dialect === 'postgresql'
        ? pgConnectionConfig(targetId, targetName, targetDatabase)
        : mysqlConnectionConfig(targetId, targetName, targetDatabase),
  };
}

async function executeOn(connectionId: string, sql: string): Promise<QueryResultPayload> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    return await invokeBackend<QueryResultPayload>('execute_query', { dbSessionId: session, sql });
  } finally {
    await disconnectBackend(session);
  }
}

async function countOnTarget(fixture: Fixture, sql: string): Promise<number> {
  return queryScalar(await executeOn(fixture.targetId, sql), 'c');
}

async function selectSourceObject(
  kind: 'view' | 'type' | 'sequence' | 'function' | 'trigger',
  name: string,
) {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (args: { kind: string; name: string }) => {
          const row = Array.from(
            document.querySelectorAll<HTMLElement>(
              `[data-testid^="schema-diff-object-row-source-${args.kind}-"]`,
            ),
          ).some((candidate) => candidate.textContent?.includes(args.name));
          const error = document.querySelector(
            `[data-testid="schema-diff-object-error-source-${args.kind}"]`,
          );
          return row || Boolean(error);
        },
        { kind, name },
      ),
    { timeout: 30000, timeoutMsg: `waiting for source ${kind} catalog row ${name}` },
  );
  const rowTestId = await browser.execute(
    (objectKind: string, objectName: string) => {
      const rows = Array.from(
        document.querySelectorAll<HTMLElement>(
          `[data-testid^="schema-diff-object-row-source-${objectKind}-"]`,
        ),
      );
      const row = rows.find((candidate) => candidate.textContent?.includes(objectName));
      const details = row?.closest('details');
      if (details) details.open = true;
      return row?.getAttribute('data-testid') ?? null;
    },
    kind,
    name,
  );
  if (!rowTestId) {
    const catalogError = await browser
      .$(`[data-testid="schema-diff-object-error-source-${kind}"]`)
      .getText()
      .catch(() => '');
    throw new Error(`source ${kind} ${name} is absent from the live catalog; ${catalogError}`);
  }
  const row = await $(`[data-testid="${rowTestId}"]`);
  await row.waitForDisplayed({ timeout: 15000 });
  const checkbox = await row.$('input[type="checkbox"]');
  if (!(await checkbox.isSelected())) await checkbox.click();
}

async function fixtureCount(connectionId: string, fixture: Fixture): Promise<number> {
  const sql =
    fixture.dialect === 'postgresql'
      ? `SELECT (
           (SELECT count(*) FROM pg_class WHERE relkind IN ('r','p') AND relname IN ('${fixture.parentTable}','${fixture.table}')) +
           (SELECT count(*) FROM pg_class WHERE relkind='v' AND relname='${fixture.view}') +
           (SELECT count(*) FROM pg_class WHERE relkind='S' AND relname='${fixture.sequence}') +
           (SELECT count(*) FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public' AND t.typname='${fixture.type}') +
           (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.proname='${fixture.routine}') +
           (SELECT count(*) FROM pg_trigger tr JOIN pg_class c ON c.oid=tr.tgrelid WHERE NOT tr.tgisinternal AND tr.tgname='${fixture.trigger}' AND c.relname='${fixture.table}')
         )::int AS c`
      : `SELECT (
           (SELECT count(*) FROM information_schema.tables WHERE table_schema=DATABASE() AND table_name IN ('${fixture.parentTable}','${fixture.table}')) +
           (SELECT count(*) FROM information_schema.views WHERE table_schema=DATABASE() AND table_name='${fixture.view}')
         ) AS c`;
  return queryScalar(await executeOn(connectionId, sql), 'c');
}

async function dropExactFixtureObject(session: string, sql: string) {
  try {
    await invokeBackend('execute_query', { dbSessionId: session, sql });
  } catch {
    // Optional fixture object may not exist yet or may already have been removed.
  }
}

async function dropFixtureObjects(session: string, fixture: Fixture) {
  await dropExactFixtureObject(session, `DROP VIEW IF EXISTS ${fixture.view}`);
  if (fixture.trigger) {
    await dropExactFixtureObject(
      session,
      `DROP TRIGGER IF EXISTS ${fixture.trigger} ON ${fixture.table}`,
    );
  }
  // Drop the dependent child first; PostgreSQL removes its owned serial sequence with it.
  await dropExactFixtureObject(session, `DROP TABLE IF EXISTS ${fixture.table}`);
  await dropExactFixtureObject(session, `DROP TABLE IF EXISTS ${fixture.parentTable}`);
  if (fixture.sequence) {
    await dropExactFixtureObject(session, `DROP SEQUENCE IF EXISTS ${fixture.sequence}`);
  }
  if (fixture.type) {
    await dropExactFixtureObject(session, `DROP TYPE IF EXISTS ${fixture.type}`);
  }
  if (fixture.routine) {
    await dropExactFixtureObject(session, `DROP FUNCTION IF EXISTS ${fixture.routine}()`);
  }
}

async function setupFixture(fixture: Fixture) {
  const source = await invokeBackend<string>('connect', { connectionId: fixture.sourceId });
  const target = await invokeBackend<string>('connect', { connectionId: fixture.targetId });
  try {
    await withSafeModeOff(async () => {
      for (const session of [source, target]) await dropFixtureObjects(session, fixture);
      if (fixture.type) {
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TYPE ${fixture.type} AS ENUM ('queued', 'done')`,
        });
      }
      await invokeBackend('execute_query', {
        dbSessionId: source,
        sql: `CREATE TABLE ${fixture.parentTable} (id INT PRIMARY KEY)${fixture.dialect === 'mysql' ? ' ENGINE=InnoDB' : ''}`,
      });
      const stateType = fixture.type ?? 'VARCHAR(16)';
      const autoId = fixture.sequence ? 'BIGSERIAL' : 'BIGINT';
      await invokeBackend('execute_query', {
        dbSessionId: source,
        sql: `CREATE TABLE ${fixture.table} (id ${autoId} PRIMARY KEY, parent_id INT NOT NULL, state ${stateType} NOT NULL, CONSTRAINT ${fixture.foreignKey} FOREIGN KEY (parent_id) REFERENCES ${fixture.parentTable}(id))${fixture.dialect === 'mysql' ? ' ENGINE=InnoDB' : ''}`,
      });
      if (fixture.routine && fixture.trigger) {
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE FUNCTION ${fixture.routine}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END; $$`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TRIGGER ${fixture.trigger} BEFORE INSERT ON ${fixture.table} FOR EACH ROW EXECUTE FUNCTION ${fixture.routine}()`,
        });
      }
      await invokeBackend('execute_query', {
        dbSessionId: source,
        sql: `CREATE VIEW ${fixture.view} AS SELECT c.id, c.state, p.id AS parent_id FROM ${fixture.table} c JOIN ${fixture.parentTable} p ON p.id = c.parent_id`,
      });
    });
  } finally {
    await disconnectBackend(source);
    await disconnectBackend(target);
  }
}

async function assertSourceViewCatalog(fixture: Fixture) {
  const sourceViewCountSql =
    fixture.dialect === 'postgresql'
      ? `SELECT count(*)::int AS c FROM information_schema.views WHERE table_schema='public' AND table_name='${fixture.view}'`
      : `SELECT count(*) AS c FROM information_schema.views WHERE table_schema=DATABASE() AND table_name='${fixture.view}'`;
  const infoSchemaCount = queryScalar(await executeOn(fixture.sourceId, sourceViewCountSql), 'c');
  const listSql =
    fixture.dialect === 'postgresql'
      ? `SELECT schemaname AS schema, viewname AS name FROM pg_views WHERE schemaname NOT IN ('pg_catalog','information_schema') ORDER BY 1,2`
      : `SELECT TABLE_SCHEMA AS \`schema\`, TABLE_NAME AS name FROM information_schema.VIEWS WHERE TABLE_SCHEMA = DATABASE() ORDER BY 1,2`;
  const rawList = await executeOn(fixture.sourceId, listSql).catch(() => null);
  const rawListMatches = rawList
    ? parseQueryRows(rawList).filter((row) => String(row[1]) === fixture.view).length
    : 0;
  let customTypeCatalog = 'not-applicable';
  if (fixture.type) {
    const columnResult = await executeOn(
      fixture.sourceId,
      `SELECT data_type, udt_name FROM information_schema.columns WHERE table_schema='public' AND table_name='${fixture.table}' AND column_name='state'`,
    );
    const columnRow = parseQueryRows(columnResult)[0];
    customTypeCatalog = `${String(columnRow?.[0])}/${String(columnRow?.[1])}`;
  }
  const session = await invokeBackend<string>('connect', { connectionId: fixture.sourceId });
  let driverMatches = 0;
  let catalogError = '';
  let ddlContainsSemicolon: boolean | null = null;
  let ddlLength = 0;
  let ddlError = '';
  let viewDefinitionLength: number | null = null;
  let viewDefinitionError = '';
  try {
    const objects = await invokeBackend<{ data: { objects: Array<{ name: string }> } }>(
      'execute_driver_command',
      {
        request: {
          dbSessionId: session,
          command: 'list_objects',
          input: { kind: 'view' },
        },
      },
    );
    driverMatches = objects.data.objects.filter((object) => object.name === fixture.view).length;
  } catch (error) {
    catalogError = error instanceof Error ? error.message : String(error);
  } finally {
    try {
      const ddlResult = await invokeBackend<{ data: { ddl: string } }>(
        'execute_driver_command',
        {
          request: {
            dbSessionId: session,
            command: 'get_object_ddl',
            input: {
              kind: 'view',
              name: fixture.view,
              schema: fixture.dialect === 'postgresql' ? 'public' : null,
            },
          },
        },
      );
      const ddl = ddlResult.data.ddl;
      ddlLength = ddl.trim().length;
      ddlContainsSemicolon = ddl.includes(';');
    } catch (error) {
      ddlContainsSemicolon = null;
      ddlError = error instanceof Error ? error.message : String(error);
    }
    await disconnectBackend(session);
  }
  if (fixture.dialect === 'mysql') {
    try {
      const definition = await executeOn(
        fixture.sourceId,
        `SELECT VIEW_DEFINITION AS ddl FROM information_schema.VIEWS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='${fixture.view}'`,
      );
      const value = parseQueryRows(definition)[0]?.[0];
      viewDefinitionLength = value == null ? 0 : String(value).length;
    } catch (error) {
      viewDefinitionError = error instanceof Error ? error.message : String(error);
    }
  }
  const safeError = sanitizeFixtureDiagnostic(catalogError);
  console.log(
    `[SD-UNIFIED] source catalog dialect=${fixture.dialect} view_information_schema=${infoSchemaCount} view_raw_list_matches=${rawListMatches} view_driver_matches=${driverMatches} view_driver_error=${safeError || 'none'} ddl_length=${ddlLength} ddl_contains_semicolon=${ddlContainsSemicolon} ddl_error=${sanitizeFixtureDiagnostic(ddlError) || 'none'} view_definition_length=${viewDefinitionLength} view_definition_error=${sanitizeFixtureDiagnostic(viewDefinitionError) || 'none'} custom_type_column=${customTypeCatalog}`,
  );
  expect(infoSchemaCount).toBe(1);
  expect(rawListMatches).toBe(1);
  expect(driverMatches).toBe(1);
  // [tester] Prove Host get_object_ddl returns the view body, not just a visible catalog row.
  expect(ddlError).toBe('');
  expect(ddlLength).toBeGreaterThan(0);
}

async function cleanupFixture(fixture: Fixture, mainWindow: string) {
  for (const connectionId of [fixture.sourceId, fixture.targetId]) {
    try {
      const session = await invokeBackend<string>('connect', { connectionId });
      await withSafeModeOff(async () => dropFixtureObjects(session, fixture));
      await disconnectBackend(session);
    } catch {
      // A setup error may mean some of these exact fixture objects never existed.
    }
  }
  const remaining = await Promise.all(
    [fixture.sourceId, fixture.targetId].map((id) => fixtureCount(id, fixture)),
  );
  expect(remaining).toEqual([0, 0]);
  console.log(
    `[SD-UNIFIED] exact fixture cleanup dialect=${fixture.dialect} source_remaining=${remaining[0]} target_remaining=${remaining[1]}`,
  );
  await teardownSchemaDiffFixture(
    [fixture.sourceId, fixture.targetId],
    [fixture.table, fixture.parentTable],
  );
  await closeExtraWindows(mainWindow);
  await browser.switchToWindow(mainWindow);
}

async function addFixtureConnections(fixture: Fixture) {
  await Promise.all([
    invokeBackend('save_connection', { config: fixture.sourceConfig }),
    invokeBackend('save_connection', { config: fixture.targetConfig }),
  ]);
}

async function openObjectPicker(fixture: Fixture) {
  await openSchemaDiffWindow();
  await selectSchemaDiffEndpoints(fixture.sourceName, fixture.targetName);
  await clickSchemaDiffNext();
  await $('[data-testid="schema-diff-unified-object-picker"]').waitForDisplayed({ timeout: 15000 });
}

async function selectPositiveChain(fixture: Fixture) {
  await setSchemaDiffTables(`${fixture.parentTable}, ${fixture.table}`);
  if (fixture.type) await selectSourceObject('type', fixture.type);
  if (fixture.sequence) await selectSourceObject('sequence', fixture.sequence);
  if (fixture.routine) await selectSourceObject('function', fixture.routine);
  if (fixture.trigger) await selectSourceObject('trigger', fixture.trigger);
  await selectSourceObject('view', fixture.view);
}

async function readPlanStatements(): Promise<string[]> {
  return browser.execute(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>('[data-testid="schema-diff-plan-panel"] li pre'),
    ).map((element) => element.textContent?.trim() ?? ''),
  );
}

function statementIndex(statements: string[], regex: RegExp, name: string): number {
  return statements.findIndex((sql) => regex.test(sql) && sql.includes(name));
}

async function runPositiveJourney(fixture: Fixture, mainWindow: string) {
  await addFixtureConnections(fixture);
  try {
    await setupFixture(fixture);
    await assertSourceViewCatalog(fixture);
    await openObjectPicker(fixture);
    await selectPositiveChain(fixture);
    await clickSchemaDiffCompare();
    try {
      await clickSchemaDiffGeneratePlan();
    } catch (error) {
      if (fixture.dialect === 'mysql') {
        const diagnostics = await browser.execute(() => ({
          body: document.body.innerText.slice(-5000),
          alerts: Array.from(document.querySelectorAll('[role="alert"], .error-message'))
            .map((element) => element.textContent?.trim() ?? '')
            .filter(Boolean),
          testIds: Array.from(document.querySelectorAll<HTMLElement>('[data-testid]'))
            .map((element) => element.getAttribute('data-testid'))
            .filter((testId): testId is string =>
              Boolean(testId && testId.startsWith('schema-diff')),
            ),
        }));
        console.log(`[SD-UNIFIED] mysql plan generation diagnostic=${JSON.stringify(diagnostics)}`);
      }
      throw error;
    }

    const statements = await readPlanStatements();
    const parentIndex = statementIndex(statements, /\bCREATE\s+TABLE\b/i, fixture.parentTable);
    const tableIndex = statementIndex(statements, /\bCREATE\s+TABLE\b/i, fixture.table);
    const viewIndex = statementIndex(statements, /\bCREATE\s+VIEW\b/i, fixture.view);
    const fkIndex = statements.findIndex(
      (sql) => /\bFOREIGN\s+KEY\b/i.test(sql) && sql.includes(fixture.foreignKey),
    );
    if ([parentIndex, tableIndex, viewIndex, fkIndex].some((index) => index < 0)) {
      const diagnostics = await browser.execute(() => ({
        requirements:
          document.querySelector('[data-testid="schema-diff-plan-requirements"]')?.textContent ??
          '',
        errors: Array.from(document.querySelectorAll('.error-message'))
          .map((element) => element.textContent?.trim() ?? '')
          .filter(Boolean)
          .join(' | '),
        plan: document.querySelector('[data-testid="schema-diff-plan-panel"]')?.textContent ?? '',
      }));
      throw new Error(
        `reviewed plan missing one or more expected operations: parent=${parentIndex}, child=${tableIndex}, view=${viewIndex}, fk=${fkIndex}; requirements=${diagnostics.requirements}; errors=${diagnostics.errors}; plan=${diagnostics.plan}; statements=${statements.join('\n')}`,
      );
    }
    expect(parentIndex).toBeLessThan(tableIndex);
    expect(tableIndex).toBeLessThan(viewIndex);
    if (/\bALTER\s+TABLE\b/i.test(statements[fkIndex])) {
      expect(parentIndex).toBeLessThan(fkIndex);
      expect(tableIndex).toBeLessThan(fkIndex);
    } else {
      expect(fkIndex).toBe(tableIndex);
    }

    if (fixture.type) {
      const typeIndex = statementIndex(statements, /\bCREATE\s+TYPE\b/i, fixture.type);
      if (typeIndex < 0)
        throw new Error(`custom type operation missing:\n${statements.join('\n')}`);
      expect(typeIndex).toBeLessThan(tableIndex);
    }
    if (fixture.sequence) {
      const sequenceIndex = statementIndex(statements, /\bCREATE\s+SEQUENCE\b/i, fixture.sequence);
      const ownerIndex = statements.findIndex(
        (sql) =>
          /\bOWNED\s+BY\b/i.test(sql) &&
          sql.includes(fixture.sequence!) &&
          sql.includes(fixture.table),
      );
      if (sequenceIndex < 0)
        throw new Error(`sequence operation missing:\n${statements.join('\n')}`);
      expect(sequenceIndex).toBeLessThan(tableIndex);
      if (ownerIndex <= tableIndex) {
        throw new Error(
          `sequence ownership phase missing or ordered before owner table:\n${statements.join('\n')}`,
        );
      }
    }
    if (fixture.routine && fixture.trigger) {
      const routineIndex = statementIndex(
        statements,
        /\bCREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\b/i,
        fixture.routine,
      );
      const triggerIndex = statementIndex(statements, /\bCREATE\s+TRIGGER\b/i, fixture.trigger);
      if (routineIndex < 0)
        throw new Error(`function operation missing:\n${statements.join('\n')}`);
      if (triggerIndex < 0) throw new Error(`trigger operation missing:\n${statements.join('\n')}`);
      expect(routineIndex).toBeLessThan(triggerIndex);
    }

    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();

    const baseTablesSql =
      fixture.dialect === 'postgresql'
        ? `SELECT count(*)::int AS c FROM information_schema.tables WHERE table_schema='public' AND table_name IN ('${fixture.parentTable}','${fixture.table}') AND table_type='BASE TABLE'`
        : `SELECT count(*) AS c FROM information_schema.tables WHERE table_schema=DATABASE() AND table_name IN ('${fixture.parentTable}','${fixture.table}') AND table_type='BASE TABLE'`;
    const viewSql =
      fixture.dialect === 'postgresql'
        ? `SELECT count(*)::int AS c FROM information_schema.views WHERE table_schema='public' AND table_name='${fixture.view}'`
        : `SELECT count(*) AS c FROM information_schema.views WHERE table_schema=DATABASE() AND table_name='${fixture.view}'`;
    const fkSql =
      fixture.dialect === 'postgresql'
        ? `SELECT count(*)::int AS c FROM information_schema.table_constraints WHERE constraint_schema='public' AND table_name='${fixture.table}' AND constraint_name='${fixture.foreignKey}' AND constraint_type='FOREIGN KEY'`
        : `SELECT count(*) AS c FROM information_schema.table_constraints WHERE constraint_schema=DATABASE() AND table_name='${fixture.table}' AND constraint_name='${fixture.foreignKey}' AND constraint_type='FOREIGN KEY'`;
    expect(await countOnTarget(fixture, baseTablesSql)).toBe(2);
    expect(await countOnTarget(fixture, viewSql)).toBe(1);
    expect(await countOnTarget(fixture, fkSql)).toBe(1);
    if (fixture.dialect === 'mysql') {
      await executeOn(fixture.targetId, `INSERT INTO ${fixture.parentTable} (id) VALUES (901)`);
      await executeOn(
        fixture.targetId,
        `INSERT INTO ${fixture.table} (id, parent_id, state) VALUES (1901, 901, 'done')`,
      );
      const targetViewRows = parseQueryRows(
        await executeOn(
          fixture.targetId,
          `SELECT id, state, parent_id FROM ${fixture.view} WHERE id=1901`,
        ),
      );
      expect(targetViewRows).toHaveLength(1);
      expect(targetViewRows[0]?.map(String)).toEqual(['1901', 'done', '901']);
    }
    if (fixture.type) {
      expect(
        await countOnTarget(
          fixture,
          `SELECT count(*)::int AS c FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public' AND t.typname='${fixture.type}' AND t.typtype='e'`,
        ),
      ).toBe(1);
    }
    if (fixture.sequence) {
      expect(
        await countOnTarget(
          fixture,
          `SELECT CASE WHEN pg_get_serial_sequence('public.${fixture.table}','id')='public.${fixture.sequence}' THEN 1 ELSE 0 END AS c`,
        ),
      ).toBe(1);
    }
    if (fixture.routine && fixture.trigger) {
      expect(
        await countOnTarget(
          fixture,
          `SELECT count(*)::int AS c FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.proname='${fixture.routine}'`,
        ),
      ).toBe(1);
      expect(
        await countOnTarget(
          fixture,
          `SELECT count(*)::int AS c FROM pg_trigger tr JOIN pg_class c ON c.oid=tr.tgrelid WHERE NOT tr.tgisinternal AND tr.tgname='${fixture.trigger}' AND c.relname='${fixture.table}'`,
        ),
      ).toBe(1);
    }
  } finally {
    await cleanupFixture(fixture, mainWindow);
  }
}

async function runInvalidDependencyJourney(fixture: Fixture, mainWindow: string) {
  await addFixtureConnections(fixture);
  try {
    await setupFixture(fixture);
    await openObjectPicker(fixture);
    await assertSourceViewCatalog(fixture);
    // Deliberately leave both source tables unselected while selecting their view.
    await setSchemaDiffTables('');
    await selectSourceObject('view', fixture.view);
    // With only an object selected, the wizard intentionally skips table comparison.
    await clickSchemaDiffGeneratePlan();

    const requirements = await $('[data-testid="schema-diff-plan-requirements"]');
    await requirements.waitForDisplayed({ timeout: 15000 });
    const requirementText = (await requirements.getText()).toLowerCase();
    expect(requirementText).toMatch(/depend|select|table|object/);
    if (fixture.dialect === 'mysql') {
      expect(requirementText).toContain(`${MYSQL_SYNC_DB}.${fixture.table}`.toLowerCase());
    }
    expect(await readPlanStatements()).toEqual([]);
    await advanceSchemaDiffToReview();
    const deploy = await $('[data-testid="schema-diff-deploy"]');
    await deploy.waitForDisplayed({ timeout: 8000 });
    expect(await deploy.isEnabled()).toBe(false);
    expect(await fixtureCount(fixture.targetId, fixture)).toBe(0);
  } finally {
    await cleanupFixture(fixture, mainWindow);
  }
}

async function runMysqlNonDefaultViewMetadataJourney(fixture: Fixture, mainWindow: string) {
  await addFixtureConnections(fixture);
  try {
    await setupFixture(fixture);
    const source = await invokeBackend<string>('connect', { connectionId: fixture.sourceId });
    try {
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE OR REPLACE VIEW ${fixture.view} AS SELECT id, state, parent_id FROM ${fixture.table} WITH CASCADED CHECK OPTION`,
        });
      });
      const rawViewMetadata = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: source,
        sql: `SELECT TABLE_SCHEMA, TABLE_NAME, VIEW_DEFINITION, CHECK_OPTION FROM information_schema.VIEWS WHERE TABLE_SCHEMA = '${MYSQL_SOURCE_DB}' AND TABLE_NAME = '${fixture.view}'`,
      });
      const rawViewRow = parseQueryRows(rawViewMetadata)[0];
      const rawCheckOption = rawViewRow?.[3];
      const checkOption = Array.isArray(rawCheckOption)
        ? String.fromCharCode(...rawCheckOption.map((byte) => Number(byte)))
        : String(rawCheckOption);
      const showCreateResult = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: source,
        sql: `SHOW CREATE VIEW \`${MYSQL_SOURCE_DB}\`.\`${fixture.view}\``,
      });
      const showCreateRow = parseQueryRows(showCreateResult)[0];
      const showCreateColumns = showCreateResult.results?.[0]?.columns?.map((column) =>
        typeof column === 'string' ? column : column.name,
      );
      const showCreateIndex = showCreateColumns?.findIndex(
        (column) => column.toLowerCase() === 'create view',
      );
      const showCreateDdl =
        showCreateRow?.[showCreateIndex != null && showCreateIndex >= 0 ? showCreateIndex : 1];
      const showCreateText = showCreateDdl == null ? '' : String(showCreateDdl);
      const rawCatalogBody = rawViewRow?.[2];
      const catalogBody = Array.isArray(rawCatalogBody)
        ? String.fromCharCode(...rawCatalogBody.map((byte) => Number(byte)))
        : rawCatalogBody == null
          ? ''
          : String(rawCatalogBody);
      const showCreateQueryBody = showCreateText
        .replace(/^.*?\sAS\s/i, '')
        .replace(/\sWITH\s+CASCADED\s+CHECK\s+OPTION\s*;?\s*$/i, '')
        .trim();
      const sanitizedShowCreateText = showCreateText.replace(
        /DEFINER=`[^`]*`@`[^`]*`/i,
        'DEFINER=`[redacted]`@`[redacted]`',
      );
      console.log(
        `[SD-UNIFIED] mysql check-option raw metadata schema=${String(rawViewRow?.[0])} name=${String(rawViewRow?.[1])} row_count=${parseQueryRows(rawViewMetadata).length} definition_length=${rawViewRow?.[2] == null ? 0 : String(rawViewRow[2]).length} check_option=${checkOption} show_create_rows=${parseQueryRows(showCreateResult).length} show_create_ddl_length=${showCreateText.length} show_create_has_cascaded_suffix=${/WITH\s+CASCADED\s+CHECK\s+OPTION/i.test(showCreateText)}`,
      );
      console.log(
        `[SD-UNIFIED] mysql check-option exact bodies catalog=${catalogBody.replace(/\s+/g, ' ').trim()} show_create=${showCreateQueryBody.replace(/\s+/g, ' ').trim()}`,
      );
      console.log(
        `[SD-UNIFIED] mysql check-option full SHOW CREATE (definer redacted)=${sanitizedShowCreateText.replace(/\s+/g, ' ').trim()}`,
      );
      let viewDdlError = '';
      try {
        const viewDdl = await invokeBackend<{
          data: { ddl: string; viewMetadata: { checkOption: string } };
        }>('execute_driver_command', {
          request: {
            dbSessionId: source,
            command: 'get_object_ddl',
            input: { kind: 'view', name: fixture.view, schema: MYSQL_SOURCE_DB },
          },
        });
        expect(viewDdl.data.ddl.trim().length).toBeGreaterThan(0);
        expect(viewDdl.data.viewMetadata.checkOption).toBe('CASCADED');
      } catch (error) {
        viewDdlError = sanitizeFixtureDiagnostic(
          error instanceof Error ? error.message : String(error),
        );
        if (process.env.SD_UNIFIED_CAPTURE_CHECK_OPTION_UI !== '1') throw error;
      }
      if (viewDdlError) {
        console.log(`[SD-UNIFIED] mysql check-option direct DDL error=${viewDdlError}`);
      }
    } finally {
      await disconnectBackend(source);
    }

    await openObjectPicker(fixture);
    await setSchemaDiffTables(`${fixture.parentTable}, ${fixture.table}`);
    await selectSourceObject('view', fixture.view);
    await clickSchemaDiffCompare();
    try {
      await clickSchemaDiffGeneratePlan();
    } catch (error) {
      const diagnostics = await browser.execute(() => ({
        activeStep: Array.from(
          document.querySelectorAll<HTMLElement>('[data-testid^="schema-diff-step-"]'),
        ).find((element) => element.className.includes('font-semibold'))?.dataset.testid,
        errors: Array.from(document.querySelectorAll('.error-message'))
          .map((element) => element.textContent?.trim() ?? '')
          .filter(Boolean),
        plan: document.querySelector('[data-testid="schema-diff-plan-panel"]')?.textContent ?? '',
      }));
      console.log(`[SD-UNIFIED] mysql check-option plan diagnostic=${JSON.stringify(diagnostics)}`);
      throw error;
    }

    const requirements = await $('[data-testid="schema-diff-plan-requirements"]');
    await requirements.waitForDisplayed({ timeout: 15000 });
    const requirementText = (await requirements.getText()).toLowerCase();
    expect(requirementText).toContain('non-default creation semantics');
    expect(await readPlanStatements()).toEqual([]);
    await advanceSchemaDiffToReview();
    const deploy = await $('[data-testid="schema-diff-deploy"]');
    await deploy.waitForDisplayed({ timeout: 8000 });
    expect(await deploy.isEnabled()).toBe(false);
    expect(await fixtureCount(fixture.targetId, fixture)).toBe(0);
  } finally {
    await cleanupFixture(fixture, mainWindow);
  }
}

describe('Schema Diff unified reviewed planner (SD-UNIFIED)', function () {
  this.timeout(180000);
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  for (const dialect of ['postgresql', 'mysql'] as const) {
    it(`SD-UNIFIED-${dialect}-create: deploys and reads back the selected dependency chain`, async () => {
      await runPositiveJourney(createFixture(dialect), mainWindow);
    });
    it(`SD-UNIFIED-${dialect}-blocked: leaves the target unchanged when a view dependency is unselected`, async () => {
      await runInvalidDependencyJourney(createFixture(dialect), mainWindow);
    });
  }

  it('SD-UNIFIED-mysql-catalog: lists functions, procedures, triggers, and views with exact schema identities', async () => {
    await runMysqlCatalogSmokeJourney(createMysqlCatalogFixture());
  });

  it('SD-UNIFIED-mysql-view-metadata-blocked: blocks WITH CASCADED CHECK OPTION before target writes', async () => {
    await runMysqlNonDefaultViewMetadataJourney(createFixture('mysql'), mainWindow);
  });
});
