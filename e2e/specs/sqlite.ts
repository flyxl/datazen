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
    // The navigator's first table fetch races the SQLite connect (it can
    // query with the file path instead of `main` before recovering), so the
    // sidebar content must be polled rather than read once immediately
    // after connecting. The aside element is re-created on re-render —
    // re-fetch it every pass and tolerate transient stale-element errors.
    // The category is opened explicitly: what the case is about is that the
    // sidebar *has* the tables, not whether the tree starts expanded.
    await expandSchemaCategory('tables', 'main').catch(() => undefined);
    const schemaErrors = await browser.execute(() => {
      const store = (
        window as unknown as {
          __schemaStore?: { getState: () => { schemas: Map<string, { error?: string | null }> } };
        }
      ).__schemaStore;
      if (!store) return 'no-store';
      return Array.from(store.getState().schemas.entries()).map(([id, state]) => ({
        id,
        error: state.error ?? null,
      }));
    });
    const tree = await browser.execute(() => {
      const nav =
        document.querySelector('[data-testid="connection-navigator-aside"]') ??
        Array.from(document.querySelectorAll('aside')).find((aside) =>
          aside.querySelector('[data-conn-item]'),
        );
      return Array.from(
        nav?.querySelectorAll<HTMLElement>('[data-testid="schema-tree-node"]') ?? [],
      ).map((node) => ({
        kind: node.dataset.treeNode,
        name: node.dataset.dbName ?? node.dataset.schemaName ?? null,
        label: (node.textContent ?? '').trim().slice(0, 40),
        expanded: node.getAttribute('aria-expanded'),
      }));
    });
    let asideText = '';
    let asideError = '';
    await browser
      .waitUntil(
        async () => {
          try {
            const aside = await connectionNavigatorAside();
            asideText = await aside.getText();
            asideError = '';
            return (
              asideHasSchemaSections(asideText) &&
              asideText.includes('users') &&
              asideText.includes('posts') &&
              asideText.includes('tags')
            );
          } catch (error) {
            asideError = error instanceof Error ? error.message.slice(0, 200) : String(error);
            return false;
          }
        },
        { timeout: 20000, interval: 1000 },
      )
      .catch(() => {
        throw new Error(
          `等待 SQLite 侧边栏显示表列表: asideError=${asideError}; asideText=${JSON.stringify(
            asideText.slice(0, 600),
          )}; tree=${JSON.stringify(tree).slice(0, 900)}; schemaErrors=${JSON.stringify(schemaErrors)}`,
        );
      });
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
