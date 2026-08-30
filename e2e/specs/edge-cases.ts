/**
 * Edge-case E2E (TC-EDGE-001/002/004/008).
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  expandAllGroups,
  openQueryTab,
  setEditorContent,
  waitForNewQueryButton,
} from '../helpers.js';
import { t } from '../i18n.js';

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

describe('边缘用例 (TC-EDGE-001/002/004/008)', () => {
  let mainWindow: string;
  const longName = `E2E-超长名称-${'名'.repeat(80)}`;
  const specialDbConnId = 'e2e-edge-special-sqlite';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  after(async () => {
    try {
      const conns = await invokeBackend<Array<{ id: string; name: string }>>('get_connections');
      for (const c of conns) {
        if (
          c.id === specialDbConnId ||
          c.name.startsWith('E2E-超长名称') ||
          c.name.startsWith('E2E-快速重复')
        ) {
          await invokeBackend('delete_connection', { id: c.id });
        }
      }
    } catch {
      /* ignore */
    }
    await closeExtraWindows(mainWindow);
  });

  it('TC-EDGE-001: 超长连接名称应可保存并在列表中可见（截断亦可）', async () => {
    await browser.switchToWindow(mainWindow);
    await invokeBackend('save_connection', {
      config: {
        id: 'e2e-edge-long-name',
        name: longName,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        group: 'E2E 测试',
        sslMode: 'disable',
      },
    });
    await browser.refresh();
    await browser.pause(1500);
    await expandAllGroups();
    const body = await $('body').getText();
    expect(body.includes('E2E-超长名称') || body.includes('名'.repeat(10))).toBe(true);
  });

  it('TC-EDGE-002: 特殊字符 SQLite 文件名连接应可保存', async () => {
    const fixture = `${process.cwd()}/e2e/fixtures/test.db`;
    await invokeBackend('save_connection', {
      config: {
        id: specialDbConnId,
        name: 'E2E-特殊字符_db-测试',
        databaseType: 'sqlite',
        host: '',
        port: 0,
        username: '',
        password: '',
        database: fixture,
        group: 'E2E 测试',
        sslMode: 'disable',
      },
    });
    const conns = await invokeBackend<Array<{ id: string; name: string }>>('get_connections');
    expect(conns.some((c) => c.id === specialDbConnId || c.name.includes('特殊字符'))).toBe(true);
  });

  it('TC-EDGE-004: 大结果集查询应返回结果或截断提示且不崩溃', async () => {
    await clickCardConnectButton();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
      timeout: 30000,
    });
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await waitForNewQueryButton(20000);
    await openQueryTab();
    await executeSQL('SELECT generate_series(1, 5000) AS n');
    await browser.pause(2000);
    const body = await $('body').getText();
    const ok =
      body.includes(t('common.rows')) ||
      body.includes('截断') ||
      body.includes(t('query.resultTruncated').split('{')[0]) ||
      body.includes('5000') ||
      body.includes(t('query.totalTime'));
    expect(ok).toBe(true);
    await closeExtraWindows(mainWindow);
  });

  it('TC-EDGE-008: 快速重复点击执行不应导致应用崩溃', async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await clickCardConnectButton();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
      timeout: 30000,
    });
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h && h !== mainWindow);
    expect(connWindow).toBeTruthy();
    await browser.switchToWindow(connWindow!);
    await waitForNewQueryButton(20000);
    await openQueryTab();
    await setEditorContent('SELECT 1 AS rapid');
    const execBtn = await $(`button*=${t('query.execute')}`);
    for (let i = 0; i < 5; i++) {
      await execBtn.click();
      await browser.pause(80);
    }
    await browser.pause(3000);
    await expect(await $(`button*=${t('query.execute')}`)).toBeDisplayed();
    await closeExtraWindows(mainWindow);
  });
});
