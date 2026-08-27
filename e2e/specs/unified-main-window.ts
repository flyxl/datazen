import { $, browser } from '@wdio/globals';
import { expect } from '@wdio/globals';
import { switchWorkspaceNav } from '../helpers.js';
import { t } from '../i18n.js';

describe('统一主窗口回归', () => {
  it('应显示窄图标栏，并支持切换工作区', async () => {
    const connectionsNav = await $('[data-testid="workspace-nav-connections"]');
    await connectionsNav.waitForDisplayed({ timeout: 15000 });
    await expect(connectionsNav).toBeDisplayed();

    const workflowNav = await $('[data-testid="workspace-nav-workflow"]');
    await workflowNav.waitForDisplayed({ timeout: 15000 });
    await expect(workflowNav).toBeDisplayed();

    const dashboardNav = await $('[data-testid="workspace-nav-dashboard"]');
    await dashboardNav.waitForDisplayed({ timeout: 15000 });
    await expect(dashboardNav).toBeDisplayed();

    const search = await $(`input[placeholder="${t('main.searchPlaceholder')}"]`);
    await search.waitForDisplayed({ timeout: 15000 });

    await switchWorkspaceNav('workspace-nav-workflow', 'workflow-workspace', 'workspace-workflow');
    const workflowWorkspace = await $('[data-testid="workflow-workspace"]');
    await expect(workflowWorkspace).toBeDisplayed();

    await switchWorkspaceNav('workspace-nav-dashboard', 'dashboard-panel', 'workspace-dashboard');
    const dashboardPanel = await $('[data-testid="dashboard-panel"]');
    await expect(dashboardPanel).toBeDisplayed();

    const size = await browser.getWindowSize();
    expect(size.width).toBeGreaterThan(1000);
    expect(size.height).toBeGreaterThan(700);
  });

  it('未打开 panel 时应显示工作区首页', async () => {
    await switchWorkspaceNav(
      'workspace-nav-connections',
      'connection-workspace-home',
      'workspace-connections-home',
    );
    const home = await $('[data-testid="connection-workspace-home"]');
    await expect(home).toBeDisplayed();
  });

  it('启动完成后不应再显示 splash logo', async () => {
    const visibleSplashLogo = await browser.execute(() => {
      const img = document.querySelector<HTMLImageElement>('#splash img.logo');
      if (!img) return false;
      const splash = img.closest('#splash');
      if (!(splash instanceof HTMLElement)) return false;
      const style = window.getComputedStyle(splash);
      return (
        style.display !== 'none' && style.opacity !== '0' && !splash.classList.contains('hide')
      );
    });
    expect(visibleSplashLogo).toBe(false);
  });
});
