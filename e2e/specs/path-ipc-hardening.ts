/**
 * E2E — Path IPC hardening (webdriver builds keep path APIs; UI uses dialog / open_*).
 *
 * Note: Tauri 2 freezes `__TAURI_INTERNALS__.invoke` (non-configurable), so E2E
 * cannot spy/replace invoke. UI wiring is asserted via source + visible controls;
 * behavior is covered by direct IPC (PIH-001~003) which is available under webdriver.
 *
 * PIH-001  open_log_dir / open_workflows_dir / open_context_dir succeed
 * PIH-002  open_path allows app-data paths; rejects outside paths
 * PIH-003  export_connections + import_connections_preview round-trip
 * PIH-004  Settings logging uses open_log_dir (source + button visible)
 * PIH-005  Settings AI context uses open_context_dir (source + button visible)
 * PIH-006  ADB pull uses dialog command (source + ADB UI has no path field)
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';

const ROOT = path.resolve(import.meta.dirname, '../..');
const FILE_CONNECTION_FIELDS = path.join(
  ROOT,
  'src/components/connection/FileConnectionFields.tsx',
);
const SETTINGS_TS = path.join(ROOT, 'src/commands/settings.ts');
const SETTINGS_WIN = path.join(ROOT, 'src/windows/settings/SettingsWindow.tsx');
const AI_SETTINGS = path.join(ROOT, 'src/windows/settings/AiSettingsSection.tsx');
const ADB_TS = path.join(ROOT, 'src/commands/adb.ts');

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function invokeBackendCatch(cmd: string, args: Record<string, unknown> = {}): Promise<string | null> {
  try {
    await invokeBackend(cmd, args);
    return null;
  } catch (e) {
    return String(e);
  }
}

describe('Path IPC Hardening (PIH-001~PIH-006)', () => {
  let mainWindow = '';
  let tmpDir = '';
  let exportPath = '';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'datazen-pih-'));
    exportPath = path.join(tmpDir, 'connections-export.json');
  });

  after(async () => {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
      try {
        await browser.switchToWindow(mainWindow);
      } catch {
        /* ignore */
      }
    }
  });

  it('PIH-001: dedicated open_* dir commands should succeed', async () => {
    await invokeBackend('open_log_dir');
    await invokeBackend('open_workflows_dir');
    await invokeBackend('open_context_dir');
  });

  it('PIH-002: open_path allows log dir and rejects outside paths', async () => {
    const logPath = await invokeBackend<string>('get_log_path');
    expect(logPath.length).toBeGreaterThan(0);

    await invokeBackend('open_path', { path: logPath });

    const outsideErr = await invokeBackendCatch('open_path', { path: '/etc' });
    expect(outsideErr).toBeTruthy();
    expect(outsideErr!).toMatch(/only allows|disabled|Path traversal|Cannot resolve/i);

    const traversalErr = await invokeBackendCatch('open_path', {
      path: `${logPath}/../../etc/passwd`,
    });
    expect(traversalErr).toBeTruthy();
  });

  it('PIH-003: export_connections + import_connections_preview round-trip', async () => {
    const count = await invokeBackend<number>('export_connections', {
      path: exportPath,
      password: 'e2e-pih-password',
    });
    expect(typeof count).toBe('number');
    expect(fs.existsSync(exportPath)).toBe(true);
    const raw = fs.readFileSync(exportPath, 'utf8');
    expect(raw).toContain('"connections"');
    expect(raw).toContain('"encrypted"');

    const preview = await invokeBackend<{
      connections: unknown[];
    }>('import_connections_preview', {
      path: exportPath,
      password: 'e2e-pih-password',
    });
    expect(Array.isArray(preview.connections)).toBe(true);
  });

  it('PIH-004: Settings logging button wires open_log_dir', async function () {
    this.timeout(20000);

    const winSrc = fs.readFileSync(SETTINGS_WIN, 'utf8');
    expect(winSrc).toContain('openLogDir()');
    expect(winSrc).not.toMatch(/openPath\(\s*(localDir|dir|defaultDir|draft)/);

    const cmdSrc = fs.readFileSync(SETTINGS_TS, 'utf8');
    expect(cmdSrc).toContain("invoke<void>('open_log_dir')");

    await browser.url('tauri://localhost/window.html?window=settings&section=logging');
    await browser.pause(1500);

    const viewLogs = await $('button*=查看日志');
    await viewLogs.waitForDisplayed({ timeout: 10000 });

    // Click exercises the handler; command itself is covered by PIH-001.
    await viewLogs.click();
    await browser.pause(500);
  });

  it('PIH-005: Settings AI context button wires open_context_dir', async function () {
    this.timeout(20000);

    const winSrc = fs.readFileSync(AI_SETTINGS, 'utf8');
    expect(winSrc).toContain('openContextDir()');

    const cmdSrc = fs.readFileSync(SETTINGS_TS, 'utf8');
    expect(cmdSrc).toContain("invoke<void>('open_context_dir')");

    await browser.url('tauri://localhost/window.html?window=settings&section=ai');
    await browser.pause(1500);

    const openCtx = await $('button*=打开上下文目录');
    await openCtx.waitForDisplayed({ timeout: 10000 });
    await openCtx.click();
    await browser.pause(500);
  });

  it('PIH-006: ADB pull uses dialog command (no local path field)', async function () {
    this.timeout(45000);

    const fieldsSrc = fs.readFileSync(FILE_CONNECTION_FIELDS, 'utf8');
    expect(fieldsSrc).toContain('adbPullDatabaseWithDialog');
    expect(fieldsSrc).not.toMatch(/\badbPullDatabase\s*\(/);
    expect(fieldsSrc).not.toContain('localSavePath');
    expect(fieldsSrc).not.toContain('adbLocalPath');

    const adbSrc = fs.readFileSync(ADB_TS, 'utf8');
    expect(adbSrc).toContain('adb_pull_database_with_dialog');
    expect(adbSrc).toContain('adbPullDatabaseWithDialog');

    await browser.url('tauri://localhost');
    await browser.pause(1000);
    mainWindow = await browser.getWindowHandle();

    const newBtn = await $('button*=新建连接');
    await newBtn.click();
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 15000, timeoutMsg: 'new-connection window did not open' },
    );
    const handles = await browser.getWindowHandles();
    const connWin = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWin);
    await browser.pause(800);

    await (await $('button*=SQLite')).click();
    await browser.pause(400);

    await browser.execute(() => {
      const labels = Array.from(document.querySelectorAll('label'));
      const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
      const checkbox = adbLabel?.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
      checkbox?.click();
    });
    await browser.pause(800);

    // ADB panel should not offer a local path PathInput (dialog-only save).
    const hasLocalPathLabel = await browser.execute(() => {
      const body = document.body.innerText || '';
      return body.includes('本地保存路径') || body.includes('Local save path');
    });
    expect(hasLocalPathLabel).toBe(false);

    await expect(await $('label*=从 Android 设备拉取')).toBeDisplayed();
  });
});
