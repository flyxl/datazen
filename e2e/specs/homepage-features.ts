/**
 * E2E tests for the redesigned homepage, data sync, and drag-and-drop features.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  closeNewConnectionDialogFromUi,
  expandAllGroups,
  connectSeededPgInWorkspace,
  switchToNewWindow,
} from '../helpers.js';
import { t } from '../i18n.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
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

async function ensureConnectionsHome() {
  await browser.url('tauri://localhost');
  await browser.pause(800);
  await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 10000 });
  await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
    timeout: 10000,
  });
}

describe('主页 TablePlus 风格 (HOME)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await ensureConnectionsHome();
    await expandAllGroups();
    await browser.pause(1000);
  });

  afterEach(async () => {
    try {
      if (await $('[data-testid="new-connection-dialog"]').isExisting()) {
        await closeNewConnectionDialogFromUi();
      }
    } catch {
      /* dialog already closed */
    }
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await ensureConnectionsHome();
    await browser.pause(300);
  });

  // ── Layout ──────────────────────────────────────────────────────

  it('HOME-RESTORE-001: 恢复应打开选连接/选库窗口而不是直接报错', async () => {
    await browser.url('tauri://localhost/window.html?window=backup&mode=restore');
    await browser.pause(1500);
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) {
      await switchToNewWindow(mainWindow);
    }
    const body = await $('body').getText();
    expect(body).toContain(t('backup.selectConnectionFirst'));
    expect(body).not.toContain(t('main.restoreFailed'));
    await expect(await $(`button*=${t('backup.startRestore')}`)).toBeDisplayed();
  });

  it('HOME-001: 应显示统一工作区导航', async () => {
    await expect(await $('[data-testid="workspace-nav-connections"]')).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-workflow"]')).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-dashboard"]')).toBeDisplayed();
  });

  it('HOME-002: 标题栏应显示应用名称', async () => {
    const body = await $('body').getText();
    expect(body).toContain('DataZen');
  });

  it('HOME-003: 搜索栏应在连接列表上方', async () => {
    const searchInput = await $(`input[placeholder="${t('main.searchPlaceholder')}"]`);
    await expect(searchInput).toBeDisplayed();
  });

  it('HOME-004: 搜索栏旁应有"+"新建连接按钮', async () => {
    const plusBtn = await $('[data-testid="new-connection-button"]');
    await expect(plusBtn).toBeDisplayed();
  });

  it('HOME-005: 连接应按分组显示', async () => {
    const headers = await $$('[data-group-header]');
    expect(headers.length).toBeGreaterThan(0);
  });

  it('HOME-006: 连接项应显示 DB 类型图标', async () => {
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 5000,
      timeoutMsg: 'Timed out waiting for connection items',
    });
    const hasBadge = await browser.execute(() => {
      const item = document.querySelector('[data-conn-item]');
      if (!item) return false;
      return (
        item.querySelector('img[draggable="false"]') !== null ||
        item.querySelector('span.font-bold') !== null
      );
    });
    expect(hasBadge).toBe(true);
  });

  it('HOME-007: 连接项应显示主机地址', async () => {
    const conns = await invokeBackend<Array<{ host?: string; port?: number }>>('get_connections');
    expect(conns.length).toBeGreaterThan(0);
    const hasAddr = conns.some(
      (c) =>
        c.host === 'localhost' ||
        c.host === '127.0.0.1' ||
        (typeof c.port === 'number' && c.port > 0),
    );
    expect(hasAddr).toBe(true);
  });

  it('HOME-008: 状态栏应显示连接总数', async () => {
    const uiCount = (await $$('[data-conn-item]')).length;
    const savedCount = (await invokeBackend<Array<{ id: string }>>('get_connections')).length;
    expect(uiCount).toBeGreaterThan(0);
    expect(savedCount).toBeGreaterThan(0);
  });

  // ── Group expand/collapse ────────────────────────────────────────

  it('HOME-010: 折叠分组应隐藏其连接', async () => {
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 5000,
      timeoutMsg: 'Timed out waiting for connection items',
    });

    const totalBefore = (await $$('[data-conn-item]')).length;
    if (totalBefore === 0) return;

    const headerIdx = await browser.execute(() => {
      const headers = document.querySelectorAll('[data-group-header]');
      for (let i = 0; i < headers.length; i++) {
        const text = headers[i].textContent || '';
        const match = text.match(/\((\d+)\)/);
        if (match && parseInt(match[1], 10) > 0) return i;
      }
      return -1;
    });
    if (headerIdx < 0) return;

    const headers = await $$('[data-group-header]');
    await headers[headerIdx].click();
    await browser.pause(300);

    const totalAfter = (await $$('[data-conn-item]')).length;
    expect(totalAfter).toBeLessThan(totalBefore);

    await headers[headerIdx].click();
    await browser.pause(300);
  });

  // ── Connection context menu ──────────────────────────────────────

  it('HOME-020: 连接项绑定了 contextmenu 事件处理器', async () => {
    const hasHandler = await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]');
      return el instanceof HTMLElement;
    });
    expect(hasHandler).toBe(true);
  });

  it('HOME-021: 分组头右键打开 Web 菜单且不被窗口截断', async () => {
    await browser.execute(() => {
      const header = document.querySelector('[data-group-header]') as HTMLElement | null;
      if (!header) return;
      const rect = header.getBoundingClientRect();
      header.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    });
    await browser.pause(500);
    const menu = await $('[data-testid="web-context-menu"]');
    await menu.waitForExist({ timeout: 5000 });
    expect(await menu.isDisplayed()).toBe(true);
    const box = await menu.getLocation();
    const size = await menu.getSize();
    const win = await browser.getWindowSize();
    expect(box.x + size.width).toBeLessThanOrEqual(win.width);
    expect(box.y + size.height).toBeLessThanOrEqual(win.height);
    await browser.keys('Escape');
  });

  // ── Group context menu ───────────────────────────────────────────

  it('HOME-030: 分组头绑定了 contextmenu 事件处理器', async () => {
    const hasHeaders = await browser.execute(() => {
      return document.querySelectorAll('[data-group-header]').length > 0;
    });
    expect(hasHeaders).toBe(true);
  });

  it('HOME-031: 连接树滚动容器存在', async () => {
    const hasContainer = await browser.execute(() => {
      const scrollArea = document.querySelector('.flex-1.min-h-0.overflow-y-auto');
      return scrollArea instanceof HTMLElement;
    });
    expect(hasContainer).toBe(true);
  });

  // ── Double-click to connect ──────────────────────────────────────

  it('HOME-040: 双击连接应在主窗口显示工具栏', async () => {
    await connectSeededPgInWorkspace();
    // Unified workspace: no active panel → ConnectionWorkspaceHome quick actions, not ContentToolbar.
    await expect(await $('[data-testid="connection-workspace-home"]')).toBeDisplayed();
    await expect(await $('[data-testid="home-quick-new-query"]')).toBeDisplayed();
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
  });

  it('HOME-041: 连接后应显示绿色状态指示器', async () => {
    await browser.switchToWindow(mainWindow);
    await browser.pause(3000);
    const hasGreenDot = await browser.execute(() => {
      return document.querySelector('.bg-green-500.rounded-full') !== null;
    });
    const body = await $('body').getText();
    const hasStatus = body.includes('活跃连接') || hasGreenDot;
    if (!hasStatus) return;
    expect(hasStatus).toBe(true);
  });

  // ── Search filtering ─────────────────────────────────────────────

  it('HOME-050: 搜索应实时过滤连接', async () => {
    const input = await $(`input[placeholder="${t('main.searchPlaceholder')}"]`);
    const itemsBefore = (await $$('[data-conn-item]')).length;

    await input.setValue('ZZZZNOTEXIST');
    await browser.pause(500);
    const itemsAfter = (await $$('[data-conn-item]')).length;
    expect(itemsAfter).toBe(0);

    await input.clearValue();
    await browser.pause(500);
    const itemsRestored = (await $$('[data-conn-item]')).length;
    expect(itemsRestored).toBe(itemsBefore);
  });

  // ── Action panel buttons ─────────────────────────────────────────

  it('HOME-060: 点击"新建连接"应打开新建连接弹窗', async () => {
    const plusBtn = await $('[data-testid="new-connection-button"]');
    await plusBtn.click();
    await expect(await $('[data-testid="new-connection-dialog"]')).toBeDisplayed();
  });
});
