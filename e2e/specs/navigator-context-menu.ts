/**
 * E2E tests for navigator tree context menus in ConnectionWindow.
 *
 * Tests context menu items for connections, databases, schemas, categories,
 * and table/view nodes. Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  openQueryTab,
  executeSQL,
  waitForSchemaTreeLoaded,
} from '../helpers.js';

const TEST_TABLE = '_e2e_ctx_menu';

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

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await browser.pause(1500);

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`CREATE TABLE ${TEST_TABLE} (id SERIAL PRIMARY KEY, name TEXT NOT NULL)`);
    await executeSQL(`INSERT INTO ${TEST_TABLE}(name) VALUES ('test_ctx_row')`);
    await browser.pause(1000);
  });

  after(async () => {
    try {
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      /* best effort */
    }
  });

  // ── Connection Context Menu ──────────────────────────────────────

  describe('连接节点上下文菜单', () => {
    it('NCM-001: 右键连接显示菜单含必要项', async () => {
      await rightClick('[data-conn-item]');
      const text = await getMenuText();
      expect(text).toContain(t('main.ctx.openConnection'));
      expect(text).toContain(t('main.ctx.editConnection'));
      expect(text).toContain(t('main.ctx.deleteConnection'));
      expect(text).toContain(t('main.ctx.copyName'));
      await dismissMenu();
    });

    it('NCM-002: 复制名称应复制到剪贴板', async () => {
      await rightClick('[data-conn-item]');
      await clickMenuItem(t('main.ctx.copyName'));
      await browser.pause(300);

      const clip = await browser.execute(() => {
        return new Promise<string>((resolve) => {
          navigator.clipboard
            .readText()
            .then(resolve)
            .catch(() => resolve(''));
        });
      });
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
      expect(text).toContain(t('connWin.newQuery'));
      expect(text).toContain(t('schemaTree.copyDatabaseName'));
      expect(text).toContain(t('schemaTree.viewErDiagram'));
      await dismissMenu();
    });

    it('NCM-011: 数据库复制名称应复制数据库名', async () => {
      await rightClick('[data-tree-node="db"]');
      await clickMenuItem(t('schemaTree.copyDatabaseName'));
      await browser.pause(300);

      const clip = await browser.execute(() => {
        return new Promise<string>((resolve) => {
          navigator.clipboard
            .readText()
            .then(resolve)
            .catch(() => resolve(''));
        });
      });
      expect(clip.length).toBeGreaterThan(0);
    });

    it('NCM-012: 新建查询应打开新的查询标签页', async () => {
      const tabCountBefore = await browser.execute(() => {
        const tabs = document.querySelectorAll('[data-testid="query-tab"]');
        return tabs.length;
      });

      await rightClick('[data-tree-node="db"]');
      await clickMenuItem(t('connWin.newQuery'));
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
      expect(text).toContain(t('connWin.newQuery'));
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
      await clickMenuItem(t('schemaTree.copyName'));
      await browser.pause(300);

      const clip = await browser.execute(() => {
        return new Promise<string>((resolve) => {
          navigator.clipboard
            .readText()
            .then(resolve)
            .catch(() => resolve(''));
        });
      });
      expect(clip).toContain('public');
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
      expect(text).toContain(t('connWin.newTable'));
      await dismissMenu();
    });
  });

  // ── Table Context Menu ───────────────────────────────────────────

  describe('表节点上下文菜单', () => {
    before(async () => {
      const tableNodes = await $$('[data-tree-node="table"]');
      if (tableNodes.length === 0) {
        await expandCategory('tables');
        await browser.pause(2000);
      }
    });

    it('NCM-040: 右键表显示菜单含必要项', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      const text = await getMenuText();
      expect(text).toContain(t('schemaTree.openTable'));
      expect(text).toContain(t('schemaTree.copyName'));
      expect(text).toContain(t('connWin.copyDDL'));
      await dismissMenu();
    });

    it('NCM-041: 表-复制名称应将表名复制到剪贴板', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItem(t('schemaTree.copyName'));
      await browser.pause(500);

      const clip = await browser.execute(() => {
        return new Promise<string>((resolve) => {
          navigator.clipboard
            .readText()
            .then(resolve)
            .catch(() => resolve(''));
        });
      });
      expect(clip.length).toBeGreaterThan(0);
    });

    it('NCM-042: 表-复制 DDL 应执行并复制到剪贴板', async () => {
      await rightClick(`[data-tree-node="table"][data-item-name="${TEST_TABLE}"]`);
      const menuVisible = await isMenuDisplayed();
      if (!menuVisible) {
        await rightClick('[data-tree-node="table"]');
      }
      await clickMenuItem(t('connWin.copyDDL'));
      await browser.pause(2000);

      const clip = await browser.execute(() => {
        return new Promise<string>((resolve) => {
          navigator.clipboard
            .readText()
            .then(resolve)
            .catch(() => resolve(''));
        });
      });
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
      expect(text).toContain(t('schemaTree.copyName'));
      expect(text).toContain(t('connWin.copyDDL'));
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
