import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  injectDialogPath,
  resetDialogQueue,
  switchToNewWindow,
} from '../helpers.js';

/**
 * Backup window UI paths (BKU-001~BKU-006).
 * Opens backup via URL (same entry as cross-window routing). Full dump IPC: backup-database.ts.
 */

async function openBackupWindowUi(mainWindow: string) {
  await browser.url('tauri://localhost/window.html?window=backup');
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) {
    await switchToNewWindow(mainWindow);
  }
  await $('[data-testid="backup-group-header"]').waitForDisplayed({ timeout: 10000 });
}

describe('备份窗口 UI (BKU-001~BKU-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
  });

  after(async () => {
    try {
      await resetDialogQueue();
    } catch {
      /* ok */
    }
  });

  it('BKU-001: URL 应打开备份窗口', async () => {
    await openBackupWindowUi(mainWindow);
    const body = await $('body').getText();
    await expect($('[data-testid="welcome-page"]')).not.toBeExisting();
    await captureJourneyStep('backup-window-open');
  });

  it('备份窗口应显示连接搜索与文件名控件 (BKU-002)', async () => {
    await openBackupWindowUi(mainWindow);
    await expect(await $(`input[placeholder="${t('backup.searchConnection')}"]`)).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('backup.fileName'));
  });

  it('分组文案应与主窗口一致，不显示 preset: 键 (BKU-005)', async () => {
    await openBackupWindowUi(mainWindow);
    const body = await $('body').getText();
    expect(body).not.toContain('preset:');
  });

  it('未选连接时应提示先选择连接 (BKU-003)', async () => {
    await openBackupWindowUi(mainWindow);
    const body = await $('body').getText();
    expect(body).toContain(t('backup.selectConnectionFirst'));
  });

  it('应能搜索并选中已有连接 (BKU-004)', async () => {
    await openBackupWindowUi(mainWindow);
    const search = await $(`input[placeholder="${t('backup.searchConnection')}"]`);
    await search.setValue('PostgreSQL');
    await browser.pause(500);
    await browser.execute(() => {
      const candidates = Array.from(document.querySelectorAll('button, [role="option"], li, div'));
      const hit = candidates.find((el) => {
        const text = el.textContent || '';
        return text.includes('PostgreSQL') || text.includes('Postgres') || text.includes('本地');
      });
      (hit as HTMLElement | undefined)?.click();
    });
    await browser.pause(800);
    const body = await $('body').getText();
    expect(
      body.includes(t('backup.searchDatabase')) ||
        body.includes(t('backup.startBackup')) ||
        body.includes(t('backup.compressGzip')),
    ).toBe(true);
  });

  it('BKU-006: 开始备份应通过注入对话框落盘 (mock 原生另存为)', async () => {
    const pgDb = process.env.E2E_PG_DB || 'goecoride';
    await openBackupWindowUi(mainWindow);

    const search = await $(`input[placeholder="${t('backup.searchConnection')}"]`);
    await search.setValue('PostgreSQL');
    await browser.pause(500);
    const connRow = await $('[data-testid="backup-connection-row"]');
    await connRow.waitForDisplayed({ timeout: 10000 });
    await connRow.click();
    await browser.pause(300);

    await browser.execute((dbName: string) => {
      const nodes = Array.from(document.querySelectorAll('div'));
      const dbRow = nodes.find(
        (el) => el.textContent?.trim() === dbName && el.className.includes('cursor-pointer'),
      );
      (dbRow as HTMLElement | undefined)?.click();
    }, pgDb);
    await browser.pause(800);

    await resetDialogQueue();
    const outPath = path.join(os.tmpdir(), `datazen-backup-${Date.now()}.sql`);
    try {
      await injectDialogPath(outPath);
      const startBtn = await $('[data-testid="backup-start-backup"]');
      await startBtn.waitForClickable({ timeout: 15000 });
      await startBtn.click();
      await browser.waitUntil(() => fs.existsSync(outPath), {
        timeout: 60000,
        interval: 1000,
        timeoutMsg: `backup file not written: ${outPath}`,
      });
      expect(fs.statSync(outPath).size).toBeGreaterThan(0);
      await captureJourneyStep('backup-file-saved', 0, true);
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
    }
  });
});
