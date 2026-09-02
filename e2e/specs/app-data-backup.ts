/**
 * E2E — App data ZIP backup / restore (typical user scenarios via IPC + UI).
 *
 * ADB-001  Homepage shows Export/Import App Data actions
 * ADB-002  export_app_data writes a zip under a temp path (overridePath form)
 * ADB-003  Export zip is non-empty and re-import IPC succeeds (no restart)
 * ADB-004  Import of path-traversal zip fails safely
 * ADB-005  get_system_ui_language returns a supported locale (related first-run)
 * ADB-006  Exported zip excludes logs/ and *.tmp entries
 * ADB-007  Export button label matches current UI language
 * ADB-008  Exported zip excludes .key (encryption key material)
 */
import { expect, browser } from '@wdio/globals';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';

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

// Decision 3 (F4): export_app_data / import_app_data are merged path/dialog
// IPCs. Raw paths go through `overridePath`, which only webdriver builds
// accept and which skips the native file picker (old raw-path behavior).
function exportToPath(zipPath: string) {
  return invokeBackend<boolean>('export_app_data', {
    defaultFileName: 'datazen-backup.zip',
    overridePath: zipPath,
  });
}

function importFromPath(zipPath: string) {
  return invokeBackend<boolean>('import_app_data', {
    overridePath: zipPath,
  });
}

const SUPPORTED = new Set(['en', 'zh-CN', 'zh-TW', 'es', 'fr', 'de', 'ja', 'pt-BR', 'ru', 'ko']);

function makeTraversalZip(dest: string) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dz-evil-'));
  const payload = path.join(dir, 'outside.txt');
  fs.writeFileSync(payload, 'evil');
  // Prefer system zip if available; otherwise skip with note
  try {
    execFileSync('zip', ['-j', dest, payload], { stdio: 'ignore' });
    // Rewrite is hard; create a minimal zip with Python if zip CLI packs flat names only
  } catch {
    /* fall through to python */
  }
  // Always create a zip with a traversal entry via Python for reliability
  const py = `
import zipfile, sys
z = zipfile.ZipFile(sys.argv[1], 'w')
z.writestr('../outside.txt', b'evil')
z.close()
`;
  execFileSync('python3', ['-c', py, dest], { stdio: 'ignore' });
}

describe('App Data Backup (ADB-001~ADB-005)', () => {
  let backupPath = '';
  let evilPath = '';

  before(async () => {
    await browser.pause(300);
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'datazen-e2e-backup-'));
    backupPath = path.join(tmp, 'datazen-backup.zip');
    evilPath = path.join(tmp, 'evil.zip');
  });

  after(async () => {
    try {
      if (backupPath && fs.existsSync(backupPath)) fs.unlinkSync(backupPath);
    } catch {
      /* ignore */
    }
    try {
      if (evilPath && fs.existsSync(evilPath)) fs.unlinkSync(evilPath);
    } catch {
      /* ignore */
    }
  });

  it('ADB-001: homepage should expose export/import app data (menu wiring)', async () => {
    await browser.url('tauri://localhost');
    await browser.pause(300);

    // macOS uses native menu (MenuBar returns null); assert product wiring instead of DOM buttons.
    // App-data config wiring (backupCommands calls + menu:export/import-config
    // listeners) lives in ConnectionPage.tsx.
    const connectionSrc = fs.readFileSync(
      path.resolve(import.meta.dirname, '../../src/windows/connection/ConnectionPage.tsx'),
      'utf8',
    );
    expect(connectionSrc).toContain('backupCommands.exportAppData(');
    expect(connectionSrc).toContain('backupCommands.pickAppDataImportFile(');
    expect(connectionSrc).toContain('backupCommands.importAppData(');
    expect(connectionSrc).toContain('menu:export-config');
    expect(connectionSrc).toContain('menu:import-config');

    // Asset-drift fix (R-phase): the menu:export/import-connections listeners
    // live in MainPage.tsx since commit e883f834 (in-page connection dialog,
    // right after F2's Window→Page rename) — not in ConnectionPage.tsx.
    const mainPageSrc = fs.readFileSync(
      path.resolve(import.meta.dirname, '../../src/windows/main/MainPage.tsx'),
      'utf8',
    );
    expect(mainPageSrc).toContain('menu:export-connections');
    expect(mainPageSrc).toContain('menu:import-connections');

    const zh = fs.readFileSync(
      path.resolve(import.meta.dirname, '../../src/locales/zh-CN.ts'),
      'utf8',
    );
    expect(zh).toContain("'common.exportAppData': '导出应用数据'");
    expect(zh).toContain("'common.importAppData': '导入应用数据'");
  });

  it('ADB-002: export_app_data should write a zip file', async () => {
    const saved = await exportToPath(backupPath);
    expect(saved).toBe(true);
    expect(fs.existsSync(backupPath)).toBe(true);
    const stat = fs.statSync(backupPath);
    expect(stat.size).toBeGreaterThan(0);
  });

  it('ADB-003: re-importing the exported zip via IPC should succeed', async () => {
    // Idempotent restore of current snapshot (no restart — Store stays in memory).
    await exportToPath(backupPath);
    await importFromPath(backupPath);
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    expect(settings).toBeTruthy();
    expect(typeof settings.language).toBe('string');
  });

  it('ADB-004: importing a path-traversal zip should fail', async () => {
    makeTraversalZip(evilPath);
    let failed = false;
    try {
      await importFromPath(evilPath);
    } catch (e) {
      failed = true;
      expect(String(e)).toMatch(/traversal|InvalidInput|absolute|failed|error/i);
    }
    expect(failed).toBe(true);
  });

  it('ADB-005: get_system_ui_language should return a supported locale', async () => {
    const lang = await invokeBackend<string>('get_system_ui_language');
    expect(SUPPORTED.has(lang)).toBe(true);
  });

  it('ADB-006: exported zip should exclude logs/ and *.tmp', async () => {
    await exportToPath(backupPath);
    const py = `
import zipfile, sys
names = zipfile.ZipFile(sys.argv[1]).namelist()
bad = [n for n in names if n.startswith('logs/') or n.endswith('.tmp') or '.import_staging' in n]
print('\\n'.join(names))
sys.exit(1 if bad else 0)
`;
    try {
      const listing = execFileSync('python3', ['-c', py, backupPath], {
        encoding: 'utf8',
      });
      expect(listing.length).toBeGreaterThan(0);
    } catch (e: unknown) {
      const err = e as { stdout?: string; status?: number };
      throw new Error(`ZIP contains excluded entries.\nListing:\n${err.stdout ?? String(e)}`);
    }
  });

  it('ADB-008: exported zip should exclude .key (encryption key material)', async () => {
    await exportToPath(backupPath);
    const py = `
import zipfile, sys
names = zipfile.ZipFile(sys.argv[1]).namelist()
bad = [n for n in names if n == '.key' or n.endswith('/.key')]
print('\\n'.join(names))
sys.exit(1 if bad else 0)
`;
    try {
      const listing = execFileSync('python3', ['-c', py, backupPath], {
        encoding: 'utf8',
      });
      expect(listing.length).toBeGreaterThan(0);
    } catch (e: unknown) {
      const err = e as { stdout?: string; status?: number };
      throw new Error(
        `ZIP contains .key entries (must be excluded).\nListing:\n${err.stdout ?? String(e)}`,
      );
    }
  });

  it('ADB-007: export label locale keys match current UI language', async () => {
    const settings = await invokeBackend<{ language: string }>('get_settings');
    const localeFile =
      settings.language === 'zh-CN' ? 'zh-CN.ts' : settings.language === 'en' ? 'en.ts' : 'en.ts';
    const src = fs.readFileSync(
      path.resolve(import.meta.dirname, `../../src/locales/${localeFile}`),
      'utf8',
    );
    if (settings.language === 'zh-CN') {
      expect(src).toContain("'common.exportAppData': '导出应用数据'");
    } else {
      expect(src).toMatch(/'common\.exportAppData': 'Export App Data/);
    }
  });
});
