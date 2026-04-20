import { expect, browser, $, $$ } from '@wdio/globals';
import { switchToNewWindow, closeExtraWindows, findCardByName } from '../helpers.js';

const TEST_CONN_NAME = 'E2E-编辑删除测试';

describe('编辑、复制和删除连接 (CM-003, CM-004, CM-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();

    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);
    const saveBtn = await $('button*=保存');
    await saveBtn.waitForDisplayed({ timeout: 5000 });

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue(TEST_CONN_NAME);
    await saveBtn.click();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000 },
    );
    await browser.switchToWindow(mainWindow);
    const card = await $(`div*=${TEST_CONN_NAME}`);
    await card.waitForDisplayed({ timeout: 5000 });
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
  });

  // ── 编辑 (CM-003) ─────────────────────────────────────────────

  it('连接卡片应显示编辑、复制、删除按钮', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    expect(card).not.toBeNull();
    await expect(await card!.$('button[title="编辑连接"]')).toBeDisplayed();
    await expect(await card!.$('button[title="复制连接"]')).toBeDisplayed();
    await expect(await card!.$('button[title="删除连接"]')).toBeDisplayed();
  });

  it('点击编辑按钮应打开编辑窗口并预填名称 (CM-003)', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    const editBtn = await card!.$('button[title="编辑连接"]');
    await editBtn.click();
    await switchToNewWindow(mainWindow);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(
      async () => (await nameInput.getValue()) === TEST_CONN_NAME,
      { timeout: 8000, timeoutMsg: '编辑表单预填超时' },
    );
    expect(await nameInput.getValue()).toBe(TEST_CONN_NAME);
  });

  it('编辑窗口标题应显示"编辑连接" (CM-003)', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    const editBtn = await card!.$('button[title="编辑连接"]');
    await editBtn.click();
    await switchToNewWindow(mainWindow);

    const titleEl = await $('span*=编辑连接');
    await titleEl.waitForDisplayed({ timeout: 5000 });
    await expect(titleEl).toBeDisplayed();
  });

  it('编辑连接名称后保存应更新主窗口卡片 (CM-003)', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    const editBtn = await card!.$('button[title="编辑连接"]');
    await editBtn.click();
    await switchToNewWindow(mainWindow);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(
      async () => (await nameInput.getValue()) === TEST_CONN_NAME,
      { timeout: 8000 },
    );

    const newName = 'E2E-已编辑连接';
    await nameInput.clearValue();
    await nameInput.setValue(newName);
    const saveBtn = await $('button*=保存');
    await saveBtn.click();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000 },
    );
    await browser.switchToWindow(mainWindow);
    const updatedCard = await $(`div*=${newName}`);
    await updatedCard.waitForDisplayed({ timeout: 5000 });

    // Rename back
    const card2 = await findCardByName(newName);
    const editBtn2 = await card2!.$('button[title="编辑连接"]');
    await editBtn2.click();
    await switchToNewWindow(mainWindow);
    const inp2 = await $('input[placeholder="例如：主数据库"]');
    await inp2.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(async () => (await inp2.getValue()) === newName, { timeout: 8000 });
    await inp2.clearValue();
    await inp2.setValue(TEST_CONN_NAME);
    const saveBtn2 = await $('button*=保存');
    await saveBtn2.click();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1, { timeout: 10000 });
    await browser.switchToWindow(mainWindow);
    const restoredCard = await $(`div*=${TEST_CONN_NAME}`);
    await restoredCard.waitForDisplayed({ timeout: 5000 });
  });

  it('编辑连接窗口应有高级设置按钮 (CM-007)', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    const editBtn = await card!.$('button[title="编辑连接"]');
    await editBtn.click();
    await switchToNewWindow(mainWindow);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(async () => (await nameInput.getValue()) === TEST_CONN_NAME, { timeout: 8000 });

    // Verify advanced settings button exists
    const advBtn = await $('button*=高级设置');
    await expect(advBtn).toBeDisplayed();
  });

  // ── 复制 (CM-006) ─────────────────────────────────────────────

  it('复制连接后应出现副本卡片 (CM-006)', async () => {
    const card = await findCardByName(TEST_CONN_NAME);
    const copyBtn = await card!.$('button[title="复制连接"]');
    await copyBtn.click();

    const copyName = `${TEST_CONN_NAME} (副本)`;
    await browser.waitUntil(
      async () => (await findCardByName(copyName)) !== null,
      { timeout: 5000, timeoutMsg: '等待复制的连接卡片出现超时' },
    );
    const copyCard = await findCardByName(copyName);
    expect(copyCard).not.toBeNull();

    // Clean up: delete the copy
    const delBtn = await copyCard!.$('button[title="删除连接"]');
    await delBtn.click();
    await browser.waitUntil(async () => (await findCardByName(copyName)) === null, { timeout: 5000 });
  });

  // ── 删除 (CM-004) ─────────────────────────────────────────────

  it('删除连接后卡片应从主窗口消失 (CM-004)', async () => {
    const cardBefore = await findCardByName(TEST_CONN_NAME);
    expect(cardBefore).not.toBeNull();

    const delBtn = await cardBefore!.$('button[title="删除连接"]');
    await delBtn.click();
    await browser.waitUntil(
      async () => (await findCardByName(TEST_CONN_NAME)) === null,
      { timeout: 5000 },
    );
    expect(await findCardByName(TEST_CONN_NAME)).toBeNull();
  });
});
