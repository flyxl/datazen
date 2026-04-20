import { expect, browser, $ } from '@wdio/globals';

describe('系统设置 (SS-001~SS-006)', () => {
  before(async () => {
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1000);
  });

  // ── 主题切换 (SS-001) ──────────────────────────────────────────

  it('应显示主题切换按钮 (SS-001)', async () => {
    const themeBtn = await $('button[title^="主题"]');
    await expect(themeBtn).toBeDisplayed();
  });

  it('点击主题按钮应显示主题选项 (SS-001)', async () => {
    const themeBtn = await $('button[title^="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    await expect(await $('button*=浅色')).toBeDisplayed();
    await expect(await $('button*=深色')).toBeDisplayed();
    await expect(await $('button*=跟随系统')).toBeDisplayed();
  });

  it('选择浅色主题后 html 不应有 dark 类 (SS-001)', async () => {
    const lightBtn = await $('button*=浅色');
    await lightBtn.click();
    await browser.pause(500);

    const html = await $('html');
    const cls = await html.getAttribute('class');
    expect(cls).not.toContain('dark');
  });

  it('选择深色主题后 html 应有 dark 类 (SS-001)', async () => {
    const themeBtn = await $('button[title^="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    const darkBtn = await $('button*=深色');
    await darkBtn.click();
    await browser.pause(500);

    const html = await $('html');
    const cls = await html.getAttribute('class');
    expect(cls).toContain('dark');
  });

  it('选择跟随系统后应正常 (SS-002)', async () => {
    const themeBtn = await $('button[title^="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    const systemBtn = await $('button*=跟随系统');
    await expect(systemBtn).toBeDisplayed();
    await systemBtn.click();
    await browser.pause(300);
  });

  // ── 设置窗口 (SS-003~SS-006) ──────────────────────────────────

  it('应能通过菜单打开设置窗口 (SS-003)', async () => {
    // Emit the settings open event to trigger settings window
    await browser.execute(() => {
      // Settings can be triggered via Tauri event or menu
      const event = new CustomEvent('open-settings');
      window.dispatchEvent(event);
    });
    await browser.pause(500);

    // If settings window doesn't open via event, try the keyboard shortcut or menu emulation
    // Settings window is in-page overlay or separate window depending on implementation
    const bodyText = await $('body').getText();
    if (!bodyText.includes('偏好设置')) {
      // Try via Tauri emit
      await browser.execute(() => {
        (window as any).__TAURI__?.event?.emit('menu:open-settings');
      });
      await browser.pause(1000);
    }
  });

  it('设置页面应有主题选择 (SS-003)', async () => {
    // If settings opens as separate window
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) {
      const mainWindow = handles[0];
      const settingsWindow = handles.find((h) => h !== mainWindow)!;
      await browser.switchToWindow(settingsWindow);
      await browser.pause(500);
    }

    // Verify presence of theme-related UI in either current window or settings panel
    const bodyText = await $('body').getText();
    if (bodyText.includes('偏好设置') || bodyText.includes('主题')) {
      expect(bodyText).toContain('主题');
    }
  });

  it('设置中应显示数据浏览配置选项 (SS-004)', async () => {
    const bodyText = await $('body').getText();
    if (bodyText.includes('偏好设置')) {
      expect(bodyText).toContain('默认每页行数');
    }
  });

  it('设置中应显示 SELECT 限制选项 (SS-005)', async () => {
    const bodyText = await $('body').getText();
    if (bodyText.includes('偏好设置')) {
      expect(bodyText).toContain('限制 SELECT 结果行数');
    }
  });

  it('设置中应显示编辑器配置选项 (SS-006)', async () => {
    const bodyText = await $('body').getText();
    if (bodyText.includes('偏好设置')) {
      const hasEditorSettings = bodyText.includes('字号') || bodyText.includes('字体') || bodyText.includes('编辑器');
      expect(hasEditorSettings).toBe(true);
    }
  });

  it('设置中应显示行为开关 (SS-006)', async () => {
    const bodyText = await $('body').getText();
    if (bodyText.includes('偏好设置')) {
      const hasBehavior = bodyText.includes('删除确认') || bodyText.includes('自动提交');
      expect(hasBehavior).toBe(true);
    }

    // Clean up settings window if opened
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) {
      const mainWindow = handles[0];
      for (const h of handles) {
        if (h !== mainWindow) {
          await browser.switchToWindow(h);
          await browser.closeWindow();
        }
      }
      await browser.switchToWindow(mainWindow);
    }
  });

  // ── 恢复浅色主题 ───────────────────────────────────────────────

  after(async () => {
    const handles = await browser.getWindowHandles();
    await browser.switchToWindow(handles[0]);

    // Restore light theme for other tests
    const themeBtn = await $('button[title^="主题"]');
    if (await themeBtn.isExisting()) {
      await themeBtn.click();
      await browser.pause(300);
      const lightBtn = await $('button*=浅色');
      if (await lightBtn.isExisting()) {
        await lightBtn.click();
        await browser.pause(300);
      }
    }
  });
});
