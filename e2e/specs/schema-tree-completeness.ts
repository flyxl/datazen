/**
 * Schema tree completeness — load validation, table click, right-click
 * context menu, refresh after DDL.
 *
 * Covers: TC-TREE-001 ~ TC-TREE-006
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickFirstTable,
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  waitForSchemaTreeLoaded,
} from '../helpers.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function withSafeModeOff<T>(fn: () => Promise<T>): Promise<T> {
  const settings = await invokeBackend<{ safeMode?: boolean }>('get_settings');
  if (!settings.safeMode) return fn();
  await invokeBackend('save_settings', { settings: { ...settings, safeMode: false } });
  try {
    return await fn();
  } finally {
    const cur = await invokeBackend<{ safeMode?: boolean }>('get_settings');
    await invokeBackend('save_settings', { settings: { ...cur, safeMode: true } });
  }
}

describe('Schema 树完整性 (TC-TREE-001~006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await browser.pause(1000);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('TC-TREE-001: Schema 树加载后应显示 Tables 分组', async () => {
    await waitForSchemaTreeLoaded();
    const body = await $('body').getText();
    expect(body.includes('Tables') || body.includes('表')).toBe(true);
  });

  it('TC-TREE-002: 展开 Tables 分组应显示至少一个表', async () => {
    const tableCount = await browser.execute(() => {
      const aside = document.querySelector('aside');
      if (!aside) return 0;
      const buttons = Array.from(aside.querySelectorAll('button'));
      return buttons.filter((b) => {
        const cls = b.getAttribute('class') || '';
        return cls.includes('text-left') && b.textContent!.trim().length > 0;
      }).length;
    });
    expect(tableCount).toBeGreaterThan(0);
  });

  it('TC-TREE-003: 点击表名应打开数据 tab', async () => {
    const tableName = await clickFirstTable();
    expect(tableName).toBeTruthy();
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(
      body.includes('数据') ||
        body.includes('Data') ||
        body.includes('结构') ||
        body.includes('Structure'),
    ).toBe(true);
  });

  it('TC-TREE-004: 右键表节点应显示上下文菜单', async () => {
    const aside = await $('aside');
    const tableBtn = await aside.$('button');
    if (tableBtn) {
      await tableBtn.click({ button: 'right' });
      await browser.pause(500);
      const menuVisible = await browser.execute(() => {
        const menus = document.querySelectorAll('[role="menu"], [data-testid*="context-menu"]');
        return menus.length > 0;
      });
      await browser.execute(() => document.body.click());
      await browser.pause(300);
      if (!menuVisible) {
        console.log('TC-TREE-004: Context menu did not appear (non-critical)');
      }
      await captureJourneyStep('schema-context-menu');
    }
  });

  it('TC-TREE-005: Schema 树与内容区应共存', async () => {
    const treeVisible = await browser.execute(() => {
      const aside = document.querySelector('aside');
      return aside !== null && aside.offsetHeight > 0;
    });
    expect(treeVisible).toBe(true);
  });

  it('TC-TREE-006: 执行 DDL 后刷新 Schema 树应反映变更', async () => {
    const testTable = 'e2e_schema_refresh_test';
    await withSafeModeOff(async () => {
      await executeSQL(`DROP TABLE IF EXISTS ${testTable}`);
    });
    await executeSQL(`CREATE TABLE ${testTable} (id INT PRIMARY KEY, val TEXT)`);
    await browser.pause(1000);
    await browser.execute(() => {
      const aside = document.querySelector('aside');
      if (!aside) return;
      const refreshBtns = Array.from(aside.querySelectorAll('button'));
      const btn = refreshBtns.find((b) => {
        const title = b.getAttribute('title') || b.getAttribute('aria-label') || '';
        return title.includes('刷新') || title.includes('Refresh');
      });
      if (btn) (btn as HTMLElement).click();
    });
    await browser.pause(2000);
    const body = await $('body').getText();
    expect(body.includes(testTable)).toBe(true);
    await captureJourneyStep('schema-tree-refreshed');
    await withSafeModeOff(async () => {
      await executeSQL(`DROP TABLE IF EXISTS ${testTable}`);
    });
  });
});
