import { expect, browser, $, $$ } from '@wdio/globals';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  closeExtraWindows,
  createAndConnectSQLiteInWorkspace,
  openQueryTab,
  executeSQL,
  clickTableInSidebar,
  switchSubTab,
  asideHasSchemaSections,
  connectionNavigatorAside,
  expandSchemaCategory,
} from '../helpers.js';
import { t } from '../i18n.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONN_NAME = 'E2E-SQLite';
const DB_PATH = path.resolve(__dirname, '../fixtures/test.db');

describe('SQLite', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    const result = await createAndConnectSQLiteInWorkspace(CONN_NAME, DB_PATH);
    mainWindow = result.mainWindow;
    connWindow = result.connWindow;
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('should show tables in sidebar', async () => {
    const aside = await connectionNavigatorAside();
    const text = await aside.getText();
    expect(asideHasSchemaSections(text)).toBe(true);
    expect(text).toContain('users');
    expect(text).toContain('posts');
    expect(text).toContain('tags');
  });

  it('should show views in sidebar', async () => {
    await expandSchemaCategory('views');
    const aside = await connectionNavigatorAside();
    const text = await aside.getText();
    expect(text).toContain(t('schemaTree.views'));
    expect(text).toContain('published_posts');
  });

  it('should display table data', async () => {
    await clickTableInSidebar('users');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('email');
  });

  it('should show table structure', async () => {
    await clickTableInSidebar('users');
    await browser.pause(500);
    await switchSubTab('structure');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('id');
    expect(body).toContain('INTEGER');
  });

  it('should execute SQL query', async () => {
    await openQueryTab();
    await executeSQL('SELECT * FROM users WHERE age > 20');
    const body = await $('body').getText();
    expect(body).toContain('name');
  });

  it('should execute multiple statements', async () => {
    await openQueryTab();
    await executeSQL('SELECT COUNT(*) FROM users;\nSELECT COUNT(*) FROM posts');
    const body = await $('body').getText();
    // Should show "结果 1" and "结果 2" tabs
    expect(body).toContain(`${t('query.result')} 1`);
  });

  it('should stream more than one IPC batch without treating batch size as a row cap', async () => {
    await openQueryTab();
    await executeSQL(
      'WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 600) SELECT x FROM c',
    );
    const body = await $('body').getText();
    expect(body).toContain(`600 ${t('common.rows')}`);
    expect(body).not.toContain(t('query.resultTruncated', { limit: 500 }));
  });

  it('should show indexes', async () => {
    await clickTableInSidebar('users');
    await browser.pause(500);
    await switchSubTab('indexes');
    await browser.pause(1000);
    // SQLite creates autoindex for UNIQUE constraints
    const body = await $('body').getText();
    expect(body).toContain('email');
  });

  it('should display view data', async () => {
    await clickTableInSidebar('published_posts');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('author');
  });
});
