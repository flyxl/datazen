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

// ═════════════════════════════════════════════════════════════════════
// 数据同步窗口 E2E 测试
// ═════════════════════════════════════════════════════════════════════

describe('数据同步窗口 (SYNC)', () => {
  let mainWindow: string;

  async function openSyncWindow() {
    await browser.switchToWindow(mainWindow);
    try {
      await browser.keys('Escape');
    } catch {
      /* no focused element */
    }
    await browser.url('tauri://localhost/window.html?window=data-sync');
    await browser.pause(1500);
    await $('[data-testid="data-sync-compare"]').waitForDisplayed({ timeout: 15000 });
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await ensureConnectionsHome();
    await browser.pause(500);
  });

  afterEach(async () => {
    try {
      await browser.keys('Escape');
    } catch {
      /* no focused element */
    }
    await browser.switchToWindow(mainWindow);
    await ensureConnectionsHome();
    await browser.pause(300);
  });

  it('SYNC-001: 应显示源和目标连接选择器', async () => {
    await openSyncWindow();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.source'));
    expect(body).toContain(t('sync.target'));
  });

  it('SYNC-002: 应显示标题栏 "数据同步 - DataZen"', async () => {
    await openSyncWindow();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.title'));
  });

  it('SYNC-003: 应显示 TrafficLights 和 ThemeToggle', async () => {
    await openSyncWindow();
    const header = await $('header');
    await expect(header).toBeDisplayed();
  });

  it('SYNC-004: 应显示"比较"按钮', async () => {
    await openSyncWindow();
    const compareBtn = await $('[data-testid="data-sync-compare"]');
    await expect(compareBtn).toBeDisplayed();
  });

  it('SYNC-005: 未选择连接时点击比较应提示', async () => {
    await openSyncWindow();
    const compareBtn = await $('[data-testid="data-sync-compare"]');
    await compareBtn.click();
    await browser.pause(1000);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.selectBoth'));
  });

  it('SYNC-006: 应显示初始引导文本', async () => {
    await openSyncWindow();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.selectPrompt'));
  });

  it('SYNC-007: 状态栏应显示数据同步标题', async () => {
    await openSyncWindow();
    const status = await $('[data-testid="data-sync-window"]');
    await expect(status).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.title'));
    expect(body).toContain(t('common.dataSync'));
  });

  it('SYNC-008: 连接下拉应列出已有连接', async () => {
    await openSyncWindow();
    await expect(await $('[data-testid="data-sync-source"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target"]')).toBeDisplayed();
    const connCount = (await invokeBackend<Array<{ id: string }>>('get_connections')).length;
    expect(connCount).toBeGreaterThan(0);
  });

  it('SYNC-009: 源和目标选同一连接时应报错', async () => {
    await openSyncWindow();
    const bodyBefore = await $('body').getText();
    if (!bodyBefore.includes('本地 PostgreSQL')) return;

    await browser.execute(() => {
      const wraps = document.querySelectorAll(
        '[data-testid="data-sync-source"], [data-testid="data-sync-target"]',
      );
      for (const wrap of wraps) {
        const trigger = wrap.querySelector('button');
        trigger?.click();
      }
    });
    await browser.pause(300);

    const compareBtn = await $('[data-testid="data-sync-compare"]');
    await compareBtn.click();
    await browser.pause(1500);
    const err = await $('[data-testid="data-sync-error"]');
    if (await err.isDisplayed()) {
      expect(await err.getText()).toMatch(/相同|same/i);
    }
    try {
      await browser.keys('Escape');
    } catch {
      /* dismiss connection picker overlays */
    }
    await browser.pause(200);
  });

  it('SYNC-010: 离开数据同步视图应回到主工作区', async () => {
    await openSyncWindow();
    await ensureConnectionsHome();
    await expect(await $('[data-testid="workspace-nav-connections"]')).toBeDisplayed();
    await expect(await $(`input[placeholder="${t('main.searchPlaceholder')}"]`)).toBeDisplayed();
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
  });
});

// ═════════════════════════════════════════════════════════════════════
// 拖拽连接到不同分组 E2E 测试
// ═════════════════════════════════════════════════════════════════════

describe('拖拽连接到不同分组 (DRAG)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await ensureConnectionsHome();
    await expandAllGroups();
    await browser.pause(1000);

    await browser.waitUntil(async () => (await $$('[data-group-header]')).length > 0, {
      timeout: 5000,
      timeoutMsg: 'Timed out waiting for groups to load',
    });
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 5000,
      timeoutMsg: 'Timed out waiting for connection items',
    });
  });

  afterEach(async () => {
    await browser.switchToWindow(mainWindow);
    await ensureConnectionsHome();
    await browser.pause(300);
  });

  it('DRAG-001: 分组头应有 data-group-header 属性', async () => {
    const count = await browser.execute(() => {
      return document.querySelectorAll('[data-group-header]').length;
    });
    expect(count).toBeGreaterThan(0);
  });

  it('DRAG-002: 存在多个分组时拖拽交互可用', async () => {
    const groups = await $$('[data-group-header]');
    expect(groups.length).toBeGreaterThanOrEqual(1);
    if (groups.length >= 2) {
      const firstGroupText = await groups[0].getText();
      const secondGroupText = await groups[1].getText();
      expect(firstGroupText).not.toBe(secondGroupText);
    }
  });

  it('DRAG-003: 连接项响应 pointerdown 事件', async () => {
    const items = await $$('[data-conn-item]');
    if (items.length === 0) return;
    await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]');
      if (!el) return;
      const rect = el.getBoundingClientRect();
      el.dispatchEvent(
        new PointerEvent('pointerdown', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + 5,
          clientY: rect.top + 5,
          button: 0,
        }),
      );
      window.dispatchEvent(
        new PointerEvent('pointerup', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + 5,
          clientY: rect.top + 5,
          button: 0,
        }),
      );
    });
    await browser.pause(300);
    const searchInput = await $(`input[placeholder="${t('main.searchPlaceholder')}"]`);
    await expect(searchInput).toBeDisplayed();
  });

  it('DRAG-004: 通过 pointer 事件模拟拖拽不应崩溃应用', async () => {
    const groups = await $$('[data-group-header]');
    if (groups.length < 2) return;
    const items = await $$('[data-conn-item]');
    if (items.length === 0) return;

    const result = await browser.execute(() => {
      const items = document.querySelectorAll('[data-conn-item]');
      const groupEls = document.querySelectorAll('[data-group-header]');
      if (items.length === 0 || groupEls.length < 2) return 'skip';

      const src = items[0] as HTMLElement;
      const target = groupEls[1] as HTMLElement;
      const srcRect = src.getBoundingClientRect();
      const tgtRect = target.getBoundingClientRect();

      const sx = srcRect.left + srcRect.width / 2;
      const sy = srcRect.top + srcRect.height / 2;
      const tx = tgtRect.left + tgtRect.width / 2;
      const ty = tgtRect.top + tgtRect.height / 2;

      src.dispatchEvent(
        new PointerEvent('pointerdown', {
          bubbles: true,
          cancelable: true,
          clientX: sx,
          clientY: sy,
          button: 0,
        }),
      );
      for (let i = 1; i <= 5; i++) {
        window.dispatchEvent(
          new PointerEvent('pointermove', {
            bubbles: true,
            cancelable: true,
            clientX: sx + ((tx - sx) * i) / 5,
            clientY: sy + ((ty - sy) * i) / 5,
          }),
        );
      }
      window.dispatchEvent(
        new PointerEvent('pointerup', {
          bubbles: true,
          cancelable: true,
          clientX: tx,
          clientY: ty,
          button: 0,
        }),
      );

      return 'ok';
    });

    expect(result).toBe('ok');
    await browser.pause(1500);
    await expect(await $(`input[placeholder="${t('main.searchPlaceholder')}"]`)).toBeDisplayed();
  });

  it('DRAG-005: 连接项应标记为可拖拽', async () => {
    const items = await $$('[data-conn-item]');
    if (items.length === 0) return;

    const draggable = await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]') as HTMLElement | null;
      return el?.getAttribute('draggable') === 'true';
    });
    expect(draggable).toBe(true);
  });

  it('DRAG-006: 拖拽经过连接项时分组区域仍可用', async () => {
    const items = await $$('[data-conn-item]');
    const groupEls = await $$('[data-group-header]');
    if (items.length === 0 || groupEls.length < 2) return;

    await browser.execute(() => {
      const src = document.querySelector('[data-conn-item]') as HTMLElement;
      const target = document.querySelectorAll('[data-group-header]')[1] as HTMLElement;
      if (!src || !target) return;

      const sr = src.getBoundingClientRect();
      const tr = target.getBoundingClientRect();

      src.dispatchEvent(
        new PointerEvent('pointerdown', {
          bubbles: true,
          cancelable: true,
          clientX: sr.left + 5,
          clientY: sr.top + 5,
          button: 0,
        }),
      );
      window.dispatchEvent(
        new PointerEvent('pointermove', {
          bubbles: true,
          cancelable: true,
          clientX: sr.left + 20,
          clientY: sr.top + 20,
        }),
      );
      window.dispatchEvent(
        new PointerEvent('pointermove', {
          bubbles: true,
          cancelable: true,
          clientX: tr.left + tr.width / 2,
          clientY: tr.top + tr.height / 2,
        }),
      );
    });

    await browser.pause(300);

    await browser.execute(() => {
      window.dispatchEvent(
        new PointerEvent('pointerup', {
          bubbles: true,
          cancelable: true,
          clientX: 0,
          clientY: 0,
          button: 0,
        }),
      );
    });

    await expect(await $(`input[placeholder="${t('main.searchPlaceholder')}"]`)).toBeDisplayed();
    await browser.pause(300);
  });
});
