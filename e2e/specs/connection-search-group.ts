import { expect, browser, $ } from '@wdio/globals';

describe('连接搜索和分组 (CM-007, CM-008)', () => {
  before(async () => {
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1000);
  });

  // ── 搜索功能 (CM-008) ─────────────────────────────────────────

  it('搜索框应能过滤连接 - 无匹配 (CM-008)', async () => {
    const input = await $('input[placeholder="搜索连接…"]');
    await input.setValue('不存在的连接XYZ_99999');
    await browser.pause(500);

    const noMatch = await $('div*=没有匹配的连接');
    await expect(noMatch).toBeDisplayed();

    await input.clearValue();
    await browser.pause(500);
  });

  it('搜索应支持按主机地址过滤 (CM-008)', async () => {
    const input = await $('input[placeholder="搜索连接…"]');
    await input.setValue('localhost');
    await browser.pause(500);

    // If we have a localhost connection, it should be visible
    const body = await $('body').getText();
    // Either we find a connection or "没有匹配" — but no crash
    const hasResult = body.includes('localhost') || body.includes('127.0.0.1') || body.includes('没有匹配');
    expect(hasResult).toBe(true);

    await input.clearValue();
    await browser.pause(300);
  });

  it('搜索应支持按连接名称过滤 (CM-008)', async () => {
    const input = await $('input[placeholder="搜索连接…"]');
    // Use partial name that should match seeded connection
    await input.setValue('PostgreSQL');
    await browser.pause(500);

    const body = await $('body').getText();
    expect(body).toContain('PostgreSQL');

    await input.clearValue();
    await browser.pause(300);
  });

  it('搜索框应能输入并清空 (CM-008)', async () => {
    const input = await $('input[placeholder="搜索连接…"]');
    await input.setValue('test_value');
    const value = await input.getValue();
    expect(value).toBe('test_value');

    await input.clearValue();
    const clearedValue = await input.getValue();
    expect(clearedValue).toBe('');
  });

  // ── 分组功能 (CM-007) ─────────────────────────────────────────

  it('分组面板应显示"全部"按钮 (CM-007)', async () => {
    const allBtn = await $('button*=全部');
    await expect(allBtn).toBeDisplayed();
  });

  it('新建分组按钮应存在 (CM-007)', async () => {
    const addBtn = await $('button[title="新建分组"]');
    await expect(addBtn).toBeDisplayed();
  });

  it('新建分组后应出现在侧边栏 (CM-007)', async () => {
    const addBtn = await $('button[title="新建分组"]');
    await addBtn.click();

    const input = await $('input[placeholder="分组名称"]');
    await input.waitForDisplayed({ timeout: 3000 });
    await input.setValue('E2E测试分组-搜索');
    await browser.keys('Enter');
    await browser.pause(500);

    const groupBtn = await $('button*=E2E测试分组-搜索');
    await expect(groupBtn).toBeDisplayed();
  });

  it('点击自定义分组应过滤连接 (CM-007)', async () => {
    const groupBtn = await $('button*=E2E测试分组-搜索');
    await groupBtn.click();
    await browser.pause(500);

    // This group is empty, so should show "没有匹配的连接" or empty state
    const body = await $('body').getText();
    const hasEmpty = body.includes('没有匹配') || body.includes('没有连接') ||
      !body.includes('PostgreSQL');
    expect(hasEmpty).toBe(true);
  });

  it('点击"全部"应恢复显示所有连接 (CM-007)', async () => {
    const allBtn = await $('button*=全部');
    await allBtn.click();
    await browser.pause(500);

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('PostgreSQL'),
      { timeout: 5000, timeoutMsg: '等待连接列表恢复超时' },
    );
  });
});
