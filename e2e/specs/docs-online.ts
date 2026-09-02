/**
 * F6 — Online help docs wiring (source + IPC; no in-app Docs sub-window).
 *
 * Note: Tauri 2 freezes `__TAURI_INTERNALS__.invoke` (non-configurable), so E2E
 * cannot spy/replace invoke. Wiring is asserted via source reads; behavior via
 * direct `open_path` IPC and window-count checks after triggering Help UI.
 *
 * DOCS-001  windowManager uses buildDocsUrl + openPath; no docs-singleton
 * DOCS-002  docsUrls.ts official GitHub Pages bases + section hashes
 * DOCS-003  App.tsx / windowKind: no DocsWindow; legacy ?window=docs → main
 * DOCS-004  open_path IPC accepts https docs URLs (EN + ZH + section)
 * DOCS-005  MenuBar help-docs → openDocsWindow (source)
 * DOCS-006  Help menu click does not spawn a Tauri sub-window
 * DOCS-007  Legacy window.html?window=docs loads main shell (negative)
 */
import fs from 'node:fs';
import path from 'node:path';
import { expect, browser, $ } from '@wdio/globals';
import { connectSeededPgInWorkspace, openQueryTab } from '../helpers.js';
import { t } from '../i18n.js';

const ROOT = path.resolve(import.meta.dirname, '../..');
const WINDOW_MANAGER = path.join(ROOT, 'src/lib/windowManager.ts');
const DOCS_URLS = path.join(ROOT, 'src/lib/docsUrls.ts');
const SETTINGS_CMD = path.join(ROOT, 'src/commands/settings.ts');
const APP_TSX = path.join(ROOT, 'src/App.tsx');
const WINDOW_KIND = path.join(ROOT, 'src/lib/windowKind.ts');
const MENU_BAR = path.join(ROOT, 'src/components/MenuBar.tsx');
const RUST_WINDOW = path.join(ROOT, 'src-tauri/src/commands/window.rs');

const DOCS_BASE_EN = 'https://flyxl.github.io/datazen/manual.html';
const DOCS_BASE_ZH = 'https://flyxl.github.io/datazen/zh/manual.html';

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

describe('Online Help Docs (DOCS-001~DOCS-007)', () => {
  let mainWindow = '';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  it('DOCS-001: openDocsWindow uses buildDocsUrl + openPath; no docs-singleton', () => {
    const wm = fs.readFileSync(WINDOW_MANAGER, 'utf8');
    expect(wm).toContain('buildDocsUrl');
    expect(wm).toContain('settingsCommands.openPath');
    expect(wm).not.toContain("openSingletonWindow('docs-singleton'");
    const openDocsBlock = wm.slice(wm.indexOf('export function openDocsWindow'));
    expect(openDocsBlock).not.toContain('create_sub_window');

    const settings = fs.readFileSync(SETTINGS_CMD, 'utf8');
    expect(settings).toContain("invoke<void>('open_path', { path })");
  });

  it('DOCS-002: docsUrls.ts defines manual.html bases and legacy section remap', () => {
    const src = fs.readFileSync(DOCS_URLS, 'utf8');
    expect(src).toContain(DOCS_BASE_EN);
    expect(src).toContain(DOCS_BASE_ZH);
    for (const section of [
      'overview',
      'features',
      'ai',
      'context',
      'workflows',
      'opsDashboard',
      'schemaDiff',
    ]) {
      expect(src).toContain(`${section}:`);
    }

    const rust = fs.readFileSync(RUST_WINDOW, 'utf8');
    expect(rust).toContain(DOCS_BASE_EN);
    expect(rust).toContain('open::that(&url)');
    expect(rust).not.toMatch(/create_sub_window[\s\S]*docs-singleton/);
  });

  it('DOCS-003: App.tsx has no DocsWindow; windowKind aliases ?window=docs → main', () => {
    const app = fs.readFileSync(APP_TSX, 'utf8');
    expect(app).not.toContain('DocsWindow');
    expect(app).not.toMatch(/case\s+['"]docs['"]/);

    const kind = fs.readFileSync(WINDOW_KIND, 'utf8');
    expect(kind).toContain("'docs'");
    expect(kind).toContain('LEGACY_MAIN_ALIASES');
  });

  it('DOCS-004: open_path IPC accepts manual.html URLs (EN, ZH, section hash)', async () => {
    await invokeBackend('open_path', { path: DOCS_BASE_EN });
    await invokeBackend('open_path', { path: DOCS_BASE_ZH });
    await invokeBackend('open_path', { path: `${DOCS_BASE_EN}#workflow` });
    await invokeBackend('open_path', { path: `${DOCS_BASE_ZH}#ai` });
  });

  it('DOCS-005: MenuBar help-docs wires openDocsWindow (source)', () => {
    const menu = fs.readFileSync(MENU_BAR, 'utf8');
    expect(menu).toContain("'help-docs'");
    expect(menu).toContain('openDocsWindow()');
    expect(menu).not.toContain('window.html?window=docs');
  });

  it('DOCS-006: Help menu click does not create a Tauri sub-window', async function () {
    this.timeout(20000);

    const handlesBefore = await browser.getWindowHandles();
    expect(handlesBefore.length).toBe(1);

    await connectSeededPgInWorkspace();
    await openQueryTab();
    const helpBtn = await $(`button[title="${t('docs.openAiHelp')}"]`);
    await helpBtn.waitForDisplayed({ timeout: 10000 });
    await helpBtn.click();
    await browser.pause(800);

    const handlesAfter = await browser.getWindowHandles();
    expect(handlesAfter.length).toBe(handlesBefore.length);
  });

  it('DOCS-007: legacy ?window=docs URL resolves to main shell (no Docs sidebar)', async function () {
    this.timeout(20000);

    await browser.url('tauri://localhost/window.html?window=docs&section=workflows');
    await browser.pause(1500);

    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);

    const hasDocsSidebar = await browser.execute(() => {
      const body = document.body.innerText || '';
      return body.includes('Getting Started') && body.includes('Workflows Guide');
    });
    expect(hasDocsSidebar).toBe(false);

    const hasMainWorkspace = await browser.execute(() => {
      return (
        document.querySelector('[data-testid="workspace-nav-connections"]') !== null ||
        document.querySelector('[data-testid="settings-page"]') !== null ||
        document.querySelector('[data-testid="welcome-page"]') !== null
      );
    });
    expect(hasMainWorkspace).toBe(true);

    await browser.url('tauri://localhost');
    await browser.pause(1000);
    mainWindow = await browser.getWindowHandle();
  });
});
