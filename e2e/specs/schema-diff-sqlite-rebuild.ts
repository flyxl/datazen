/** SQLite-only Schema Diff journey using two disposable file databases. */
import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import {
  advanceSchemaDiffToReview,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  closeExtraWindows,
  deploySchemaDiffPlan,
  disconnectBackend,
  invokeBackend,
  openSchemaDiffWindow,
  selectSchemaDiffEndpoints,
  setSchemaDiffTables,
} from '../helpers.js';

type SqliteFile = {
  exec(sql: string): void;
  prepare(sql: string): { get(): Record<string, unknown> | undefined };
  close(): void;
};
const Sqlite = createRequire(import.meta.url)('better-sqlite3') as new (file: string) => SqliteFile;

describe('[tester] SQLite Schema Diff rebuild', () => {
  const stamp = Date.now().toString(36);
  const directory = fs.mkdtempSync(
    path.join(process.cwd(), 'e2e', '.app-data', 'datazen-sqlite-rebuild-wdio-'),
  );
  const sourceFile = path.join(directory, 'source.sqlite');
  const targetFile = path.join(directory, 'target.sqlite');
  const sourceId = `sqlite_rebuild_source_${stamp}`;
  const targetId = `sqlite_rebuild_target_${stamp}`;
  const sourceName = `SQLite Rebuild Source ${stamp}`;
  const targetName = `SQLite Rebuild Target ${stamp}`;
  const table = `rebuild_${stamp}`;
  const staleTable = `stale_rebuild_${stamp}`;
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    const source = new Sqlite(sourceFile);
    const target = new Sqlite(targetFile);
    try {
      source.exec(`CREATE TABLE ${table} (id INTEGER PRIMARY KEY, value BLOB NOT NULL)`);
      target.exec(`CREATE TABLE ${table} (id INTEGER PRIMARY KEY, value TEXT NOT NULL)`);
      target.exec(`INSERT INTO ${table} (id, value) VALUES (7, 'kept')`);
      source.exec(`CREATE TABLE ${staleTable} (id INTEGER PRIMARY KEY, value BLOB NOT NULL)`);
      target.exec(`CREATE TABLE ${staleTable} (id INTEGER PRIMARY KEY, value TEXT NOT NULL)`);
      target.exec(`INSERT INTO ${staleTable} (id, value) VALUES (9, 'unchanged')`);
    } finally {
      source.close();
      target.close();
    }
    for (const [id, name, file] of [
      [sourceId, sourceName, sourceFile],
      [targetId, targetName, targetFile],
    ]) {
      await invokeBackend('save_connection', {
        config: {
          id,
          name,
          databaseType: 'sqlite',
          host: '',
          port: 0,
          username: '',
          password: '',
          database: file,
          group: 'E2E',
          sslMode: 'disable',
        },
      });
    }
    for (const connectionId of [sourceId, targetId]) {
      const session = await invokeBackend<string>('connect', { connectionId });
      await disconnectBackend(session);
    }
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    for (const connectionId of [sourceId, targetId]) {
      await invokeBackend('delete_connection', { id: connectionId }).catch(() => undefined);
    }
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it('rebuilds an existing SQLite table and preserves rows', async () => {
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(sourceName, targetName);
    await setSchemaDiffTables(table);
    await clickSchemaDiffCompare();
    await clickSchemaDiffGeneratePlan();
    const plan = await $('[data-testid="schema-diff-plan-panel"]');
    expect((await plan.getText()).toLowerCase()).toContain(table);
    await advanceSchemaDiffToReview();
    const confirmation = await $(
      '[data-testid="schema-diff-deploy-panel"] input[placeholder="DEPLOY"]',
    );
    if (await confirmation.isExisting()) await confirmation.setValue('DEPLOY');
    await deploySchemaDiffPlan();

    const target = new Sqlite(targetFile);
    try {
      const row = target.prepare(`SELECT id, value FROM ${table}`).get();
      expect(row?.id).toBe(7);
      expect(row?.value).toBe('kept');
      const schema = target
        .prepare(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '${table}'`)
        .get();
      expect(String(schema?.sql)).toMatch(/BLOB/i);
    } finally {
      target.close();
    }
  });

  it('rejects a target schema changed after review before writing', async () => {
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(sourceName, targetName);
    await setSchemaDiffTables(staleTable);
    await clickSchemaDiffCompare();
    await clickSchemaDiffGeneratePlan();
    await advanceSchemaDiffToReview();
    const confirmation = await $(
      '[data-testid="schema-diff-deploy-panel"] input[placeholder="DEPLOY"]',
    );
    if (await confirmation.isExisting()) await confirmation.setValue('DEPLOY');

    const target = new Sqlite(targetFile);
    try {
      target.exec(`ALTER TABLE ${staleTable} ADD COLUMN drift TEXT`);
    } finally {
      target.close();
    }

    await deploySchemaDiffPlan({ assertSuccess: false });
    expect(await $('[data-testid="schema-diff-deploy-status"]').getText()).toContain('failed');
    expect(await $('[data-testid="schema-diff-deploy-count"]').getText()).toMatch(/^0\//);
    expect(await $('[data-testid="schema-diff-deploy-errors"]').getText()).not.toBe('');

    const unchangedTarget = new Sqlite(targetFile);
    try {
      const row = unchangedTarget.prepare(`SELECT id, value, drift FROM ${staleTable}`).get();
      expect(row).toEqual({ id: 9, value: 'unchanged', drift: null });
      const schema = unchangedTarget
        .prepare(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '${staleTable}'`)
        .get();
      expect(String(schema?.sql)).toMatch(/value\s+TEXT/i);
      expect(String(schema?.sql)).toMatch(/drift\s+TEXT/i);
    } finally {
      unchangedTarget.close();
    }
  });
});
