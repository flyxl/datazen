/**
 * Zero-connection workspace empty state (ZERO-001 ~ ZERO-005).
 *
 * Replaces the retired WelcomePage suite (`welcome.ts`): with no saved
 * connections the main window now renders `ConnectionPage` directly, and the
 * empty state is owned by `ConnectionWorkspaceHome` state 1 (no-connections
 * CTA) plus the navigator `no-connections` row.
 *
 * wdio.conf.ts always seeds `conn_e2e_pg` in the global `before` hook, so this
 * suite clears all connections locally, reloads the main window, and restores
 * the seeded PG connection in `after` so later specs keep working.
 */
import { CONNECT_COMMANDS, dbSessionIdOf } from '../lib/sessionId.js';
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  waitForNewConnectionDialog,
  closeNewConnectionDialogFromUi,
  clickNewConnectionSave,
} from '../helpers.js';
import { t } from '../i18n.js';

interface Conn {
  id: string;
  name: string;
}

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
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
  return (CONNECT_COMMANDS.has(cmd) ? dbSessionIdOf(result) : result) as T;
}

async function deleteAllConnections() {
  const conns = await invokeBackend<Conn[]>('get_connections');
  for (const c of conns) {
    await invokeBackend('delete_connection', { id: c.id });
  }
}

async function reseedE2ePgConnection() {
  const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
  const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
  const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
  const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
  const pgDatabase = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';

  await invokeBackend('save_connection', {
    config: {
      id: 'conn_e2e_pg',
      name: '本地 PostgreSQL',
      databaseType: 'postgresql',
      host: pgHost,
      port: pgPort,
      username: pgUser,
      password: pgPassword,
      database: pgDatabase,
      group: 'E2E 测试',
      colorTag: 'blue',
      sslMode: 'disable',
    },
  });
}

describe('零连接工作区空状态 (ZERO-001 ~ ZERO-005)', () => {
  let mainWindow: string;
  const zeroConnName = 'ZERO-零连接工作区连接';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await deleteAllConnections();
    // Bypass the onboarding wizard gate (a wiped data dir is a fresh install
    // whose wizard window replaces the main window); this suite asserts the
    // post-onboarding zero-connection workspace itself.
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    await invokeBackend('save_settings', {
      settings: { ...settings, onboarding: { completed: true, version: 1 } },
    });
    await browser.execute(() => location.reload());
    await browser.pause(1500);
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
  });

  after(async () => {
    const conns = await invokeBackend<Conn[]>('get_connections');
    for (const c of conns) {
      if (c.name === zeroConnName || c.id.startsWith('zero-e2e-conn-')) {
        await invokeBackend('delete_connection', { id: c.id });
      }
    }
    await reseedE2ePgConnection();
    // Keep the onboarding gate closed for the following suites: the first-run
    // journey owns its own spec (`journeys/onboarding-journey.ts`) and would
    // otherwise replace MainPage for everything that runs after this file.
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    await invokeBackend('save_settings', {
      settings: { ...settings, onboarding: { completed: true, version: 1 } },
    });
    await browser.execute(() => location.reload());
    await browser.pause(1500);
  });

  it('ZERO-001: 无连接时主窗直接显示工作区空状态（导航栏仍在）', async () => {
    const home = await $('[data-testid="connection-workspace-home"]');
    await home.waitForDisplayed({ timeout: 15000 });
    await expect(home).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-databases"]')).toBeDisplayed();
  });

  it('ZERO-002: 空状态展示无连接文案与新建/导入入口', async () => {
    await $('[data-testid="connection-workspace-home"]').waitForDisplayed({ timeout: 15000 });
    const body = await $('body').getText();
    expect(body).toContain(t('main.noConnections'));
    expect(body).toContain(t('connWin.home.emptyNoConnectionsHint'));
    await expect(await $('[data-testid="new-connection-button"]')).toBeDisplayed();
    await expect(await $('[data-testid="import-connections-button"]')).toBeDisplayed();
  });

  it('ZERO-003: 空状态 CTA 打开新建连接弹窗', async () => {
    await $('[data-testid="connection-workspace-home"]').waitForDisplayed({ timeout: 15000 });
    const cta = await $('[data-testid="new-connection-button"]');
    await cta.click();
    await waitForNewConnectionDialog();
    await expect(await $('[data-testid="new-connection-dialog"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-save"]')).toBeDisplayed();
    await closeNewConnectionDialogFromUi();
  });

  it('ZERO-004: 保存首个连接后空状态消失、连接列表出现', async () => {
    await $('[data-testid="connection-workspace-home"]').waitForDisplayed({ timeout: 15000 });
    await $('[data-testid="new-connection-button"]').click();
    await waitForNewConnectionDialog();

    const nameInput = await $('[data-testid="new-conn-name"]');
    await nameInput.setValue(zeroConnName);
    await clickNewConnectionSave();

    await browser.waitUntil(
      async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
      { timeout: 15000, timeoutMsg: '等待新建连接弹窗关闭超时' },
    );
    await browser.switchToWindow(mainWindow);

    const nav = await $('[data-testid="workspace-nav-databases"]');
    await nav.waitForDisplayed({ timeout: 15000 });
    await expect(nav).toBeDisplayed();
    // `[data-testid="new-connection-button"]` cannot express "the zero state is
    // gone": the navigator toolbar's own "+" carries the same testid and is
    // rendered first, so it always exists. Assert on the empty-state-only
    // testids instead — they exist only while the workspace still has no
    // connections, and the card list's CTA appears once it has one.
    expect(await $('[data-testid="import-connections-button"]').isExisting()).toBe(false);
    await $('[data-testid="empty-new-connection-button"]').waitForDisplayed({ timeout: 15000 });

    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 10000,
      timeoutMsg: '等待连接列表出现超时',
    });
  });

  it('ZERO-005: 删除最后一个连接后回到工作区空状态', async () => {
    const conns = await invokeBackend<Conn[]>('get_connections');
    expect(conns.length).toBeGreaterThan(0);
    for (const c of conns) {
      await invokeBackend('delete_connection', { id: c.id });
    }

    await browser.execute(() => location.reload());
    await browser.pause(1500);

    const home = await $('[data-testid="connection-workspace-home"]');
    await home.waitForDisplayed({ timeout: 15000 });
    await expect(home).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-databases"]')).toBeDisplayed();
  });
});
