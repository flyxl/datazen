/**
 * PostgreSQL multi-database session UI (F4).
 *
 * Covers:
 *   - Connect without database → schema tree lists multiple DB nodes
 *   - Expand a DB → tables load under that node
 *   - QueryPanel DB selector visible when multiple DBs
 *
 * Credentials: E2E_PG_* (see e2e/.env.example). Skips gracefully when
 * PostgreSQL is unreachable or E2E_SKIP_PG=1.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { createConnection } from 'node:net';
import { t } from '../i18n.js';
import {
  createAndConnectPostgreSQL,
  closeExtraWindows,
  openQueryTab,
} from '../helpers.js';

const CONN_NAME = 'E2E-PostgreSQL-MultiDb';
const PG_HOST = process.env.E2E_PG_HOST || '127.0.0.1';
const PG_PORT = Number(process.env.E2E_PG_PORT) || 5432;
const EXPECTED_DB = process.env.E2E_PG_DB || 'postgres';

function skipRequested(): boolean {
  return process.env.E2E_SKIP_PG === '1';
}

async function postgresReachable(timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host: PG_HOST, port: PG_PORT });
    const timer = setTimeout(() => {
      sock.destroy();
      resolve(false);
    }, timeoutMs);
    sock.on('connect', () => {
      clearTimeout(timer);
      sock.destroy();
      resolve(true);
    });
    sock.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

/** Collect sidebar button labels that look like database nodes (not table rows). */
async function listSidebarDbNames(): Promise<string[]> {
  const names = await browser.execute(() => {
    const out: string[] = [];
    document.querySelectorAll('aside button').forEach((btn) => {
      const cls = btn.getAttribute('class') || '';
      // DB nodes use text-sm + teal Database icon; tables use text-[13px] + pl-8
      if (!cls.includes('text-sm') || cls.includes('pl-8') || cls.includes('text-[13px]')) return;
      const text = (btn.textContent || '').trim();
      if (text) out.push(text);
    });
    return out;
  });
  return names;
}

async function clickSidebarDb(dbName: string) {
  const clicked = await browser.execute((name: string) => {
    const buttons = Array.from(document.querySelectorAll('aside button'));
    for (const btn of buttons) {
      const text = (btn.textContent || '').trim();
      const cls = btn.getAttribute('class') || '';
      if (text === name && cls.includes('text-sm') && !cls.includes('pl-8')) {
        (btn as HTMLElement).click();
        return true;
      }
    }
    return false;
  }, dbName);
  if (!clicked) throw new Error(`未找到数据库节点 "${dbName}"`);
}

describe('PostgreSQL 多库会话 UI (F4-E2E)', () => {
  let mainWindow: string;
  let shouldSkip = false;

  before(async () => {
    if (skipRequested()) {
      console.warn('⏩ Skipping PostgreSQL multi-db E2E: E2E_SKIP_PG=1');
      shouldSkip = true;
      return;
    }
    if (!(await postgresReachable())) {
      console.warn(
        `⏩ Skipping PostgreSQL multi-db E2E: ${PG_HOST}:${PG_PORT} unreachable`,
      );
      shouldSkip = true;
      return;
    }

    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    try {
      await createAndConnectPostgreSQL({
        name: CONN_NAME,
        host: PG_HOST,
        port: String(PG_PORT),
        user: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: '', // no default DB → multi-db session tree
      });
    } catch (err) {
      console.warn('⏩ Skipping PostgreSQL multi-db E2E: connect failed', err);
      shouldSkip = true;
    }
  });

  after(async () => {
    if (shouldSkip || !mainWindow) return;
    try {
      await closeExtraWindows(mainWindow);
    } catch { /* ignore */ }
  });

  it('无默认库连接后侧边栏应列出多个数据库节点 (F4-E2E-001)', async function () {
    if (shouldSkip) return this.skip();

    await browser.waitUntil(
      async () => (await listSidebarDbNames()).length >= 2,
      { timeout: 20000, timeoutMsg: '等待多库 schema 树加载超时' },
    );

    const dbNames = await listSidebarDbNames();
    expect(dbNames.length).toBeGreaterThanOrEqual(2);
    expect(dbNames).toContain(EXPECTED_DB);

    // Multi-db tree should not show StandardSchemaTree section headers before expand
    const asideText = await $('aside').getText();
    expect(asideText.includes(`${t('schemaTree.tables')} (`) || asideText.startsWith(t('schemaTree.tables'))).toBe(false);
  });

  it('展开数据库节点后应加载表 (F4-E2E-002)', async function () {
    if (shouldSkip) return this.skip();

    await browser.waitUntil(
      async () => (await listSidebarDbNames()).includes(EXPECTED_DB),
      { timeout: 15000, timeoutMsg: `等待数据库节点 ${EXPECTED_DB}` },
    );

    await clickSidebarDb(EXPECTED_DB);
    await browser.pause(2000);

    await browser.waitUntil(
      async () => {
        const text = await $('aside').getText();
        // Either tables appeared or empty-state under the expanded DB
        return text.includes(t('schemaTree.noTables')) || (await $$('aside button')).length > (await listSidebarDbNames()).length;
      },
      { timeout: 15000, timeoutMsg: '展开数据库后未加载表' },
    );

    const asideText = await $('aside').getText();
    const dbNames = await listSidebarDbNames();
    // After expand, either table buttons or empty message; DB nodes still present
    expect(dbNames).toContain(EXPECTED_DB);
    const hasTablesOrEmpty =
      asideText.includes(t('schemaTree.noTables')) ||
      (await browser.execute(() => {
        return Array.from(document.querySelectorAll('aside button')).some((btn) => {
          const cls = btn.getAttribute('class') || '';
          return cls.includes('pl-8') || cls.includes('text-[13px]');
        });
      }));
    expect(hasTablesOrEmpty).toBe(true);
  });

  it('多库时 QueryPanel 应显示数据库选择器 (F4-E2E-003)', async function () {
    if (shouldSkip) return this.skip();

    const dbNames = await listSidebarDbNames();
    if (dbNames.length < 2) {
      console.warn('Only one DB visible; skipping QueryPanel selector assertion');
      return this.skip();
    }

    await openQueryTab();
    await browser.pause(1000);

    // QueryPanel Select: aria-haspopup=listbox near execute button
    const selector = await $('button[aria-haspopup="listbox"]');
    await expect(selector).toBeDisplayed();

    const selectorText = (await selector.getText()).trim();
    expect(selectorText.length).toBeGreaterThan(0);
    // Selected value should be one of the listed databases
    expect(dbNames.some((n) => selectorText.includes(n))).toBe(true);
  });
});
