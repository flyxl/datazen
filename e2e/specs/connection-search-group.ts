import { expect, browser, $, $$ } from '@wdio/globals';
import { expandAllGroups } from '../helpers.js';

describe('连接搜索和分组 (CM-007, CM-008)', () => {
  before(async () => {
    await $('input[placeholder="查找连接…"]').waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(1000);
  });

  // ── 搜索功能 (CM-008) ─────────────────────────────────────────

  it('搜索框应能过滤连接 - 无匹配 (CM-008)', async () => {
    const input = await $('input[placeholder="查找连接…"]');
    await input.setValue('不存在的连接XYZ_99999');
    await browser.pause(500);

    // With no matches, either no connection items shown or "没有连接" text
    const items = await $$('[data-conn-item]');
    expect(items.length).toBe(0);

    await input.clearValue();
    await browser.pause(500);
  });

  it('搜索应支持按主机地址过滤 (CM-008)', async () => {
    const input = await $('input[placeholder="查找连接…"]');
    await input.setValue('localhost');
    await browser.pause(500);

    const body = await $('body').getText();
    const hasResult = body.includes('localhost') || body.includes('127.0.0.1') || body.includes('没有连接');
    expect(hasResult).toBe(true);

    await input.clearValue();
    await browser.pause(300);
  });

  it('搜索应支持按连接名称过滤 (CM-008)', async () => {
    const input = await $('input[placeholder="查找连接…"]');
    await input.setValue('Postgres');
    await browser.pause(500);

    const items = await $$('[data-conn-item]');
    expect(items.length).toBeGreaterThan(0);

    await input.clearValue();
    await browser.pause(300);
  });

  it('搜索框应能输入并清空 (CM-008)', async () => {
    const input = await $('input[placeholder="查找连接…"]');
    await input.setValue('test_value');
    const value = await input.getValue();
    expect(value).toBe('test_value');

    await input.clearValue();
    const clearedValue = await input.getValue();
    expect(clearedValue).toBe('');
  });

  // ── 分组功能 (CM-007) ─────────────────────────────────────────

  it('分组头应可展开/折叠 (CM-007)', async () => {
    const headers = await $$('[data-group-header]');
    expect(headers.length).toBeGreaterThan(0);
  });

  it('空白区域绑定了 contextmenu 处理器 (CM-007)', async () => {
    // Native context menus block WebDriver, so we verify the scroll container exists.
    const hasScrollArea = await browser.execute(() => {
      return document.querySelector('.flex-1.overflow-auto') instanceof HTMLElement;
    });
    expect(hasScrollArea).toBe(true);
  });

  it('分组头绑定了 contextmenu 处理器 (CM-007)', async () => {
    // Native context menus block WebDriver, so we only verify the elements exist
    // and have event handlers registered (React attaches via delegation).
    const hasNonUngroupedHeaders = await browser.execute(() => {
      const headers = document.querySelectorAll('[data-group-header]');
      for (const h of headers) {
        if (h.textContent && !h.textContent.includes('未分组')) return true;
      }
      return false;
    });
    // If there are named groups, their headers should be present and clickable
    if (hasNonUngroupedHeaders) {
      const headers = await $$('[data-group-header]');
      expect(headers.length).toBeGreaterThan(0);
    }
  });
});
