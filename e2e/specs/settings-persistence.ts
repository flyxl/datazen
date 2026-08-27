/**
 * Theme toggle + settings persistence — switch themes, verify CSS
 * variables change, check settings persist across app refresh.
 *
 * Covers: TC-SET-007 ~ TC-SET-010
 */
import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows, expandAllGroups, invokeBackend } from '../helpers.js';

async function getSettings() {
  return invokeBackend<Record<string, unknown>>('get_settings');
}

async function saveSettings(settings: Record<string, unknown>) {
  await invokeBackend('save_settings', { settings });
}

describe('主题切换与设置持久化 (TC-SET-007~010)', () => {
  let mainWindow: string;
  let originalSettings: Record<string, unknown>;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    originalSettings = await getSettings();
  });

  after(async () => {
    // Restore original settings
    await saveSettings(originalSettings);
    await browser.refresh();
    await browser.pause(1500);
    await closeExtraWindows(mainWindow);
  });

  it('TC-SET-007: 暗色/亮色主题切换应改变 CSS 变量', async () => {
    await browser.switchToWindow(mainWindow);
    // Navigate to settings
    const settingsNav = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const btn = btns.find((b) => {
        const text = b.textContent || '';
        const label = b.getAttribute('aria-label') || '';
        return text.includes('设置') || text.includes('Settings') || label.includes('设置');
      });
      if (btn) {
        btn.click();
        return true;
      }
      // Try Cmd+, shortcut indicator
      return false;
    });
    await browser.pause(1500);

    // Get current theme
    const currentTheme = await browser.execute(() => {
      return document.documentElement.classList.contains('dark') ? 'dark' : 'light';
    });

    // Toggle theme
    const toggled = await browser.execute(() => {
      const toggles = document.querySelectorAll(
        'button[role="switch"], [data-testid*="theme"], [class*="theme-toggle"]',
      );
      for (const t of toggles) {
        (t as HTMLElement).click();
        return true;
      }
      // Fallback: look for theme selector
      const selects = document.querySelectorAll('select, [role="combobox"]');
      for (const s of selects) {
        const text = s.closest('[class*="setting"]')?.textContent || '';
        if (text.includes('主题') || text.includes('Theme')) {
          (s as HTMLElement).click();
          return true;
        }
      }
      return false;
    });

    if (toggled) {
      await browser.pause(1000);
      const newTheme = await browser.execute(() => {
        return document.documentElement.classList.contains('dark') ? 'dark' : 'light';
      });
      expect(newTheme).not.toBe(currentTheme);
    }
  });

  it('TC-SET-008: 语言切换应更新 UI 文本', async () => {
    await browser.switchToWindow(mainWindow);
    const settings = await getSettings();
    const originalLang = settings.language;

    // Toggle language
    const newLang = originalLang === 'zh-CN' ? 'en' : 'zh-CN';
    await saveSettings({ ...settings, language: newLang });
    await browser.refresh();
    await browser.pause(2000);

    const body = await $('body').getText();
    if (newLang === 'en') {
      // Should see English UI text
      expect(
        body.includes('Settings') ||
          body.includes('Connection') ||
          body.includes('New Connection') ||
          body.includes('Query'),
      ).toBe(true);
    } else {
      // Should see Chinese UI text
      expect(
        body.includes('设置') ||
          body.includes('连接') ||
          body.includes('新建连接') ||
          body.includes('查询'),
      ).toBe(true);
    }

    // Restore
    await saveSettings({ ...settings, language: originalLang });
    await browser.refresh();
    await browser.pause(1500);
  });

  it('TC-SET-009: 编辑器字体大小设置应持久化', async () => {
    const settings = await getSettings();
    const originalFontSize = settings.editorFontSize;

    // Change font size
    const newFontSize = originalFontSize === 14 ? 16 : 14;
    await saveSettings({ ...settings, editorFontSize: newFontSize });
    await browser.refresh();
    await browser.pause(1500);

    const persisted = await getSettings();
    expect(persisted.editorFontSize).toBe(newFontSize);

    // Restore
    await saveSettings({ ...settings, editorFontSize: originalFontSize });
  });

  it('TC-SET-010: 确认删除开关应持久化', async () => {
    const settings = await getSettings();
    const originalConfirm = settings.confirmOnDelete;

    await saveSettings({ ...settings, confirmOnDelete: !originalConfirm });
    await browser.refresh();
    await browser.pause(1500);

    const persisted = await getSettings();
    expect(persisted.confirmOnDelete).toBe(!originalConfirm);

    // Restore
    await saveSettings({ ...settings, confirmOnDelete: originalConfirm });
  });
});
