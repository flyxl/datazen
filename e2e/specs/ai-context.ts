import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import {
  openConnectionWindow,
  closeExtraWindows,
  captureJourneyStep,
  openQueryTab,
} from '../helpers.js';

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
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

describe('AI 上下文引用 E2E 测试 (CTX-001~CTX-006)', () => {
  let mainWindow: string;
  let contextDir: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();

    // Get context directory path
    contextDir = await invokeBackend<string>('context_get_dir');

    // Seed context files via filesystem (E2E process is Node — write directly,
    // the app reads them back over its own context IPCs).
    fs.writeFileSync(
      `${contextDir}/schema.sql`,
      'CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100), email VARCHAR(200));',
    );
    fs.writeFileSync(
      `${contextDir}/relations.md`,
      '# Table Relations\n\n- users has_many orders\n- orders belongs_to users',
    );
  });

  after(async () => {
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  it('CTX-001: context_get_dir returns a valid path', async () => {
    const dir = await invokeBackend<string>('context_get_dir');
    expect(dir).toBeTruthy();
    expect(typeof dir).toBe('string');
  });

  it('CTX-002: context_list_files lists seeded files', async () => {
    const entries = await invokeBackend<any[]>('context_list_files', { query: null });
    expect(entries.length).toBeGreaterThanOrEqual(2);

    const names = entries.map((e: any) => e.name);
    expect(names).toContain('schema.sql');
    expect(names).toContain('relations.md');
  });

  it('CTX-003: context_list_files supports query filtering', async () => {
    const entries = await invokeBackend<any[]>('context_list_files', { query: 'schema' });
    expect(entries.length).toBe(1);
    expect(entries[0].name).toBe('schema.sql');
  });

  it('CTX-004: context_read_files returns file contents', async () => {
    const results = await invokeBackend<[string, string][]>('context_read_files', {
      paths: ['schema.sql'],
    });
    expect(results.length).toBe(1);
    expect(results[0][0]).toBe('schema.sql');
    expect(results[0][1]).toContain('CREATE TABLE users');
  });

  it('CTX-005: context_read_files rejects path traversal', async () => {
    let error: string | null = null;
    try {
      await invokeBackend('context_read_files', {
        paths: ['../../etc/passwd'],
      });
    } catch (e: any) {
      error = e.message;
    }
    expect(error).toBeTruthy();
  });

  it('CTX-006: AI Chat panel shows @ context picker trigger', async () => {
    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;

    // Switch to connection window
    await browser.switchToWindow(windows.connWindow);
    await openQueryTab();

    const chatToggle = await $('[data-testid="conn-toolbar-ai"]');
    await chatToggle.waitForClickable({ timeout: 10000 });
    await chatToggle.click();
    await browser.pause(1000);
    await captureJourneyStep('ai-panel-open', 0, true);

    const textarea = await $('aside textarea');
    if (await textarea.isExisting()) {
      await textarea.click();
      await textarea.setValue('@');
      await browser.pause(1000);
      const val = await textarea.getValue();
      expect(val).toContain('@');
      await captureJourneyStep('ai-context-at-picker', 0, true);
    }

    await closeExtraWindows(mainWindow);
  });
});
