/** Shared fixture and UI helpers for the split R3 Data Transfer WDIO journeys. */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  clickTransferNext,
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  executeQuery,
  invokeBackend,
  openDataTransferWindow,
  parseQueryRows,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';

export { disconnectBackend, parseQueryRows, queryScalar };

export type DbKind = 'postgresql' | 'mysql';
export type Fixture = {
  id: string;
  name: string;
  database: string;
  kind: DbKind;
  schema?: string;
};

const stamp = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
export const prefix = `dzm_${stamp}`;
const databases = {
  pgSource: `${prefix}_pgs`,
  pgTarget: `${prefix}_pgt`,
  mysqlSource: `${prefix}_mys`,
  mysqlTarget: `${prefix}_myt`,
};
export const fixtures: Fixture[] = [
  {
    id: `e2e_${prefix}_pgs`,
    name: `E2E ${prefix} PG source`,
    database: databases.pgSource,
    kind: 'postgresql',
    schema: 'public',
  },
  {
    id: `e2e_${prefix}_pgt`,
    name: `E2E ${prefix} PG target`,
    database: databases.pgTarget,
    kind: 'postgresql',
    schema: `${prefix}_target_schema`,
  },
  {
    id: `e2e_${prefix}_mys`,
    name: `E2E ${prefix} MySQL source`,
    database: databases.mysqlSource,
    kind: 'mysql',
  },
  {
    id: `e2e_${prefix}_myt`,
    name: `E2E ${prefix} MySQL target`,
    database: databases.mysqlTarget,
    kind: 'mysql',
  },
];
export const pgTables = { parent: `${prefix}_pg_parent`, child: `${prefix}_pg_child` };
export const pgNumericTables = {
  parent: `${prefix}_pg_numeric_parent`,
  child: `${prefix}_pg_numeric_child`,
};
export const pgCollationTable = `${prefix}_pg_collation`;
export const pgTextIndexTable = `${prefix}_pg_text_index`;
export const pgLossyTypesTable = `${prefix}_pg_lossy_types`;
export const pgArrayTable = `${prefix}_pg_array`;
const pgEnumType = `${prefix}_pg_mood`;
export const pgEnumTable = `${prefix}_pg_enum`;
export const renamedTables = {
  parent: `${prefix}_mapped_parent`,
  child: `${prefix}_mapped_child`,
  omittedChild: `${prefix}_omitted_child`,
};
export const mysqlTables = { parent: `${prefix}_my_parent`, child: `${prefix}_my_child` };
const sharedSequence = `${prefix}_shared_seq`;
export const sharedTables = { first: `${prefix}_shared_a`, second: `${prefix}_shared_b` };
const adminIds: Partial<Record<DbKind, string>> = {};
export const sessions = new Set<string>();
const createdDatabases = new Set<string>();

function connectionConfig(fixture: Fixture, admin = false) {
  return {
    id: fixture.id,
    name: fixture.name,
    databaseType: fixture.kind,
    host:
      fixture.kind === 'postgresql'
        ? process.env.E2E_PG_HOST || '127.0.0.1'
        : process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port:
      fixture.kind === 'postgresql'
        ? Number(process.env.E2E_PG_PORT) || 5432
        : Number(process.env.E2E_MYSQL_PORT) || 3306,
    username:
      fixture.kind === 'postgresql'
        ? process.env.E2E_PG_USER || 'flyxl'
        : process.env.E2E_MYSQL_USER || 'root',
    password:
      fixture.kind === 'postgresql'
        ? process.env.E2E_PG_PASSWORD || ''
        : process.env.E2E_MYSQL_PASSWORD || '',
    database: admin ? (fixture.kind === 'postgresql' ? 'postgres' : 'mysql') : fixture.database,
    sslMode: 'disable',
    ...(fixture.schema && !admin ? { schema: fixture.schema } : {}),
  };
}

async function saveFixture(fixture: Fixture, admin = false) {
  await invokeBackend('save_connection', { config: connectionConfig(fixture, admin) });
}

export async function sql(session: string, statement: string): Promise<QueryResultPayload> {
  return executeQuery(session, statement);
}

export async function connectFixture(fixture: Fixture): Promise<string> {
  const session = await connectBackend(fixture.id);
  sessions.add(session);
  return session;
}

async function createFixtureDatabase(kind: DbKind, database: string) {
  const fixture: Fixture = {
    id: `e2e_${prefix}_${kind}_admin`,
    name: `E2E ${prefix} ${kind} admin`,
    database,
    kind,
  };
  if (!adminIds[kind]) {
    adminIds[kind] = fixture.id;
    await saveFixture(fixture, true);
  }
  const adminSession = await connectBackend(fixture.id);
  sessions.add(adminSession);
  try {
    await withSafeModeOff(async () => {
      await sql(
        adminSession,
        kind === 'postgresql'
          ? `CREATE DATABASE ${database}`
          : `CREATE DATABASE \`${database}\` CHARACTER SET utf8mb4`,
      );
    });
    createdDatabases.add(`${kind}:${database}`);
  } finally {
    await disconnectBackend(adminSession);
    sessions.delete(adminSession);
  }
}

async function setUpDatabasesAndTables() {
  await createFixtureDatabase('postgresql', databases.pgSource);
  await createFixtureDatabase('postgresql', databases.pgTarget);
  await createFixtureDatabase('mysql', databases.mysqlSource);
  await createFixtureDatabase('mysql', databases.mysqlTarget);
  for (const fixture of fixtures) await saveFixture(fixture);

  const pgTarget = await connectFixture(fixtures[1]);
  try {
    await withSafeModeOff(async () => {
      await sql(pgTarget, `CREATE SCHEMA ${fixtures[1].schema}`);
    });
  } finally {
    await disconnectBackend(pgTarget);
    sessions.delete(pgTarget);
  }

  const pg = await connectFixture(fixtures[0]);
  const mysql = await connectFixture(fixtures[2]);
  try {
    await withSafeModeOff(async () => {
      await sql(
        pg,
        `CREATE TABLE ${pgTables.parent} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          code VARCHAR(40) NOT NULL UNIQUE
        )`,
      );
      await sql(
        pg,
        `CREATE TABLE ${pgTables.child} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          parent_id BIGINT NOT NULL CONSTRAINT fk_${prefix}_pg_parent
            REFERENCES ${pgTables.parent}(id),
          label VARCHAR(120) NOT NULL
        )`,
      );
      await sql(pg, `CREATE INDEX ix_${prefix}_pg_label ON ${pgTables.child}(label)`);
      await sql(pg, `INSERT INTO ${pgTables.parent} (id, code) VALUES (11, 'pg-a'), (12, 'pg-b')`);
      await sql(
        pg,
        `INSERT INTO ${pgTables.child} (id, parent_id, label) VALUES (21, 11, 'one'), (22, 12, 'two')`,
      );
      await sql(
        pg,
        `CREATE TABLE ${pgNumericTables.parent} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          code INTEGER NOT NULL UNIQUE
        )`,
      );
      await sql(
        pg,
        `CREATE TABLE ${pgNumericTables.child} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          parent_id BIGINT NOT NULL CONSTRAINT fk_${prefix}_pg_numeric_parent
            REFERENCES ${pgNumericTables.parent}(id),
          quantity INTEGER NOT NULL
        )`,
      );
      await sql(
        pg,
        `CREATE INDEX ix_${prefix}_pg_numeric_quantity ON ${pgNumericTables.child}(quantity)`,
      );
      await sql(pg, `INSERT INTO ${pgNumericTables.parent} (id, code) VALUES (11, 101), (12, 102)`);
      await sql(
        pg,
        `INSERT INTO ${pgNumericTables.child} (id, parent_id, quantity) VALUES (21, 11, 5), (22, 12, 8)`,
      );
      await sql(
        pg,
        `CREATE TABLE ${pgCollationTable} (
          id INTEGER PRIMARY KEY,
          label VARCHAR(32) NOT NULL
        )`,
      );
      await sql(pg, `INSERT INTO ${pgCollationTable} (id, label) VALUES (1, 'sorting')`);
      await sql(
        pg,
        `CREATE TABLE ${pgTextIndexTable} (id BIGINT PRIMARY KEY, label TEXT NOT NULL)`,
      );
      await sql(pg, `CREATE INDEX ix_${prefix}_pg_text_label ON ${pgTextIndexTable}(label)`);
      await sql(pg, `INSERT INTO ${pgTextIndexTable} (id, label) VALUES (91, 'indexed text')`);
      await sql(
        pg,
        `CREATE TABLE ${pgLossyTypesTable} (
          unbounded_varchar character varying NOT NULL,
          unbounded_text text NOT NULL,
          unbounded_numeric numeric NOT NULL,
          wall_time time with time zone NOT NULL,
          instant timestamp with time zone NOT NULL
        )`,
      );
      await sql(
        pg,
        `INSERT INTO ${pgLossyTypesTable}
          (unbounded_varchar, unbounded_text, unbounded_numeric, wall_time, instant)
         VALUES (repeat('v', 256), repeat('t', 65536),
           '100000000000000000000000000000000000', '04:05:06-08',
           '2026-09-26 04:05:06+08')`,
      );
      await sql(
        pg,
        `CREATE TABLE ${pgArrayTable} (id INTEGER PRIMARY KEY, array_values INTEGER[] NOT NULL)`,
      );
      await sql(pg, `INSERT INTO ${pgArrayTable} (id, array_values) VALUES (1, ARRAY[1, 2])`);
      await sql(pg, `CREATE TYPE ${pgEnumType} AS ENUM ('ok', 'urgent')`);
      await sql(pg, `CREATE TABLE ${pgEnumTable} (mood ${pgEnumType} NOT NULL)`);
      await sql(pg, `INSERT INTO ${pgEnumTable} (mood) VALUES ('urgent')`);

      await sql(pg, `CREATE SEQUENCE ${sharedSequence} START WITH 50 INCREMENT BY 7`);
      await sql(
        pg,
        `CREATE TABLE ${sharedTables.first} (
          id BIGINT PRIMARY KEY DEFAULT nextval('${sharedSequence}')
        )`,
      );
      await sql(
        pg,
        `CREATE TABLE ${sharedTables.second} (
          id BIGINT PRIMARY KEY DEFAULT nextval('${sharedSequence}')
        )`,
      );
      await sql(pg, `INSERT INTO ${sharedTables.first} DEFAULT VALUES`);
      await sql(pg, `INSERT INTO ${sharedTables.second} DEFAULT VALUES`);

      await sql(
        mysql,
        `CREATE TABLE ${mysqlTables.parent} (
          id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
          code VARCHAR(40) NOT NULL UNIQUE
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4`,
      );
      await sql(
        mysql,
        `CREATE TABLE ${mysqlTables.child} (
          id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
          parent_id BIGINT NOT NULL,
          label VARCHAR(120) NOT NULL,
          CONSTRAINT fk_${prefix}_my_parent FOREIGN KEY (parent_id)
            REFERENCES ${mysqlTables.parent}(id),
          INDEX ix_${prefix}_my_label (label)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4`,
      );
      await sql(
        mysql,
        `INSERT INTO ${mysqlTables.parent} (id, code) VALUES (31, 'my-a'), (32, 'my-b')`,
      );
      await sql(
        mysql,
        `INSERT INTO ${mysqlTables.child} (id, parent_id, label) VALUES (41, 31, 'three'), (42, 32, 'four')`,
      );
    });
  } finally {
    await disconnectBackend(pg);
    await disconnectBackend(mysql);
    sessions.delete(pg);
    sessions.delete(mysql);
  }

  const mysqlTarget = await connectFixture(fixtures[3]);
  try {
    await withSafeModeOff(async () => {
      await sql(
        mysqlTarget,
        `CREATE TABLE ${pgArrayTable} (id INTEGER PRIMARY KEY, array_values JSON NULL)`,
      );
    });
  } finally {
    await disconnectBackend(mysqlTarget);
    sessions.delete(mysqlTarget);
  }
}

async function selectOnlyTables(tableNames: string[]) {
  const rows = await $$('[data-testid="data-transfer-table-row"]');
  for (const row of rows) {
    const text = await row.getText();
    const checkbox = await row.$('input[type="checkbox"]');
    const shouldSelect = tableNames.some((name) => text.includes(name));
    if (await checkbox.isSelected().catch(() => false)) {
      if (!shouldSelect) await checkbox.click();
    } else if (shouldSelect) {
      await checkbox.click();
    }
  }
  for (const name of tableNames) {
    let found = false;
    for (const row of rows) {
      if ((await row.getText()).includes(name)) found = true;
    }
    if (!found) throw new Error(`source table should be listed: ${name}`);
  }
}

async function enableCreateNewMappings(
  tableNames: string[],
  options: {
    clearSuggestedTypes?: boolean;
    createNew?: boolean;
    renameTables?: Record<string, string>;
    renameColumns?: Record<string, Record<string, string>>;
  } = {},
) {
  for (const name of tableNames) {
    await browser.waitUntil(
      async () => {
        const items = await $$('[data-testid="data-transfer-mapping-table-item"]');
        for (const candidate of items) {
          if ((await candidate.getText()).includes(name)) return true;
        }
        return false;
      },
      { timeout: 10000, timeoutMsg: `missing mapping row for ${name}` },
    );
    const items = await $$('[data-testid="data-transfer-mapping-table-item"]');
    let clicked = false;
    for (const candidate of items) {
      if ((await candidate.getText()).includes(name)) {
        await candidate.click();
        clicked = true;
        break;
      }
    }
    if (!clicked) throw new Error(`missing mapping row for ${name}`);
    if (options.createNew ?? true) {
      const toggle = await $('[data-testid="data-transfer-create-new-toggle"]');
      await toggle.waitForDisplayed({ timeout: 8000 });
      if (!(await toggle.isSelected())) await toggle.click();
      const target = await $('[data-testid="data-transfer-target-table-input"]');
      // Turning "create new table" on deliberately clears the prefilled source
      // name: the reviewer has to name the table they are about to create, and
      // the Next gate stays closed until they do. The fixture therefore always
      // types a name — the source name unless the case renames the table.
      const targetName = options.renameTables?.[name] ?? name;
      await target.setValue(targetName);
      await target.click();
      await browser.keys(['Tab']);
      for (const [sourceColumn, targetColumn] of Object.entries(
        options.renameColumns?.[name] ?? {},
      )) {
        const column = await $(`[data-testid="data-transfer-target-col-${sourceColumn}"]`);
        await column.waitForDisplayed({ timeout: 8000 });
        await column.setValue(targetColumn);
      }
    }
    if (options.clearSuggestedTypes) {
      const suggestedTypes = await $$('input[data-testid^="data-transfer-target-type-"]');
      for (const input of suggestedTypes) {
        if (await input.isDisplayed()) await input.clearValue();
      }
    }
  }
}

async function advanceToPreviewOutcome() {
  for (let step = 0; step < 8; step++) {
    if (await $('[data-testid="data-transfer-preview"]').isExisting()) return;
    if (await $('[data-testid="data-transfer-preview-error"]').isExisting()) return;
    const next = await $('[data-testid="data-transfer-next"]');
    if (!(await next.isExisting())) break;
    await clickTransferNext();
  }
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-transfer-preview"]').isExisting()) ||
      (await $('[data-testid="data-transfer-preview-error"]').isExisting()),
    { timeout: 20000, timeoutMsg: 'preview neither succeeded nor failed visibly' },
  );
}

export async function prepareTransfer(
  source: Fixture,
  target: Fixture,
  selected: string[],
  mode: 'data' | 'both' | 'structure',
  options: {
    clearSuggestedTypes?: boolean;
    createNew?: boolean;
    renameTables?: Record<string, string>;
    renameColumns?: Record<string, Record<string, string>>;
  } = {},
) {
  await closeExtraWindows(await browser.getWindowHandle());
  await openDataTransferWindow();
  await selectDzOptionInWrap('data-transfer-source', source.name);
  await selectDzOptionInWrap('data-transfer-target', target.name);
  await clickTransferNext();
  const modeId =
    mode === 'both'
      ? 'data-transfer-mode-both'
      : mode === 'data'
        ? 'data-transfer-mode-data'
        : 'data-transfer-mode-structure';
  const modeOption = await $(`[data-testid="${modeId}"]`);
  await modeOption.click();
  if (!(await modeOption.isSelected())) throw new Error(`transfer mode was not selected: ${mode}`);
  await clickTransferNext();
  await browser.waitUntil(
    async () => {
      const rows = await $$('[data-testid="data-transfer-table-row"]');
      return (await rows.length) > 0;
    },
    { timeout: 15000, timeoutMsg: 'source tables were not inspected' },
  );
  await selectOnlyTables(selected);
  await clickTransferNext();
  await $('[data-testid="data-transfer-mapping-step"]').waitForDisplayed({ timeout: 15000 });
  await enableCreateNewMappings(selected, options);
  await advanceToPreviewOutcome();
}

export async function executeIfPreviewExists() {
  if (await $('[data-testid="data-transfer-preview-error"]').isExisting()) {
    return {
      rejected: true,
      text: await $('[data-testid="data-transfer-preview-error"]').getText(),
    };
  }
  const preview = await $('[data-testid="data-transfer-preview"]');
  await preview.waitForDisplayed({ timeout: 10000 });
  const pageText = await preview.getText();
  const execute = await $('[data-testid="data-transfer-execute"]');
  if (!(await execute.isEnabled())) return { rejected: true, text: pageText };
  await execute.click();
  const result = await $('[data-testid="data-transfer-result"]');
  await result.waitForDisplayed({ timeout: 30000 });
  // The panel exists while the job is still running (`data-completed="false"`),
  // so reading outcomes on sight reported "no outcomes" as a rejection. Wait for
  // the verdict to settle before deciding anything.
  await browser.waitUntil(
    async () =>
      (await result.getAttribute('data-completed')) === 'true' ||
      (await result.getAttribute('data-verdict-severity')) !== 'active',
    { timeout: 60000, timeoutMsg: 'data transfer verdict never settled' },
  );
  const resultText = await result.getText();
  const tableResults = await result.$$('[data-testid^="data-transfer-table-result-"]');
  const outcomes: string[] = [];
  for (const tableResult of tableResults) {
    const outcome = await tableResult.getAttribute('data-outcome');
    if (outcome) outcomes.push(outcome);
  }
  const errorBlocks = await result.$$('.error-message');
  const rejected =
    outcomes.some((outcome) => outcome !== 'committed') || (await errorBlocks.length) > 0;
  return { rejected, text: resultText };
}

export async function inspectStructurePreview(source: Fixture, target: Fixture, table: string) {
  await prepareTransfer(source, target, [table], 'structure');
  const error = await $('[data-testid="data-transfer-preview-error"]');
  if (await error.isExisting()) return { blocked: true, text: await error.getText() };
  const preview = await $('[data-testid="data-transfer-preview"]');
  await preview.waitForDisplayed({ timeout: 10000 });
  const text = await preview.getText();
  const warningNodes = await preview.$$('p.text-xs.text-fg-muted');
  const reasonNodes = await preview.$$('p.text-warning');
  const noticeTexts: string[] = [];
  for (const node of warningNodes) noticeTexts.push(await node.getText());
  for (const node of reasonNodes) noticeTexts.push(await node.getText());
  const notices = noticeTexts.join('\n');
  const safelyExplained =
    /cannot preserve|unsupported|unbounded|narrow|truncate|precision|time.?zone|timezone/i.test(
      notices,
    );
  return { blocked: safelyExplained, text: `${notices}\n${text}` };
}

export async function assertPgToMysqlObjectsAndRows() {
  const target = await connectFixture(fixtures[3]);
  try {
    const parentRows = await sql(target, `SELECT COUNT(*) AS c FROM ${pgNumericTables.parent}`);
    const childRows = await sql(target, `SELECT COUNT(*) AS c FROM ${pgNumericTables.child}`);
    expect(queryScalar(parentRows, 'c')).toBe(2);
    expect(queryScalar(childRows, 'c')).toBe(2);
    expect(
      parseQueryRows(
        await sql(target, `SELECT id, code FROM ${pgNumericTables.parent} ORDER BY id`),
      ),
    ).toEqual([
      [11, 101],
      [12, 102],
    ]);
    expect(
      parseQueryRows(
        await sql(
          target,
          `SELECT id, parent_id, quantity FROM ${pgNumericTables.child} ORDER BY id`,
        ),
      ),
    ).toEqual([
      [21, 11, 5],
      [22, 12, 8],
    ]);
    const primaryKeys = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.table_constraints
       WHERE table_schema = DATABASE() AND constraint_type = 'PRIMARY KEY'
       AND table_name IN ('${pgNumericTables.parent}', '${pgNumericTables.child}')`,
    );
    const indexes = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.statistics
       WHERE table_schema = DATABASE() AND table_name = '${pgNumericTables.child}'
       AND index_name = 'ix_${prefix}_pg_numeric_quantity' AND non_unique = 1`,
    );
    const fks = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.key_column_usage
       WHERE table_schema = DATABASE() AND table_name = '${pgNumericTables.child}'
       AND referenced_table_name = '${pgNumericTables.parent}'`,
    );
    const identities = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.columns
       WHERE table_schema = DATABASE()
       AND table_name IN ('${pgNumericTables.parent}', '${pgNumericTables.child}')
       AND extra LIKE '%auto_increment%'`,
    );
    expect(queryScalar(primaryKeys, 'c')).toBe(2);
    expect(queryScalar(indexes, 'c')).toBe(1);
    expect(queryScalar(fks, 'c')).toBe(1);
    expect(queryScalar(identities, 'c')).toBe(2);
    await sql(target, `INSERT INTO ${pgNumericTables.child} (parent_id, quantity) VALUES (11, 13)`);
    const generatedId = await sql(target, `SELECT MAX(id) AS c FROM ${pgNumericTables.child}`);
    expect(queryScalar(generatedId, 'c')).toBeGreaterThan(22);
  } finally {
    await disconnectBackend(target);
    sessions.delete(target);
  }
}

export async function assertMysqlToPgObjectsRowsAndSequence() {
  const target = await connectFixture(fixtures[1]);
  try {
    const parentRows = await sql(target, `SELECT COUNT(*) AS c FROM ${mysqlTables.parent}`);
    const childRows = await sql(target, `SELECT COUNT(*) AS c FROM ${mysqlTables.child}`);
    expect(queryScalar(parentRows, 'c')).toBe(2);
    expect(queryScalar(childRows, 'c')).toBe(2);
    const primaryKeys = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.table_constraints
       WHERE table_schema = current_schema() AND constraint_type = 'PRIMARY KEY'
       AND table_name IN ('${mysqlTables.parent}', '${mysqlTables.child}')`,
    );
    const indexes = await sql(
      target,
      `SELECT COUNT(*) AS c FROM pg_indexes WHERE schemaname = current_schema()
       AND tablename = '${mysqlTables.child}' AND indexname = 'ix_${prefix}_my_label'`,
    );
    const fks = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.key_column_usage
       WHERE table_schema = current_schema() AND table_name = '${mysqlTables.child}'
       AND referenced_table_name = '${mysqlTables.parent}'`,
    );
    const identities = await sql(
      target,
      `SELECT COUNT(*) AS c FROM information_schema.columns
       WHERE table_schema = current_schema() AND table_name IN ('${mysqlTables.parent}', '${mysqlTables.child}')
       AND is_identity = 'YES'`,
    );
    expect(queryScalar(primaryKeys, 'c')).toBe(2);
    expect(queryScalar(indexes, 'c')).toBe(1);
    expect(queryScalar(fks, 'c')).toBe(1);
    expect(queryScalar(identities, 'c')).toBe(2);
    const next = await sql(
      target,
      `INSERT INTO ${mysqlTables.child} (parent_id, label) VALUES (31, 'later') RETURNING id`,
    );
    expect(queryScalar(next, 'id')).toBeGreaterThan(42);
  } finally {
    await disconnectBackend(target);
    sessions.delete(target);
  }
}

export async function createPostgresTargetStructureForMysqlFixture() {
  const target = await connectFixture(fixtures[1]);
  try {
    await withSafeModeOff(async () => {
      await sql(
        target,
        `CREATE TABLE ${mysqlTables.parent} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          code VARCHAR(40) NOT NULL UNIQUE
        )`,
      );
      await sql(
        target,
        `CREATE TABLE ${mysqlTables.child} (
          id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
          parent_id BIGINT NOT NULL REFERENCES ${mysqlTables.parent}(id),
          label VARCHAR(120) NOT NULL
        )`,
      );
      await sql(target, `CREATE INDEX ix_${prefix}_my_label ON ${mysqlTables.child}(label)`);
    });
  } finally {
    await disconnectBackend(target);
    sessions.delete(target);
  }
}

export async function startR3Suite(): Promise<string> {
  const mainWindow = await browser.getWindowHandle();
  await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
  await setUpDatabasesAndTables();
  return mainWindow;
}

export async function cleanupR3Suite(mainWindow: string): Promise<void> {
  const cleanupFailures: string[] = [];
  await closeExtraWindows(mainWindow).catch(() => undefined);
  await browser.switchToWindow(mainWindow).catch(() => undefined);
  for (const session of [...sessions]) {
    try {
      await disconnectBackend(session);
    } catch (error) {
      cleanupFailures.push(`disconnect owned session: ${String(error)}`);
    }
  }
  for (const entry of [...createdDatabases].reverse()) {
    const [kind, database] = entry.split(':') as [DbKind, string];
    const id = adminIds[kind];
    if (!id) continue;
    let adminSession: string | undefined;
    try {
      adminSession = await connectBackend(id);
      sessions.add(adminSession);
      const ownedAdminSession = adminSession;
      await withSafeModeOff(async () => {
        if (kind === 'postgresql') {
          await sql(
            ownedAdminSession,
            `SELECT pg_terminate_backend(pid) FROM pg_stat_activity
             WHERE datname = '${database}' AND pid <> pg_backend_pid()`,
          );
        } else {
          const processIds = await sql(
            ownedAdminSession,
            `SELECT ID FROM information_schema.PROCESSLIST
             WHERE DB = '${database}' AND ID <> CONNECTION_ID()`,
          );
          for (const row of parseQueryRows(processIds)) {
            const processId = Number(row[0]);
            if (Number.isSafeInteger(processId) && processId > 0) {
              await sql(ownedAdminSession, `KILL ${processId}`);
            }
          }
        }
        await sql(
          ownedAdminSession,
          kind === 'postgresql' ? `DROP DATABASE ${database}` : `DROP DATABASE \`${database}\``,
        );
      });
      const remaining = await sql(
        ownedAdminSession,
        kind === 'postgresql'
          ? `SELECT COUNT(*) AS c FROM pg_database WHERE datname = '${database}'`
          : `SELECT COUNT(*) AS c FROM information_schema.schemata WHERE schema_name = '${database}'`,
      );
      if (queryScalar(remaining, 'c') !== 0) {
        cleanupFailures.push(`${kind} fixture database ${database} still exists after DROP`);
      } else {
        createdDatabases.delete(entry);
      }
    } catch (error) {
      cleanupFailures.push(`${kind} fixture database ${database}: ${String(error)}`);
    } finally {
      if (adminSession) {
        await disconnectBackend(adminSession);
        sessions.delete(adminSession);
      }
    }
  }
  for (const fixture of fixtures) {
    try {
      await invokeBackend('delete_connection', { id: fixture.id });
    } catch (error) {
      cleanupFailures.push(`delete owned connection ${fixture.id}: ${String(error)}`);
    }
  }
  for (const id of Object.values(adminIds)) {
    if (id) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch (error) {
        cleanupFailures.push(`delete owned admin connection ${id}: ${String(error)}`);
      }
    }
  }
  if (cleanupFailures.length > 0) {
    throw new Error(`fixture cleanup failed: ${cleanupFailures.join('; ')}`);
  }
}
