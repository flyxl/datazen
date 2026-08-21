/**
 * E2E: 对象过滤器 ObjectFilterDialog 完整闭环（ops §5.4）
 *
 * 全流程：建测试表 → 连接 → 右键连接「对象过滤…」→ 填 include/exclude + 隐藏系统库 →
 * 保存 → 刷新树断言仅显示命中表（同时验证 ObjectFilterDialog 的可打开性——即 import 修复回归）→
 * 重新打开断言持久化回填 → 清空保存断言恢复。
 *
 * 数据构造：seeded PG 的某个库重建 e2e_ft_1 / e2e_ft_2（命中）与 plain_table（排除）。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQL,
  openQueryTab,
  withSafeModeOff,
} from '../helpers.js';

const STAMP = Date.now().toString(36);
const FT1 = `e2e_ft_1_${STAMP}`;
const FT2 = `e2e_ft_2_${STAMP}`;
const PLAIN = `plain_table_${STAMP}`;

async function rightClickConn(connName: string | undefined) {
  const ok = await browser.execute((name: string | undefined) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    let item: Element | null = null;
    if (name) {
      item =
        items.find((el) => {
          const attr = el.getAttribute('data-conn-name');
          if (attr) return attr === name;
          return el.querySelector('span.truncate')?.textContent?.trim() === name;
        }) ?? null;
    } else {
      item = items[0] ?? null;
    }
    if (!item) return false;
    const rect = (item as HTMLElement).getBoundingClientRect();
    item.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
    return true;
  }, connName);
  await browser.pause(400);
  return ok;
}

/** 当前 web context menu 内所有按钮文本。 */
async function menuText(): Promise<string> {
  const menu = await $('[data-testid="web-context-menu"]');
  if (!(await menu.isExisting())) return '';
  return menu.getText();
}

/** 点击菜单项（文本包含匹配）。 */
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

/** 关闭 web context menu。 */
async function dismissMenu() {
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
}

/** 读取当前可见的 database 树（表 / 视图）节点文本名称集合。 */
async function visibleTableNames(): Promise<string[]> {
  await browser.pause(600);
  return browser.execute(() => {
    const names: string[] = [];
    document.querySelectorAll('[data-tree-node="table"]').forEach((n) => {
      const it = n.getAttribute('data-item-name');
      if (it) names.push(it);
    });
    document.querySelectorAll('[data-tree-node="view"]').forEach((n) => {
      const it = n.getAttribute('data-item-name');
      if (it) names.push(it);
    });
    return names;
  });
}

/** 展开连接 → 其下某 schema（默认 public）以加载表节点。 */
async function expandSchemaTables(schemaFilter = 'public') {
  await browser.execute(() => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const conn = items[0];
    (conn as HTMLElement)?.click();
  });
  await browser.pause(1500);
  await browser.execute((s: string) => {
    const schemas = Array.from(document.querySelectorAll('[data-tree-node="schema"]'));
    const target = schemas.find((el) => el.textContent?.toLowerCase().includes(s.toLowerCase()));
    (target as HTMLElement)?.click();
  }, schemaFilter);
  await browser.pause(1200);
  await browser.execute(() => {
    const cats = Array.from(document.querySelectorAll('[data-tree-node="category"]'));
    const tables = cats.find((c) => c.getAttribute('data-cat-id') === 'tables');
    (tables as HTMLElement)?.click();
  });
  await browser.pause(1200);
}

/** 读取 ObjectFilter 对话框中某输入框（按 placeholder）当前值，供回填断言。 */
async function dialogInputValue(placeholder: string): Promise<string> {
  const el = await $(`input[placeholder="${placeholder}"]`);
  const v = await el.getValue();
  return v;
}

/** 清空一个输入框并 setValue。 */
async function setInputByPlaceholder(placeholder: string, value: string) {
  const el = await $(`input[placeholder="${placeholder}"]`);
  await el.clearValue();
  if (value) await el.setValue(value);
  await browser.pause(200);
}

describe('运维 §5.4: 对象过滤器 (OPS-FILTER)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    // 用 seeded PG 建测试表
    await connectSeededPgInWorkspace();
    await openQueryTab();
    await withSafeModeOff(async () => {
      await executeSQL(`DROP TABLE IF EXISTS ${FT1}`);
      await executeSQL(`DROP TABLE IF EXISTS ${FT2}`);
      await executeSQL(`DROP TABLE IF EXISTS ${PLAIN}`);
      await executeSQL(`CREATE TABLE ${FT1} (id int PRIMARY KEY)`);
      await executeSQL(`CREATE TABLE ${FT2} (id int PRIMARY KEY)`);
      await executeSQL(`CREATE TABLE ${PLAIN} (id int PRIMARY KEY)`);
    });
    await closeExtraWindows(mainWindow);
  });

  after(async () => {
    try {
      await connectSeededPgInWorkspace();
      await openQueryTab();
      await withSafeModeOff(async () => {
        await executeSQL(`DROP TABLE IF EXISTS ${FT1}`);
        await executeSQL(`DROP TABLE IF EXISTS ${FT2}`);
        await executeSQL(`DROP TABLE IF EXISTS ${PLAIN}`);
      });
    } catch {
      /* best effort */
    }
    await closeExtraWindows(mainWindow);
  });

  it('OPS-FILTER-001: 右键连接菜单含「对象过滤」并打开对话框', async () => {
    const ok = await rightClickConn('');
    expect(ok).toBe(true);
    const text = await menuText();
    expect(text).toContain(t('main.ctx.objectFilter'));
    await clickMenuItem(t('main.ctx.objectFilter'));
    // 对话框打开（含标题文案）
    await expect(await $(`[role="dialog"]`)).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('objectFilter.title'));
  });

  it('OPS-FILTER-002: 设置 include=e2e_* 与隐藏系统库并保存', async () => {
    // 检查框勾选隐藏系统库
    await browser.execute((label: string) => {
      const labels = Array.from(document.querySelectorAll('[role="dialog"] label'));
      const target = labels.find((l) => l.textContent?.includes(label));
      if (target) (target.querySelector('input[type="checkbox"]') as HTMLElement)?.click();
    }, t('objectFilter.hideSystemSchemas'));
    await setInputByPlaceholder(t('objectFilter.includePlaceholder'), 'e2e_*');
    // 保存
    const save = await $(`[role="dialog"] button*=${t('common.save')}`);
    await save.click();
    await browser.pause(600);
    // 对话框应关闭
    await browser.waitUntil(
      async () =>
        !(await $(`[role="dialog"]`)
          .isDisplayed()
          .catch(() => false)),
      {
        timeout: 8000,
        timeoutMsg: '保存后对象过滤对话框未关闭',
      },
    );
  });

  it('OPS-FILTER-003: 保存后树只显示命中 include 的表', async () => {
    // 重新连接以应用过滤（或刷新树）
    await closeExtraWindows(mainWindow);
    await connectSeededPgInWorkspace();
    await browser.pause(1500);
    await expandSchemaTables();
    const names = await visibleTableNames();
    expect(names).toContain(FT1);
    expect(names).toContain(FT2);
    expect(names).not.toContain(PLAIN);
  });

  it('OPS-FILTER-004: 重新打开对话框断言设置持久化回填', async () => {
    await rightClickConn('');
    await clickMenuItem(t('main.ctx.objectFilter'));
    await browser.pause(500);
    const includeVal = await dialogInputValue(t('objectFilter.includePlaceholder'));
    expect(includeVal).toBe('e2e_*');
    // 取消关闭
    const cancelBtn = await $(`[role="dialog"] button*=${t('common.cancel')}`);
    await cancelBtn.click();
  });

  it('OPS-FILTER-005: 清空 include 保存后树恢复', async () => {
    await rightClickConn('');
    await clickMenuItem(t('main.ctx.objectFilter'));
    await setInputByPlaceholder(t('objectFilter.includePlaceholder'), '');
    const save = await $(`[role="dialog"] button*=${t('common.save')}`);
    await save.click();
    await browser.pause(500);
    await expandSchemaTables();
    const names = await visibleTableNames();
    expect(names).toContain(PLAIN);
  });
});
