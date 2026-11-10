/**
 * WDIO helpers to open a DriverFixture connection window.
 */
import { browser, $ } from '@wdio/globals';
import type { DriverFixtureDefinition } from './fixtures';
import {
  clickCardConnectButton,
  closeExtraWindows,
  createAndConnectMySQL,
  dblclickConnByExactName,
  expandAllGroups,
  findCardByName,
  openSeededPgConnectionWindow,
  openNewConnectionDialogFromUi,
  selectNewConnectionDriver,
  clickNewConnectionSave,
  waitForConnectionToolbar,
} from '../helpers.js';
import { t } from '../i18n.js';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SQLITE_SETUP = path.resolve(__dirname, '../create-sqlite-test-db.mjs');

function resolveSqliteFixturePath(): string {
  const workerIndex = process.env.E2E_WORKER_INDEX;
  const port = process.env.E2E_WD_PORT;
  const workerKey =
    workerIndex != null && workerIndex !== ''
      ? `worker-${workerIndex}`
      : port != null && port !== ''
        ? `port-${port}`
        : null;
  return (
    process.env.E2E_SQLITE_DB_PATH ||
    path.resolve(__dirname, '../fixtures', workerKey ? `test-${workerKey}.db` : 'test.db')
  );
}

export interface ContractConnCtx {
  readonly fixture: DriverFixtureDefinition;
  readonly mainWindow: string;
  readonly connWindow: string;
}

async function openSqlite(fixture: DriverFixtureDefinition, mainWindow: string) {
  // The SQLite database is an ignored/generated fixture. Recreate it before
  // connecting so the navigator sees the stable contract table at first load.
  // Use per-worker paths in parallel runs to avoid cross-worker fixture races.
  const sqliteDbPath = resolveSqliteFixturePath();
  execFileSync(process.execPath, [SQLITE_SETUP], {
    stdio: 'pipe',
    env: { ...process.env, E2E_SQLITE_DB_PATH: sqliteDbPath },
  });
  await expandAllGroups();
  const existing = await findCardByName(fixture.displayName);
  if (existing) {
    await dblclickConnByExactName(fixture.displayName);
  } else {
    await openNewConnectionDialogFromUi();
    await selectNewConnectionDriver('sqlite');
    await browser.pause(300);
    await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`).setValue(fixture.displayName);
    const dbInput = await $('input[placeholder="/path/to/db.sqlite"]');
    await dbInput.setValue(sqliteDbPath);
    await clickNewConnectionSave();
    await browser.waitUntil(
      async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
      { timeout: 10000 },
    );
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
    await dblclickConnByExactName(fixture.displayName);
  }
  await browser.switchToWindow(mainWindow);
  // waitForConnectionToolbar alone can be a false positive here: the
  // previous MySQL connection may still own the same main window while the
  // SQLite double-click is connecting asynchronously. The document title is
  // owned by the native window, so use the rendered workspace home as the
  // active-connection marker instead of document.title.
  try {
    await browser.waitUntil(
      async () =>
        browser.execute((name: string) => {
          const home = document.querySelector('[data-testid="connection-workspace-home"]');
          const text = home?.textContent ?? '';
          return text.includes(name) && /SQLite/i.test(text);
        }, fixture.displayName),
      { timeout: 20000, timeoutMsg: `等待 SQLite 连接 "${fixture.displayName}" 激活超时` },
    );
  } catch (error) {
    const state = await browser.execute(() => ({
      title: document.title,
      toolbar: Boolean(document.querySelector('[data-testid="conn-toolbar-new-query"]')),
      body: document.body.innerText.slice(0, 1800),
      connections: Array.from(document.querySelectorAll('[data-conn-item]')).map((item) => ({
        name: item.getAttribute('data-conn-name') || item.textContent?.trim() || '',
        expanded:
          item.querySelector('button[aria-expanded]')?.getAttribute('aria-expanded') ?? null,
      })),
    }));
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}; SQLite activation state=${JSON.stringify(state)}`,
    );
  }
  await waitForConnectionToolbar();
  return mainWindow;
}

/** Close extras, open fixture connection, return ctx focused on connection window. */
export async function openFixtureConnection(
  fixture: DriverFixtureDefinition,
  mainWindow: string,
): Promise<ContractConnCtx> {
  await browser.switchToWindow(mainWindow);
  await closeExtraWindows(mainWindow);
  await browser.pause(400);

  let connWindow: string;
  switch (fixture.id) {
    case 'postgres':
      connWindow = await openSeededPgConnectionWindow(mainWindow);
      break;
    case 'mysql': {
      const r = await createAndConnectMySQL({ name: fixture.displayName });
      connWindow = r.connWindow;
      break;
    }
    case 'sqlite':
      connWindow = await openSqlite(fixture, mainWindow);
      break;
    default:
      throw new Error(`Unsupported fixture: ${(fixture as DriverFixtureDefinition).id}`);
  }

  return { fixture, mainWindow, connWindow };
}

export async function focusContractCtx(ctx: ContractConnCtx) {
  await browser.switchToWindow(ctx.connWindow);
}

/**
 * Make `name` the one and only expanded connection in the navigator, and make
 * it the active one.
 *
 * The matrix runs postgres, then mysql, then sqlite inside a single Tauri
 * process, and every fixture leaves its connection live: the `after` hook only
 * closes extra OS windows. All three databases carry identically named
 * `e2e_contract_*` tables, so a merely-*expanded* leftover connection renders
 * duplicate `[data-testid="schema-tree-node"][data-item-name=...]` rows. A
 * `clickTableInSidebar` lookup returns the first match and therefore opened the
 * table panel against the wrong driver — seeded rows in SQLite, a `rows=0` read
 * from `datazen_test`. Collapsing every other connection first removes the
 * ambiguity; the row click then re-activates this connection, because
 * ConnectionPage derives the active dbSession from the active tab/panel.
 */
export async function focusContractConnection(name: string) {
  await browser.waitUntil(
    async () =>
      browser.execute((conn: string) => {
        const items = Array.from(document.querySelectorAll('[data-conn-item]'));
        return items.some((el) => (el.textContent || '').includes(conn));
      }, name),
    { timeout: 15000, timeoutMsg: `等待连接 "${name}" 出现在导航树超时` },
  );
  await browser.execute((conn: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    for (const item of items) {
      const isTarget = (item.textContent || '').includes(conn);
      const chevron = item.querySelector('button[aria-expanded]') as HTMLElement | null;
      if (!chevron) continue;
      const expanded = chevron.getAttribute('aria-expanded') === 'true';
      if (isTarget && !expanded) chevron.click();
      else if (!isTarget && expanded) chevron.click();
    }
  }, name);
  await browser.pause(500);
  // Activate the connection: ConnectionPage derives the active dbSessionId from
  // the active tab/panel, so a new query tab inherits this connection only once
  // it has been clicked open.
  await browser.execute((conn: string) => {
    const item = Array.from(document.querySelectorAll('[data-conn-item]')).find((el) =>
      (el.textContent || '').includes(conn),
    );
    if (!item) return;
    const candidates = Array.from(item.querySelectorAll('button, [role="treeitem"]'));
    const row = candidates.find((c) => !c.querySelector('button[aria-expanded]')) as
      | HTMLElement
      | undefined;
    (row ?? (item as unknown as HTMLElement)).click();
  }, name);
  await browser.pause(500);
}
