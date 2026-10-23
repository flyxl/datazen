/**
 * E2E: 查询历史按 database 分组（next-move 测试计划 TS-QH-E01~E05）
 *
 * 走通：执行两条 SQL → 打开历史侧栏 → 默认「当前库」作用域（无分组头）→
 * 切换「全部」出现按库分组头 → 搜索过滤与作用域叠加 → 清空按钮存在。
 * 依赖 wdio.conf.ts 种下的 `本地 PostgreSQL`。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  executeSQL,
  openQueryTab,
  openSeededPgConnectionWindow,
} from '../helpers.js';

const MARKER_A = 'SELECT 123 AS qh_marker_a';
const MARKER_B = 'SELECT 456 AS qh_marker_b';

async function openHistorySidebar() {
  // The toolbar collapses to icon-only on narrow panels, so the label lives in
  // `title` / `aria-label` instead of the button text — match the stable test id.
  const btn = await $('[data-testid="editor-history-toggle"]');
  await btn.waitForClickable({ timeout: 10000 });
  await btn.click();
  await browser.pause(500);
}

async function scopeButtonPressed(testid: string): Promise<boolean> {
  const btn = await $(`[data-testid="${testid}"]`);
  if (!(await btn.isExisting())) return false;
  return (await btn.getAttribute('aria-pressed')) === 'true';
}

describe('查询历史 database 分组 (QH)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // A previous spec can leave a backend session for the seeded connection
    // bound to its (now dropped) worker database; openSeededPgConnectionWindow
    // would reuse that session, the marker records would land under a
    // database that never matches the UI's selected database, and the history
    // "current" scope would permanently fall back to grouped headers (QH-002).
    // Kill any leaked session first so the UI connect creates a fresh one on
    // this run's worker database.
    try {
      const leaked = await connectBackend('conn_e2e_pg');
      if (leaked) await disconnectBackend(leaked);
    } catch {
      /* no leaked session */
    }
    await openSeededPgConnectionWindow(mainWindow);
    await openQueryTab();
    // 产生两条带当前库上下文的历史记录
    await executeSQL(MARKER_A);
    await executeSQL(MARKER_B);
    await openHistorySidebar();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('QH-001: 历史侧栏提供 当前库/全部 作用域切换，默认当前库', async () => {
    const cur = await $('[data-testid="history-scope-current"]');
    const all = await $('[data-testid="history-scope-all"]');
    await expect(cur).toBeDisplayed();
    await expect(all).toBeDisplayed();
    expect(await scopeButtonPressed('history-scope-current')).toBe(true);
    expect(await scopeButtonPressed('history-scope-all')).toBe(false);
  });

  it('QH-002: 当前库作用域可见刚执行的记录，且无分组头', async () => {
    // Wait for the observable resolved state: the scope fallback (which
    // renders group headers + a hint inside the "current" scope) must be gone
    // before counting headers — counting against a still-resolving sidebar
    // races the history/database context update.
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="history-scope-fallback-hint"]')
          .isExisting()
          .catch(() => false)),
      {
        timeout: 10000,
        timeoutMsg:
          'history scope fallback still active — marker records did not resolve to the selected database',
      },
    );
    const body = await $('body').getText();
    expect(body).toContain('qh_marker_a');
    expect(body).toContain('qh_marker_b');
    const headers = await $$('[data-testid="history-group-label"]');
    expect(headers.length).toBe(0);
  });

  it('QH-003: 切换「全部」后出现按库分组头，记录仍可见', async () => {
    await $('[data-testid="history-scope-all"]').click();
    await browser.pause(300);
    const headers = await $$('[data-testid="history-group-label"]');
    expect(headers.length).toBeGreaterThanOrEqual(1);
    const body = await $('body').getText();
    expect(body).toContain('qh_marker_a');
  });

  it('QH-004: 搜索过滤在全部作用域下生效，无命中显示空态', async () => {
    const search = await $(`input[aria-label="${t('query.searchHistory')}"]`);
    await search.setValue('qh_marker_b');
    await browser.pause(300);
    let body = await $('body').getText();
    expect(body).toContain('qh_marker_b');
    expect(body).not.toContain('qh_marker_a');

    await search.setValue('__no_such_marker__');
    await browser.pause(300);
    body = await $('body').getText();
    expect(body).toContain(t('query.noHistoryMatch'));
    await search.setValue('');
  });

  it('QH-005: 切回当前库作用域仍正常显示', async () => {
    await $('[data-testid="history-scope-current"]').click();
    await browser.pause(300);
    expect(await scopeButtonPressed('history-scope-current')).toBe(true);
    const body = await $('body').getText();
    expect(body).toContain('qh_marker_a');
    expect(body).toContain(t('query.database'));
  });
});
