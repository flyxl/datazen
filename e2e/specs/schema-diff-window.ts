import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { t } from '../i18n.js';
import {
  advanceSchemaDiffToPlan,
  captureJourneyStep,
  clickSchemaDiffCompare,
  clickSchemaDiffNext,
  closeExtraWindows,
  invokeBackend,
  openSchemaDiffWindow,
  selectDzOptionInWrap,
  selectSchemaDiffEndpoints,
  stubClipboardCapture,
  readStubbedClipboard,
  restoreClipboardStub,
  waitForSchemaDiffNextEnabled,
  E2E_PG_CONN_NAME,
} from '../helpers.js';
import { seedSecondPgConnection } from '../lib/testDataLifecycle.js';

/**
 * Schema Diff window shell + primary controls (SD-001~SD-004).
 */

describe('结构对比窗口 (SD-001~SD-004, SD-LIM)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-001: 应能通过 URL 打开结构对比窗口', async () => {
    await openSchemaDiffWindow();
    await expect(await $('[data-testid="schema-diff-window"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('common.schemaDiff'));
    expect(body).toContain(t('schemaDiff.step.endpoints'));
    expect(body).toContain(t('schemaDiff.step.objects'));
    await captureJourneyStep('schema-diff-window-open');
  });

  it('SD-002: 应显示向导步骤与下一步导航', async () => {
    await openSchemaDiffWindow();
    await expect(await $('[data-testid="schema-diff-next"]')).toBeDisplayed();
    await expect(await $('[data-testid="schema-diff-step-compare"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.step.plan'));
  });

  it('SD-003: 未选表点下一步应提示必填', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints('本地 PostgreSQL', 'E2E-PG-目标');
    await clickSchemaDiffNext();
    const objectsPanel = await $('[data-testid="schema-diff-objects-panel"]');
    await objectsPanel.waitForDisplayed({ timeout: 15000 });
    await browser.waitUntil(
      async () => (await $$('[data-testid="schema-diff-table-row"]').length) > 0,
      { timeout: 20000, timeoutMsg: '等待表列表加载' },
    );
    const rows = await $$('[data-testid="schema-diff-table-row"]');
    for (const row of rows) {
      const checkbox = await row.$('input[type="checkbox"]');
      if (await checkbox.isSelected()) {
        await checkbox.click();
      }
    }
    await clickSchemaDiffNext({ requireEnabled: false });
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.tableRequired'));
    await captureJourneyStep('schema-diff-table-required');
  });

  it('SD-004: 应通过弹窗显示当前版本限制说明', async () => {
    await openSchemaDiffWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="schema-diff-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });
    const panel = await dialog.$('[data-testid="schema-diff-limitations"]');
    await panel.waitForDisplayed({ timeout: 8000 });
    expect(await panel.getText()).toContain(t('schemaDiff.limitations.noViews'));
    await expect(await $('[data-testid="schema-diff-window"]')).toBeDisplayed();
  });

  it('SD-LIM-001: 勾选「不再显示」后再次打开不应弹出限制说明', async () => {
    await openSchemaDiffWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="schema-diff-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });

    const dismiss = await $('[data-testid="schema-diff-limitations-dismiss"]');
    await dismiss.click();
    const closeBtn = await $('[data-testid="schema-diff-limitations-close"]');
    await closeBtn.click();
    await browser.waitUntil(async () => !(await dialog.isDisplayed().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: '等待限制说明弹窗关闭超时',
    });

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await openSchemaDiffWindow({ dismissLimitations: false, clearLimitationsPref: false });

    const dialogAgain = await $('[data-testid="schema-diff-limitations-dialog"]');
    expect(await dialogAgain.isExisting().catch(() => false)).toBe(false);
  });

  it('SD-005: 同一连接不同数据库应允许进入下一步', async () => {
    await openSchemaDiffWindow();
    await selectDzOptionInWrap('schema-diff-source', E2E_PG_CONN_NAME);
    await selectDzOptionInWrap('schema-diff-target', E2E_PG_CONN_NAME);
    await selectDzOptionInWrap('schema-diff-source-database', 'datazen_sync_src');
    await selectDzOptionInWrap('schema-diff-target-database', 'datazen_sync_tgt');
    await waitForSchemaDiffNextEnabled();
    await captureJourneyStep('schema-diff-same-conn-diff-db');
  });

  it('SD-006: 进入计划步骤应自动生成部署脚本', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(E2E_PG_CONN_NAME, 'E2E-PG-目标');
    await clickSchemaDiffNext();
    await clickSchemaDiffCompare();
    await advanceSchemaDiffToPlan();
    await expect(await $('[data-testid="schema-diff-allow-destructive"]')).toBeDisplayed();
    await captureJourneyStep('schema-diff-auto-plan');
  });

  it('SD-007: 复制 SQL 应显示已复制反馈', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(E2E_PG_CONN_NAME, 'E2E-PG-目标');
    await clickSchemaDiffNext();
    await clickSchemaDiffCompare();
    await advanceSchemaDiffToPlan();
    await stubClipboardCapture();
    try {
      const copyBtn = await $('[data-testid="schema-diff-copy-sql"]');
      await copyBtn.waitForClickable({ timeout: 15000 });
      await copyBtn.click();
      await browser.waitUntil(async () => (await copyBtn.getText()).includes(t('common.copied')), {
        timeout: 5000,
        timeoutMsg: '等待复制 SQL 反馈超时',
      });
      expect(await readStubbedClipboard()).not.toBe('');
    } finally {
      await restoreClipboardStub();
    }
    await captureJourneyStep('schema-diff-copy-sql');
  });

  it('SD-008: 导入配置应打开对话框而非系统粘贴菜单', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(E2E_PG_CONN_NAME, 'E2E-PG-目标');
    await clickSchemaDiffNext();
    await clickSchemaDiffCompare();
    await advanceSchemaDiffToPlan();

    const importBtn = await $('[data-testid="schema-diff-import-config"]');
    await importBtn.waitForClickable({ timeout: 15000 });
    await importBtn.click();
    const dialog = await $('[data-testid="schema-diff-import-config-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });
    await expect(await $('[data-testid="schema-diff-import-config-text"]')).toBeDisplayed();
    const contextMenu = await $('[data-testid="web-context-menu"]');
    expect(await contextMenu.isExisting().catch(() => false)).toBe(false);
    await captureJourneyStep('schema-diff-import-dialog');
  });

  it('SD-009: 导出配置应通过保存对话框写入 JSON 文件', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(E2E_PG_CONN_NAME, 'E2E-PG-目标');
    await clickSchemaDiffNext();
    await clickSchemaDiffCompare();
    await advanceSchemaDiffToPlan();

    const outPath = path.join(os.tmpdir(), `datazen-sd-config-${Date.now()}.json`);
    try {
      await invokeBackend('test_inject_dialog_result', { result: { path: outPath } });
      const exportBtn = await $('[data-testid="schema-diff-export-config"]');
      await exportBtn.waitForClickable({ timeout: 15000 });
      await exportBtn.click();
      await browser.waitUntil(
        async () => (await exportBtn.getText()).includes(t('schemaDiff.configExported')),
        { timeout: 8000, timeoutMsg: '等待导出配置成功反馈超时' },
      );
      expect(fs.existsSync(outPath)).toBe(true);
      const raw = fs.readFileSync(outPath, 'utf-8');
      const parsed = JSON.parse(raw) as { version?: number };
      expect(parsed.version).toBe(2);
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
      await invokeBackend('test_reset_dialog_queue');
    }
    await captureJourneyStep('schema-diff-export-config');
  });
});
