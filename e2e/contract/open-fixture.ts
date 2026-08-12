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
  switchToNewWindow,
} from '../helpers.js';
import { t } from '../i18n.js';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SQLITE_DB = path.resolve(__dirname, '../fixtures/test.db');

export interface ContractConnCtx {
  readonly fixture: DriverFixtureDefinition;
  readonly mainWindow: string;
  readonly connWindow: string;
}

async function openSqlite(fixture: DriverFixtureDefinition, mainWindow: string) {
  await expandAllGroups();
  const existing = await findCardByName(fixture.displayName);
  if (existing) {
    await dblclickConnByExactName(fixture.displayName);
  } else {
    await $(`button*=${t('action.newConnection')}`).click();
    await switchToNewWindow(mainWindow);
    await $('button*=SQLite').click();
    await browser.pause(300);
    await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`).setValue(fixture.displayName);
    const dbInput = await $('input[placeholder="/path/to/db.sqlite"]');
    await dbInput.setValue(SQLITE_DB);
    await $(`button*=${t('common.save')}`).click();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1, {
      timeout: 10000,
    });
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
    await dblclickConnByExactName(fixture.displayName);
  }
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
    timeout: 30000,
  });
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
  await browser.pause(800);
  return connWindow;
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
