import { expect, browser } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * Dialog-injection infrastructure sample (R regression proxy).
 *
 * Progress-file anchor: F3-E2E-003 「原生保存对话框取消 → 返回 false 且不写文件」
 * was previously marked 【留待 R 阶段回归】with a manual-black-box exception,
 * because WebdriverIO cannot drive the real native dialog. The webdriver-only
 * dialog-injection surface (`test_inject_dialog_result` /
 * `test_reset_dialog_queue`, see src-tauri/src/commands/dialog.rs) removes that
 * blocker: the spec pre-queues a dialog answer, triggers the command WITHOUT
 * `overridePath` — i.e. through the REAL dialog branch — and asserts the
 * cancel feedback end-to-end.
 *
 * These cases are the executable template for the remaining dialog-cancel
 * cases registered in docs/development/ipc-refactor-progress.md (their rewrite
 * is handled uniformly by the R regression agent).
 */

// ── Helpers ─────────────────────────────────────────────────────────

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

const TMP_DIR = os.tmpdir();

// ═════════════════════════════════════════════════════════════════════
// Group 1: injected dialog results replace the native dialog branch
// ═════════════════════════════════════════════════════════════════════

describe('对话框注入基建 (DIALOG-INJECTION)', () => {
  before(async () => {
    await browser.setTimeout({ script: 60000 });
    // Case isolation: start every spec run with an empty injection queue.
    await invokeBackend('test_reset_dialog_queue');
  });

  after(async () => {
    try {
      await invokeBackend('test_reset_dialog_queue');
    } catch {
      /* ok */
    }
  });

  it('DI-001: injected canceled → backup_database (dialog branch, no overridePath) returns false', async () => {
    // Queue the cancel answer, then trigger the merged backup command the
    // way PRODUCTION does — no overridePath, so the native save dialog branch
    // runs and must consume the injected answer instead of blocking E2E.
    await invokeBackend('test_inject_dialog_result', { result: { canceled: true } });

    const saved = await invokeBackend<boolean>('backup_database', {
      // Cancel happens BEFORE any session lookup (dialog-first ordering),
      // so a placeholder session id proves the dialog branch was taken.
      dbSessionId: 'e2e-dialog-injection-placeholder',
      defaultFileName: 'datazen-dialog-cancel-sample.sql',
      filterExtension: 'sql',
      options: [],
      compress: false,
    });

    // Cancel feedback contract (F3 decision 2): user dismissed → false.
    expect(saved).toBe(false);
  });

  it('DI-002: injected path → save_text_with_dialog writes the file at the injected location', async () => {
    const outPath = path.join(TMP_DIR, `datazen-inject-${Date.now()}.csv`);
    try {
      await invokeBackend('test_inject_dialog_result', { result: { path: outPath } });

      const saved = await invokeBackend<boolean>('save_text_with_dialog', {
        contents: 'id,name\n1,injected\n',
        defaultFileName: 'ignored-by-injection.csv',
        filterName: 'CSV',
        extensions: ['csv'],
      });

      expect(saved).toBe(true);
      expect(fs.readFileSync(outPath, 'utf-8')).toBe('id,name\n1,injected\n');
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
    }
  });

  it('DI-003: queued answers are consumed FIFO across both shapes; reset clears leftovers', async () => {
    await invokeBackend('test_inject_dialog_result', { result: { canceled: true } });
    const outPath = path.join(TMP_DIR, `datazen-inject-fifo-${Date.now()}.csv`);
    await invokeBackend('test_inject_dialog_result', { result: { path: outPath } });

    try {
      // First dialog request consumes `canceled` → null session handle.
      const cancelledToken = await invokeBackend<string | null>('begin_save_with_dialog', {
        defaultFileName: 'first.csv',
        filterName: 'CSV',
        extensions: ['csv'],
      });
      expect(cancelledToken).toBe(null);

      // Second request consumes `path` → a live save session is created.
      const token = await invokeBackend<string>('begin_save_with_dialog', {
        defaultFileName: 'second.csv',
        filterName: 'CSV',
        extensions: ['csv'],
      });
      expect(typeof token).toBe('string');
      await invokeBackend('abort_save', { token });
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
    }

    // Isolation primitive: clearing the (now empty) queue must succeed and
    // leave nothing behind for subsequent tests.
    await invokeBackend('test_reset_dialog_queue');
  });

  it('DI-004: malformed injection payload is rejected loudly instead of silently queuing', async () => {
    await expect(
      invokeBackend('test_inject_dialog_result', { result: {} }),
    ).rejects.toThrow(/canceled/);
    await expect(
      invokeBackend('test_inject_dialog_result', { result: { canceled: false } }),
    ).rejects.toThrow(/canceled/);
  });
});
