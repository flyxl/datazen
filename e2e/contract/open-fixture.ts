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
const SQLITE_DB = path.resolve(__dirname, '../fixtures/test.db');
const SQLITE_SETUP = path.resolve(__dirname, '../create-sqlite-test-db.mjs');

export interface ContractConnCtx {
  readonly fixture: DriverFixtureDefinition;
  readonly mainWindow: string;
  readonly connWindow: string;
}

async function openSqlite(fixture: DriverFixtureDefinition, mainWindow: string) {
  // The SQLite database is an ignored/generated fixture. Recreate it before
  // connecting so the navigator sees the stable contract table at first load.
  execFileSync(process.execPath, [SQLITE_SETUP], { stdio: 'pipe' });
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
    await dbInput.setValue(SQLITE_DB);
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
        browser.execute(
          (name: string) => {
            const home = document.querySelector('[data-testid="connection-workspace-home"]');
            const text = home?.textContent ?? '';
            return text.includes(name) && /SQLite/i.test(text);
          },
          fixture.displayName,
        ),
      { timeout: 20000, timeoutMsg: `等待 SQLite 连接 "${fixture.displayName}" 激活超时` },
    );
  } catch (error) {
    const state = await browser.execute(() => ({
      title: document.title,
      toolbar: Boolean(document.querySelector('[data-testid="conn-toolbar-new-query"]')),
      body: document.body.innerText.slice(0, 1800),
      connections: Array.from(document.querySelectorAll('[data-conn-item]')).map((item) => ({
        name: item.getAttribute('data-conn-name') || item.textContent?.trim() || '',
        expanded: item.querySelector('button[aria-expanded]')?.getAttribute('aria-expanded') ?? null,
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
