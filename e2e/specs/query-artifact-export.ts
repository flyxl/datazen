import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  closeExtraWindows, connectSeededPgInWorkspace, executeSQL, injectDialogCanceled,
  injectDialogPath, openQueryTab, resetDialogQueue, setEditorContent,
} from '../helpers.js';

// Read only our synthetic result file; assertions return booleans and never
// echo artifact contents, provenance, error messages or connection identities.
function hasSyntheticRow(file: string, value: number): boolean {
  return fs.readFileSync(file, 'utf8').trim().split('\n').some((line) => {
    const record = JSON.parse(line) as { rows?: unknown[][]; output?: { rows?: unknown[][] } };
    return [record.rows, record.output?.rows].some((rows) =>
      Array.isArray(rows) && rows.some((row) => Array.isArray(row) && row[0] === value));
  });
}

describe('Published query result artifact export', function () {
  this.timeout(120000);
  it('exports the retained execution through the real Host dialog IPC and resets on the next run', async () => {
    const mainWindow = await browser.getWindowHandle();
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'datazen-artifact-export-'));
    const first = path.join(directory, 'first.jsonl');
    const next = path.join(directory, 'next.jsonl');
    let stage = 'connect';
    try {
      await resetDialogQueue();
      await connectSeededPgInWorkspace();
      await openQueryTab();
      stage = 'query';
      await executeSQL('SELECT 73 AS artifact_export_probe');
      const button = await $('[data-testid="query-artifact-export-button"]');
      await button.waitForDisplayed({ timeout: 20000 });
      stage = 'cancel-save';
      await injectDialogCanceled();
      await button.click();
      await button.waitForEnabled({ timeout: 10000 });
      expect(await $('[data-testid="query-artifact-export-saved"]').isExisting()).toBe(false);
      expect(await $('[data-testid="query-artifact-export-error"]').isExisting()).toBe(false);
      expect(fs.existsSync(first)).toBe(false);
      stage = 'save-retry';
      await injectDialogPath(first);
      await button.click();
      await $('[data-testid="query-artifact-export-saved"]').waitForDisplayed({ timeout: 20000 });
      expect(fs.existsSync(first)).toBe(true);
      expect(hasSyntheticRow(first, 73)).toBe(true);
      stage = 'new-execution';
      await setEditorContent('SELECT 75 AS artifact_export_probe FROM pg_sleep(2)');
      await $('[data-testid="editor-execute-button"]').click();
      await browser.waitUntil(async () => !(await $('[data-testid="query-artifact-export"]').isExisting()), {
        timeout: 1500, timeoutMsg: 'Previous artifact export remained available during the next run',
      });
      await $('[data-testid="query-artifact-export-button"]').waitForDisplayed({ timeout: 20000 });
      stage = 'save-new-execution';
      await injectDialogPath(next);
      await $('[data-testid="query-artifact-export-button"]').click();
      await $('[data-testid="query-artifact-export-saved"]').waitForDisplayed({ timeout: 20000 });
      expect(hasSyntheticRow(next, 75)).toBe(true);
      expect(hasSyntheticRow(next, 73)).toBe(false);
    } catch (error) {
      const category = error instanceof Error && ['Error', 'AssertionError', 'TimeoutError', 'WebDriverError'].includes(error.name)
        ? error.name : 'Error';
      throw new Error(`Published result export failed at ${stage} (${category})`);
    } finally {
      try { await resetDialogQueue(); }
      finally {
        fs.rmSync(directory, { recursive: true, force: true });
        await closeExtraWindows(mainWindow);
      }
    }
  });
});
