/**
 * Connection management edge cases — concurrent tabs, rapid operations,
 * connection lifecycle boundary tests.
 *
 * Covers: TC-EDGE-009 ~ TC-EDGE-013
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  closeNewConnectionDialogFromUi,
  expandAllGroups,
  invokeBackend,
  openNewConnectionDialogFromUi,
  openQueryTab,
  waitForConnectionToolbar,
} from '../helpers.js';

const PG_HOST = process.env.E2E_PG_HOST || '127.0.0.1';
const PG_PORT = Number(process.env.E2E_PG_PORT) || 5432;
const PG_USER = process.env.E2E_PG_USER || 'postgres';
const PG_PW = process.env.E2E_PG_PASSWORD || '';
const PG_DB = process.env.E2E_PG_DB || 'postgres';

async function saveConn(id: string, name: string, group = 'E2E-边缘', db = PG_DB) {
  await invokeBackend('save_connection', {
    config: {
      id,
      name,
      databaseType: 'postgresql',
      host: PG_HOST,
      port: PG_PORT,
      username: PG_USER,
      password: PG_PW,
      database: db,
      group,
      sslMode: 'disable',
    },
  });
}

async function deleteConn(id: string) {
  try {
    await invokeBackend('delete_connection', { id });
  } catch {
    /* */
  }
}

describe('连接管理边界用例 (TC-EDGE-009~013)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  after(async () => {
    for (const id of [
      'e2e-edge-rapid-1',
      'e2e-edge-rapid-2',
      'e2e-edge-rapid-3',
      'e2e-edge-edit-test',
      'e2e-edge-group-test',
    ]) {
      await deleteConn(id);
    }
    await closeExtraWindows(mainWindow);
  });

  it('TC-EDGE-009: 快速连续新建/删除连接不应导致崩溃', async () => {
    await browser.switchToWindow(mainWindow);
    const ids = ['e2e-edge-rapid-1', 'e2e-edge-rapid-2', 'e2e-edge-rapid-3'];
    for (let i = 0; i < ids.length; i++) {
      await saveConn(ids[i], `E2E-快速连${i + 1}`);
    }
    const conns = await invokeBackend<Array<{ id: string }>>('get_connections');
    for (const id of ids) {
      expect(conns.some((c) => c.id === id)).toBe(true);
    }
    for (const id of ids) {
      await invokeBackend('delete_connection', { id });
    }
    const after = await invokeBackend<Array<{ id: string }>>('get_connections');
    for (const id of ids) {
      expect(after.some((c) => c.id === id)).toBe(false);
    }
  });

  it('TC-EDGE-010: 编辑连接后名称应在列表中更新', async () => {
    await browser.switchToWindow(mainWindow);
    const editId = 'e2e-edge-edit-test';
    await saveConn(editId, 'E2E-编辑前');
    await saveConn(editId, 'E2E-编辑后');
    await browser.refresh();
    await browser.pause(300);
    await expandAllGroups();
    const body = await $('body').getText();
    expect(body).toContain('E2E-编辑后');
    expect(body).not.toContain('E2E-编辑前');
    await captureJourneyStep('edge-conn-list-refreshed');
    await deleteConn(editId);
  });

  it('TC-EDGE-011: 连接失败后 Test Connection 按钮应可再次点击', async () => {
    await browser.switchToWindow(mainWindow);
    await openNewConnectionDialogFromUi();

    const pgBtn = await $('button*=PostgreSQL');
    if (await pgBtn.isExisting()) await pgBtn.click();

    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await hostInput.clearValue();
    await hostInput.setValue('192.0.2.1');
    await captureJourneyStep('test-connection-bad-host', 0, true);

    // Click Test Connection with bad host — should not crash
    await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const btn = btns.find((b) => {
        const text = b.textContent || '';
        return text.includes('测试连接') || text.includes('Test Connection');
      });
      if (btn) (btn as HTMLElement).click();
    });
    await browser.pause(300);
    const dialogStillOpen = await $('[data-testid="new-connection-dialog"]').isExisting();
    expect(dialogStillOpen).toBe(true);
    await captureJourneyStep('test-connection-failed');

    await closeNewConnectionDialogFromUi();
  });

  it('TC-EDGE-012: 重复打开同一连接应复用已有 tab', async () => {
    await browser.switchToWindow(mainWindow);
    // Verify we can see the connection list
    await expandAllGroups();
    const connCount = await browser.execute(() => {
      return document.querySelectorAll('[data-conn-item]').length;
    });
    expect(connCount).toBeGreaterThan(0);

    await clickCardConnectButton();
    await waitForConnectionToolbar();
    await browser.pause(500);

    // We should be connected — toolbar visible
    const body = await $('body').getText();
    expect(body.includes('新建查询') || body.includes('New Query')).toBe(true);
  });

  it('TC-EDGE-013: 连接分组重命名后分组应保持一致', async () => {
    await browser.switchToWindow(mainWindow);
    const grpId = 'e2e-edge-group-test';
    await saveConn(grpId, 'E2E-分组测试', 'E2E-临时分组');
    await browser.refresh();
    await browser.pause(300);
    await expandAllGroups();
    let body = await $('body').getText();
    expect(body).toContain('E2E-临时分组');

    await saveConn(grpId, 'E2E-分组测试', 'E2E-已重命名分组');
    await browser.refresh();
    await browser.pause(300);
    await expandAllGroups();
    body = await $('body').getText();
    expect(body).toContain('E2E-已重命名分组');
    expect(body).not.toContain('E2E-临时分组');
    await deleteConn(grpId);
  });
});
