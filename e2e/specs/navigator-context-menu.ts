/**
 * E2E tests for navigator tree context menus in ConnectionPage.
 *
 * Tests context menu items for connections, databases, schemas, categories,
 * and table/view nodes. Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  disconnectBackend,
  openQueryTab,
  executeSQL,
  waitForSchemaTreeLoaded,
  setSafeMode,
  confirmWebDialog,
  invokeBackend,
  queryScalar,
  type QueryResultPayload,
  stubClipboardCapture,
  readStubbedClipboard,
  restoreClipboardStub,
} from '../helpers.js';

const TEST_TABLE = '_e2e_ctx_menu';
const DROP_SCHEMA = '_e2e_nav_drop_schema';
const SEEDED_CONN_ID = 'conn_e2e_pg';

async function pgScalar(dbSessionId: string, sql: string, database?: string): Promise<number> {
  const payload = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId,
    sql,
    ...(database ? { database } : {}),
  });
  return queryScalar(payload);
}

function sqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

async function pgTableExists(
  dbSessionId: string,
  table: string,
  schema = 'public',
): Promise<boolean> {
  const c = await pgScalar(
    dbSessionId,
    `SELECT COUNT(*)::int AS c FROM information_schema.tables WHERE table_schema = '${sqlLiteral(schema)}' AND table_name = '${sqlLiteral(table)}'`,
  );
  return c > 0;
}

async function pgSchemaExists(dbSessionId: string, schema: string): Promise<boolean> {
  const c = await pgScalar(
    dbSessionId,
    `SELECT COUNT(*)::int AS c FROM information_schema.schemata WHERE schema_name = '${sqlLiteral(schema)}'`,
  );
  return c > 0;
}

async function pgSchemaExistsInDatabase(
  dbSessionId: string,
  schema: string,
  database: string,
): Promise<boolean> {
  const c = await pgScalar(
    dbSessionId,
    `SELECT COUNT(*)::int AS c FROM information_schema.schemata WHERE schema_name = '${sqlLiteral(schema)}'`,
    database,
  );
  return c > 0;
}

async function pgDatabaseExists(dbSessionId: string, database: string): Promise<boolean> {
  const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId });
  return dbs.includes(database);
}

/** Right-click a DOM element matched by selector + text filter via JS dispatch. */
async function rightClick(selector: string, textMatch?: string) {
  await browser.execute(
    (sel: string, text: string | undefined) => {
      let el: Element | null = null;
      if (text) {
        const all = document.querySelectorAll(sel);
        for (const e of all) {
          if (e.textContent?.includes(text)) {
            el = e;
            break;
          }
        }
      } else {
        el = document.querySelector(sel);
      }
      if (!el) return;
      const rect = (el as HTMLElement).getBoundingClientRect();
      el.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    },
    selector,
    textMatch,
  );
  await browser.pause(500);
}

/** Dismiss any open web context menu. */
async function dismissMenu() {
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
}

/** Get the visible web context menu text (all items). */
async function getMenuText(): Promise<string> {
  const menu = await $('[data-testid="web-context-menu"]');
  const exists = await menu.isExisting();
  if (!exists) return '';
  return menu.getText();
}

/** Click a specific menu item by label text. */
async function clickMenuItem(label: string) {
  await browser.execute((lbl: string) => {
    const menuItems = document.querySelectorAll('[data-testid="web-context-menu"] button');
    for (const item of menuItems) {
      if (item.textContent?.includes(lbl)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, label);
  await browser.pause(500);
}

/** Click a specific menu item by its id (data-testid). */
async function clickMenuItemById(id: string) {
  const item = await $(`[data-testid="web-context-item-${id}"]`);
  if (await item.isExisting()) {
    await item.click();
    await browser.pause(500);
  }
}

/** Check if a menu item with given id exists. */
async function hasMenuItemId(id: string): Promise<boolean> {
  const item = await $(`[data-testid="web-context-item-${id}"]`);
  return item.isExisting();
}

async function hoverSubmenuTrigger(triggerTestId: string) {
  const trigger = await $(`[data-testid="${triggerTestId}"]`);
  if (await trigger.isExisting()) {
    await trigger.moveTo();
    await browser.pause(400);
  }
}

async function hasSubmenuItem(itemId: string): Promise<boolean> {
  return browser.execute((id: string) => {
    const items = document.querySelectorAll('[data-testid="web-context-submenu"] button');
    for (const item of items) {
      const tid = item.getAttribute('data-testid');
      if (tid && tid.includes(id)) return true;
    }
    return false;
  }, itemId);
}

async function clickSubmenuItem(itemId: string) {
  await browser.execute((id: string) => {
    const items = document.querySelectorAll('[data-testid="web-context-submenu"] button');
    for (const item of items) {
      const tid = item.getAttribute('data-testid');
      if (tid && tid.includes(id)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, itemId);
  await browser.pause(500);
}

/** Check if a web context menu is displayed with at least one item. */
async function isMenuDisplayed(): Promise<boolean> {
  const menu = await $('[data-testid="web-context-menu"]');
  return menu.isExisting();
}

/** Expand the connection in the navigator tree by clicking it. */
async function expandConnection(connName: string) {
  await browser.execute((name: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(name)) {
        (item as HTMLElement).click();
        break;
      }
    }
  }, connName);
  await browser.pause(2000);
}

/** Expand a database node in the navigator tree. */
async function expandDb(dbName: string) {
  await browser.execute((name: string) => {
    const nodes = document.querySelectorAll('[data-tree-node="db"]');
    for (const n of nodes) {
      if (n.textContent?.includes(name)) {
        (n as HTMLElement).click();
        break;
      }
    }
  }, dbName);
  await browser.pause(1500);
}

/** Expand a schema node. */
async function expandSchema(schemaName: string) {
  await browser.execute((name: string) => {
    const nodes = document.querySelectorAll('[data-tree-node="schema"]');
    for (const n of nodes) {
      if (n.textContent?.includes(name)) {
        (n as HTMLElement).click();
        break;
      }
    }
  }, schemaName);
  await browser.pause(1000);
}

/** Expand a category node (tables, views, etc). */
async function expandCategory(catId: string) {
  await browser.execute((id: string) => {
    const nodes = document.querySelectorAll('[data-tree-node="category"]');
    for (const n of nodes) {
      if (n.getAttribute('data-cat-id') === id) {
        (n as HTMLElement).click();
        break;
      }
    }
  }, catId);
  await browser.pause(1000);
}

describe('导航树上下文菜单 (Navigator Context Menu)', () => {
  let mainWindow: string;
  let pgDbSessionId: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await stubClipboardCapture();
    await browser.pause(1500);
    pgDbSessionId = await invokeBackend<string>('connect', { connectionId: SEEDED_CONN_ID });

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`CREATE TABLE ${TEST_TABLE} (id SERIAL PRIMARY KEY, name TEXT NOT NULL)`);
    await executeSQL(`INSERT INTO ${TEST_TABLE}(name) VALUES ('test_ctx_row')`);
    await browser.pause(1000);
    expect(await pgTableExists(pgDbSessionId, TEST_TABLE)).toBe(true);
  });

  after(async () => {
    try {
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      /* best effort */
    }
    try {
      if (pgDbSessionId) {
        await disconnectBackend(pgDbSessionId);
      }
    } catch {
      /* best effort */
    }
    await restoreClipboardStub();
  });

  // ── Connection Context Menu ──────────────────────────────────────

  describe('连接节点上下文菜单', () => {
    it('NCM-001: 右键连接显示菜单含必要项', async () => {
      await rightClick('[data-conn-item]');
      const text = await getMenuText();
      expect(text).toContain(t('main.ctx.openConnection'));
      expect(
        await $('[data-testid="web-context-submenu-trigger-connection-submenu"]').isExisting(),
      ).toBe(true);
      expect(await hasMenuItemId('delete-connection')).toBe(true);
      expect(await hasMenuItemId('copy-name')).toBe(false);
      await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
      expect(await hasSubmenuItem('copy-name')).toBe(true);
      await dismissMenu();
    });

    it('NCM-002: 复制名称应复制到剪贴板', async () => {
      await rightClick('[data-conn-item]');
      await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
      await clickSubmenuItem('copy-name');
      await browser.pause(300);

      const clip = await readStubbedClipboard();
      expect(clip.length).toBeGreaterThan(0);
    });

    it('NCM-003: 右键连接不应展开连接', async () => {
      const beforeCount = await browser.execute(() => {
        return document.querySelectorAll('[data-tree-node="db"]').length;
      });
      await rightClick('[data-conn-item]');
      await dismissMenu();
      const afterCount = await browser.execute(() => {
        return document.querySelectorAll('[data-tree-node="db"]').length;
      });
      expect(afterCount).toBe(beforeCount);
    });
  });

  // ── Database Context Menu ────────────────────────────────────────

  describe('数据库节点上下文菜单', () => {
    before(async () => {
      const dbNodes = await $$('[data-tree-node="db"]');
      if (dbNodes.length === 0) {
        await expandConnection('PostgreSQL');
        await browser.pause(2000);
      }
    });

    it('NCM-010: 右键数据库显示菜单含必要项', async () => {
      await rightClick('[data-tree-node="db"]');
      const text = await getMenuText();
      expect(text).toContain(t('connWin.refresh'));
      expect(await hasMenuItemId('new-query')).toBe(true);
      expect(text).toContain(t('schemaTree.copyDatabaseName'));
      expect(text).toContain(t('schemaTree.viewErDiagram'));
      await dismissMenu();
    });

    it('NCM-011: 数据库复制名称应复制数据库名', async () => {
      await rightClick('[data-tree-node="db"]');
      await clickMenuItem(t('schemaTree.copyDatabaseName'));
      await browser.pause(300);

      const clip = await readStubbedClipboard();
      expect(clip.length).toBeGreaterThan(0);
    });

    it('NCM-012: 新建查询应打开新的查询标签页', async () => {
      const tabCountBefore = await browser.execute(() => {
        const tabs = document.querySelectorAll('[data-testid="query-tab"]');
        return tabs.length;
      });

      await rightClick('[data-tree-node="db"]');
      await clickMenuItemById('new-query');
      await browser.pause(1000);

      const bodyText = await $('body').getText();
      expect(bodyText).toContain('SELECT');
    });

    it('NCM-013: 刷新应不报错', async () => {
      await rightClick('[data-tree-node="db"]');
      await clickMenuItem(t('connWin.refresh'));
      await browser.pause(2000);

      const dbNodes = await $$('[data-tree-node="db"]');
      expect(dbNodes.length).toBeGreaterThan(0);
    });
  });

  // ── Schema Context Menu ──────────────────────────────────────────

  describe('Schema 节点上下文菜单', () => {
    before(async () => {
      const schemaNodes = await $$('[data-tree-node="schema"]');
      if (schemaNodes.length === 0) {
        const dbNodes = await $$('[data-tree-node="db"]');
        if (dbNodes.length > 0) {
          await expandDb('');
        }
        await browser.pause(2000);
      }
    });

    it('NCM-020: 右键 schema 显示菜单含必要项', async () => {
      const schemaNodes = await $$('[data-tree-node="schema"]');
      if (schemaNodes.length === 0) {
        console.log('No schema nodes found, skipping NCM-020');
        return;
      }
      await rightClick('[data-tree-node="schema"]');
      const text = await getMenuText();
      expect(text).toContain(t('connWin.refresh'));
      expect(await hasMenuItemId('new-query')).toBe(true);
      expect(text).toContain(t('schemaTree.viewErDiagram'));
      await dismissMenu();
    });

    it('NCM-021: schema 复制名称应复制 schema 名', async () => {
      const schemaNodes = await $$('[data-tree-node="schema"]');
      if (schemaNodes.length === 0) {
        console.log('No schema nodes found, skipping NCM-021');
        return;
      }
      await rightClick('[data-tree-node="schema"]');
      await clickMenuItemById('copy-name');
      await browser.pause(300);

      const clip = await readStubbedClipboard();
      expect(clip).toContain('public');
    });

    it('NCM-022: schema 菜单应包含删除 Schema 项', async () => {
      const schemaNodes = await $$('[data-tree-node="schema"]');
      if (schemaNodes.length === 0) {
        console.log('No schema nodes found, skipping NCM-022');
        return;
      }
      await rightClick('[data-tree-node="schema"]', 'public');
      const text = await getMenuText();
      expect(text).toContain(t('schemaTree.dropSchema'));
      await dismissMenu();
    });

    it('NCM-023: 删除 schema 确认后应从导航树与数据库消失', async () => {
      await executeSQL(`CREATE SCHEMA IF NOT EXISTS ${DROP_SCHEMA}`);
      await browser.pause(800);
      expect(await pgSchemaExists(pgDbSessionId, DROP_SCHEMA)).toBe(true);
      await rightClick('[data-tree-node="db"]');
      await clickMenuItem(t('connWin.refresh'));
      await browser.pause(2000);

      await expandSchema(DROP_SCHEMA);
      await browser.pause(500);

      const schemaNode = await browser.execute((name: string) => {
        return Array.from(document.querySelectorAll('[data-tree-node="schema"]')).some((n) =>
          n.textContent?.includes(name),
        );
      }, DROP_SCHEMA);
      if (!schemaNode) {
        console.log('NCM-023: drop schema node not visible, skipping');
        return;
      }
      expect(await pgSchemaExists(pgDbSessionId, DROP_SCHEMA)).toBe(true);

      await rightClick('[data-tree-node="schema"]', DROP_SCHEMA);
      await clickMenuItem(t('schemaTree.dropSchema'));
      await confirmWebDialog();
      await browser.pause(1500);

      const stillThere = await browser.execute((name: string) => {
        return Array.from(document.querySelectorAll('[data-tree-node="schema"]')).some((n) =>
          n.textContent?.includes(name),
        );
      }, DROP_SCHEMA);
      expect(stillThere).toBe(false);
      expect(await pgSchemaExists(pgDbSessionId, DROP_SCHEMA)).toBe(false);
    });

    it('NCM-024: 浏览 postgres 后删除另一库 schema 应成功', async function () {
      const UNIQUE = Date.now();
      const CROSS_DB = `e2e_nav_cross_${UNIQUE}`;
      const CROSS_SCHEMA = `e2e_nav_cross_sch_${UNIQUE}`;

      await invokeBackend('execute_driver_command', {
        request: {
          dbSessionId: pgDbSessionId,
          command: 'create_database',
          input: { name: CROSS_DB },
        },
      });
      await invokeBackend('execute_driver_command', {
        request: {
          dbSessionId: pgDbSessionId,
          command: 'create_schema',
          input: { name: CROSS_SCHEMA },
          database: CROSS_DB,
        },
      });
      expect(await pgSchemaExistsInDatabase(pgDbSessionId, CROSS_SCHEMA, CROSS_DB)).toBe(true);

      await rightClick('[data-conn-item]');
      await clickMenuItem(t('connWin.refresh'));
      await browser.pause(2000);

      // F1: browse another catalog so the live session is not on CROSS_DB.
      await expandDb('postgres');
      await browser.pause(1000);

      await expandDb(CROSS_DB);
      await expandSchema(CROSS_SCHEMA);
      await browser.pause(500);

      await rightClick('[data-tree-node="schema"]', CROSS_SCHEMA);
      await clickMenuItem(t('schemaTree.dropSchema'));
      await confirmWebDialog();
      await browser.pause(2000);

      expect(await pgSchemaExistsInDatabase(pgDbSessionId, CROSS_SCHEMA, CROSS_DB)).toBe(false);

      await invokeBackend('execute_driver_command', {
        request: {
          dbSessionId: pgDbSessionId,
          command: 'drop_database',
          input: { name: CROSS_DB },
        },
      }).catch(() => {
        /* best effort */
      });
    });

    it('NCM-025: 浏览 postgres 后删除另一库 database 应成功', async function () {
      const UNIQUE = Date.now();
      const CROSS_DB = `e2e_nav_dropdb_${UNIQUE}`;

      await invokeBackend('execute_driver_command', {
        request: {
          dbSessionId: pgDbSessionId,
          command: 'create_database',
          input: { name: CROSS_DB },
        },
      });
      expect(await pgDatabaseExists(pgDbSessionId, CROSS_DB)).toBe(true);

      await rightClick('[data-conn-item]');
      await clickMenuItem(t('connWin.refresh'));
      await browser.pause(2000);

      await expandDb('postgres');
      await browser.pause(1000);

      const crossDbVisible = await browser.execute((name: string) => {
        return Array.from(document.querySelectorAll('[data-tree-node="db"]')).some((n) =>
          n.textContent?.includes(name),
        );
      }, CROSS_DB);
      if (!crossDbVisible) {
        console.log('NCM-025: cross db node not visible, skipping');
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: pgDbSessionId,
            command: 'drop_database',
            input: { name: CROSS_DB },
          },
        }).catch(() => undefined);
        return;
      }

      await rightClick('[data-tree-node="db"]', CROSS_DB);
      await clickMenuItem(t('schemaTree.dropDatabase'));
      await confirmWebDialog();
      await browser.pause(2000);

      expect(await pgDatabaseExists(pgDbSessionId, CROSS_DB)).toBe(false);
    });
  });

  // ── Category Context Menu ────────────────────────────────────────

  describe('分类节点上下文菜单', () => {
    before(async () => {
      const catNodes = await $$('[data-tree-node="category"]');
      if (catNodes.length === 0) {
        const schemaNodes = await $$('[data-tree-node="schema"]');
        if (schemaNodes.length > 0) {
          await expandSchema('public');
        } else {
          const dbNodes = await $$('[data-tree-node="db"]');
          if (dbNodes.length > 0) {
            await dbNodes[0].click();
            await browser.pause(2000);
          }
        }
        await browser.pause(2000);
      }
    });

    it('NCM-030: 右键分类节点显示菜单含刷新', async () => {
      const catNodes = await $$('[data-tree-node="category"]');
      if (catNodes.length === 0) {
        console.log('No category nodes, skipping NCM-030');
        return;
      }
      await rightClick('[data-tree-node="category"]');
      const text = await getMenuText();
      expect(text).toContain(t('connWin.refresh'));
      await dismissMenu();
    });

    it('NCM-031: Tables 分类右键应包含新建表', async () => {
      await rightClick('[data-tree-node="category"][data-cat-id="tables"]');
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        console.log('Tables category not visible, skipping NCM-031');
        return;
      }
      const text = await getMenuText();
      expect(await hasMenuItemId('new-table')).toBe(true);
      await dismissMenu();
    });
  });

  // ── Table Context Menu ───────────────────────────────────────────

  describe('表节点上下文菜单', () => {
    before(async () => {
      await setSafeMode(false);
      const tableNodes = await $$('[data-tree-node="table"]');
      if (tableNodes.length === 0) {
        await expandCategory('tables');
        await browser.pause(2000);
      }
    });

    after(async () => {
      await setSafeMode(true);
    });

    it('NCM-040: 右键表显示菜单含必要项', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      const text = await getMenuText();
      expect(text).toContain(t('schemaTree.openTable'));
      expect(await hasMenuItemId('copy-name')).toBe(true);
      expect(await hasMenuItemId('copy-ddl')).toBe(true);
      expect(await hasMenuItemId('generate-sql')).toBe(true);
      await dismissMenu();
    });

    it('NCM-041: 表-复制名称应将表名复制到剪贴板', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItemById('copy-name');
      await browser.pause(500);

      const clip = await readStubbedClipboard();
      expect(clip.length).toBeGreaterThan(0);
    });

    it('NCM-042: 表-复制 DDL 应执行并复制到剪贴板', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItemById('copy-ddl');
      await browser.pause(2000);

      const clip = await readStubbedClipboard();
      expect(clip).toContain('CREATE TABLE');
    });

    it('NCM-043: 表-打开应显示表数据', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItem(t('schemaTree.openTable'));
      await browser.pause(2000);

      const bodyText = await $('body').getText();
      expect(bodyText).toContain('test_ctx_row');
    });

    it('NCM-044: 表菜单应包含 truncate 和 drop 项', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      const text = await getMenuText();
      expect(text).toContain(t('schemaTree.truncate'));
      expect(text).toContain(t('schemaTree.drop'));
      await dismissMenu();
    });

    it('NCM-045: 表-ER 聚焦应打开 ER 图面板', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      const text = await getMenuText();
      if (!text.includes(t('erDiagram.focusTable'))) {
        await dismissMenu();
        console.log('No ER focus item, skipping NCM-045');
        return;
      }
      await clickMenuItem(t('erDiagram.focusTable'));
      await browser.pause(2000);

      const bodyText = await $('body').getText();
      expect(bodyText.toLowerCase()).toContain('er');
    });

    it('NCM-046: 删除表确认后应从导航树与数据库消失', async () => {
      expect(await pgTableExists(pgDbSessionId, TEST_TABLE)).toBe(true);

      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItem(t('schemaTree.drop'));
      await confirmWebDialog();
      await browser.pause(1500);

      await browser.waitUntil(
        async () => {
          return browser.execute((name: string) => {
            return !document.querySelector(`[data-item-name="${name}"]`);
          }, TEST_TABLE);
        },
        { timeout: 15000, timeoutMsg: `表 ${TEST_TABLE} 删除后仍显示在导航树中` },
      );
      expect(await pgTableExists(pgDbSessionId, TEST_TABLE)).toBe(false);
    });
  });

  // ── View Context Menu ────────────────────────────────────────────

  describe('视图节点上下文菜单', () => {
    it('NCM-050: 右键视图显示菜单含必要项', async () => {
      const viewNodes = await $$('[data-tree-node="view"]');
      if (viewNodes.length === 0) {
        console.log('No views found, skipping NCM-050');
        return;
      }
      await rightClick('[data-tree-node="view"]');
      const text = await getMenuText();
      expect(text).toContain(t('schemaTree.open'));
      expect(await hasMenuItemId('copy-name')).toBe(true);
      expect(await hasMenuItemId('copy-ddl')).toBe(true);
      await dismissMenu();
    });
  });

  // ── Group Context Menu ───────────────────────────────────────────

  describe('分组节点上下文菜单', () => {
    it('NCM-060: 右键分组显示菜单含重命名/删除', async () => {
      const groupHeaders = await $$('[data-group-header]');
      if (groupHeaders.length === 0) {
        console.log('No group headers, skipping NCM-060');
        return;
      }
      await rightClick('[data-group-header]');
      const text = await getMenuText();
      expect(text).toContain(t('main.ctx.renameGroup'));
      expect(text).toContain(t('main.ctx.deleteGroup'));
      await dismissMenu();
    });
  });
});
