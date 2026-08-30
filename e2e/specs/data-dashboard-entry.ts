import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import {
  E2E_DASHBOARD_PREFIX,
  cleanupDashboard,
  createEmptyDashboard,
  openDashboardFromMain,
  openSettingsWindow,
} from '../helpers/data-dashboard.js';

const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-entry`;

describe('数据看板入口 (UJ-01, UJ-11)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(1000);
    await cleanupDashboard(DASHBOARD_ID);
    await createEmptyDashboard(DASHBOARD_ID, 'E2E Entry Board');
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await cleanupDashboard(DASHBOARD_ID);
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
  });

  it('UJ-01: 主窗口导航应打开嵌入看板面板（无 dashboard-dialog）', async () => {
    await openDashboardFromMain(mainWindow);

    const dialog = await $('[data-testid="dashboard-dialog"]');
    expect(await dialog.isExisting()).toBe(false);

    const root = await $('[data-testid="dashboard-panel"]');
    await expect(root).toBeDisplayed();
  });

  it('UJ-11: 设置窗口不应包含监控/Monitor 分区', async () => {
    await openSettingsWindow();

    const monitorNav = await $('[data-testid="settings-nav-monitor"]');
    expect(await monitorNav.isExisting()).toBe(false);

    const body = await $('body').getText();
    expect(body.includes('监控') || body.includes('Monitor')).toBe(false);
  });
});
