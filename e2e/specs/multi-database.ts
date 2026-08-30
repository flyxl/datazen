/**
 * Multi-database session UI (F2/F4).
 *
 * Parameterized test covering both MySQL and PostgreSQL:
 *   - Connect without database → schema tree lists multiple DB nodes
 *   - Expand a DB → tables load under that node
 *   - QueryPanel DB selector visible when multiple DBs
 *
 * Credentials: E2E_MYSQL_* / E2E_PG_* (see e2e/.env.example). Each driver
 * skips gracefully when unreachable or E2E_SKIP_*=1.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { createConnection } from 'node:net';
import { t } from '../i18n.js';
import {
  createAndConnectMySQL,
  createAndConnectPostgreSQL,
  closeExtraWindows,
  openQueryTab,
} from '../helpers.js';

// ── Driver configs ────────────────────────────────────────────────

interface DriverConfig {
  label: string;
  skipEnv: string;
  host: string;
  port: number;
  expectedDb: string;
  connName: string;
  connect: (opts: Record<string, string>) => Promise<void>;
  reachable: (host: string, port: number, timeoutMs?: number) => Promise<boolean>;
}

const drivers: DriverConfig[] = [
  {
    label: 'MySQL',
    skipEnv: 'E2E_SKIP_MYSQL',
    host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port: Number(process.env.E2E_MYSQL_PORT) || 3306,
    expectedDb: process.env.E2E_MYSQL_DB || 'datazen_test',
    connName: 'E2E-MySQL-MultiDb',
    connect: (opts) =>
      createAndConnectMySQL({
        name: opts.name,
        host: opts.host,
        port: opts.port,
        user: opts.user,
        password: opts.password,
        database: '', // no default DB → multi-db session tree
      }),
    reachable: async (host, port, timeoutMs = 2000) =>
      new Promise((resolve) => {
        const sock = createConnection({ host, port });
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
      }),
  },
  {
    label: 'PostgreSQL',
    skipEnv: 'E2E_SKIP_PG',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    expectedDb: process.env.E2E_PG_DB || 'postgres',
    connName: 'E2E-PostgreSQL-MultiDb',
    connect: (opts) =>
      createAndConnectPostgreSQL({
        name: opts.name,
        host: opts.host,
        port: opts.port,
        user: opts.user,
        password: opts.password,
        database: '', // no default DB → multi-db session tree
      }),
    reachable: async (host, port, timeoutMs = 2000) =>
      new Promise((resolve) => {
        const sock = createConnection({ host, port });
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
      }),
  },
];

// ── Shared helpers ────────────────────────────────────────────────

/** Collect sidebar database node names via the tree's stable data attributes. */
async function listSidebarDbNames(): Promise<string[]> {
  return browser.execute(() => {
    const out: string[] = [];
    document.querySelectorAll('aside [data-tree-node="db"]').forEach((btn) => {
      const name = btn.getAttribute('data-db-name');
      if (name) out.push(name);
    });
    return out;
  });
}

async function clickSidebarDb(dbName: string) {
  const clicked = await browser.execute((name: string) => {
    const nodes = document.querySelectorAll('aside [data-tree-node="db"]');
    for (const btn of Array.from(nodes)) {
      if (btn.getAttribute('data-db-name') === name) {
        (btn as HTMLElement).click();
        return true;
      }
    }
    return false;
  }, dbName);
  if (!clicked) throw new Error(`未找到数据库节点 "${dbName}"`);
}

// ── Parameterized test suites ─────────────────────────────────────

for (const drv of drivers) {
  const TEST_ID = drv.label === 'MySQL' ? 'F2-E2E' : 'F4-E2E';

  describe(`${drv.label} 多库会话 UI (${TEST_ID})`, () => {
    let mainWindow: string;
    let shouldSkip = false;

    before(async () => {
      if (process.env[drv.skipEnv] === '1') {
        console.warn(`⏩ Skipping ${drv.label} multi-db E2E: ${drv.skipEnv}=1`);
        shouldSkip = true;
        return;
      }
      if (!(await drv.reachable(drv.host, drv.port))) {
        console.warn(`⏩ Skipping ${drv.label} multi-db E2E: ${drv.host}:${drv.port} unreachable`);
        shouldSkip = true;
        return;
      }

      const handles = await browser.getWindowHandles();
      mainWindow = handles.find((h) => h === 'main') ?? handles[0];
      await browser.switchToWindow(mainWindow);
      await closeExtraWindows(mainWindow);
      await browser.pause(1000);

      try {
        await drv.connect({
          name: drv.connName,
          host: drv.host,
          port: String(drv.port),
          user:
            process.env[`E2E_${drv.label.toUpperCase()}_USER`] ||
            (drv.label === 'MySQL' ? 'root' : 'postgres'),
          password: process.env[`E2E_${drv.label.toUpperCase()}_PASSWORD`] || '',
        });
      } catch (err) {
        console.warn(`⏩ Skipping ${drv.label} multi-db E2E: connect failed`, err);
        shouldSkip = true;
      }
    });

    after(async () => {
      if (shouldSkip || !mainWindow) return;
      try {
        await closeExtraWindows(mainWindow);
      } catch {
        /* ignore */
      }
    });

    it(`无默认库连接后侧边栏应列出多个数据库节点 (${TEST_ID}-001)`, async function () {
      if (shouldSkip) return this.skip();

      await browser.waitUntil(async () => (await listSidebarDbNames()).length >= 2, {
        timeout: 20000,
        timeoutMsg: '等待多库 schema 树加载超时',
      });

      const dbNames = await listSidebarDbNames();
      expect(dbNames.length).toBeGreaterThanOrEqual(2);
      expect(dbNames).toContain(drv.expectedDb);

      // Multi-db tree should not show StandardSchemaTree section headers before expand
      const asideText = await $('aside').getText();
      expect(
        asideText.includes(`${t('schemaTree.tables')} (`) ||
          asideText.startsWith(t('schemaTree.tables')),
      ).toBe(false);
    });

    it(`展开数据库节点后应加载表 (${TEST_ID}-002)`, async function () {
      if (shouldSkip) return this.skip();

      await browser.waitUntil(async () => (await listSidebarDbNames()).includes(drv.expectedDb), {
        timeout: 15000,
        timeoutMsg: `等待数据库节点 ${drv.expectedDb}`,
      });

      await clickSidebarDb(drv.expectedDb);
      await browser.pause(2000);

      await browser.waitUntil(
        async () => {
          const text = await $('aside').getText();
          return (
            text.includes(t('schemaTree.noTables')) ||
            (await $$('aside button')).length > (await listSidebarDbNames()).length
          );
        },
        { timeout: 15000, timeoutMsg: '展开数据库后未加载表' },
      );

      const asideText = await $('aside').getText();
      const dbNames = await listSidebarDbNames();
      expect(dbNames).toContain(drv.expectedDb);
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

    it(`多库时 QueryPanel 应显示数据库选择器 (${TEST_ID}-003)`, async function () {
      if (shouldSkip) return this.skip();

      const dbNames = await listSidebarDbNames();
      if (dbNames.length < 2) {
        console.warn('Only one DB visible; skipping QueryPanel selector assertion');
        return this.skip();
      }

      await openQueryTab();
      await browser.pause(1000);

      const selector = await $('button[aria-haspopup="listbox"]');
      await expect(selector).toBeDisplayed();

      const selectorText = (await selector.getText()).trim();
      expect(selectorText.length).toBeGreaterThan(0);
      expect(dbNames.some((n) => selectorText.includes(n))).toBe(true);
    });
  });
}
