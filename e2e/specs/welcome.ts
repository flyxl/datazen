/**
 * F5 welcome page journeys (F5-E2E-001 ~ F5-E2E-005).
 *
 * wdio.conf.ts always seeds `conn_e2e_pg` in the global `before` hook, so this
 * suite clears all connections locally, reloads the main window, and restores
 * the seeded PG connection in `after` so later specs keep working.
 */
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
  return result as T;
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

describe('首次安装欢迎页 (F5-E2E-001 ~ F5-E2E-005)', () => {
  let mainWindow: string;
  const welcomeConnName = 'F5-E2E-欢迎页连接';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await deleteAllConnections();
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
      if (c.name === welcomeConnName || c.id.startsWith('f5-e2e-welcome-')) {
        await invokeBackend('delete_connection', { id: c.id });
      }
    }
    await reseedE2ePgConnection();
    await browser.execute(() => location.reload());
    await browser.pause(1500);
  });

  it('F5-E2E-001: 无连接时主窗显示欢迎页而非工作区侧栏', async () => {
    const welcome = await $('[data-testid="welcome-page"]');
    await welcome.waitForDisplayed({ timeout: 15000 });
    await expect(welcome).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-connections"]')).not.toBeExisting();
  });

  it('F5-E2E-002: 欢迎页展示标题与四宫格功能介绍', async () => {
    await $('[data-testid="welcome-page"]').waitForDisplayed({ timeout: 15000 });
    const body = await $('body').getText();
    expect(body).toContain(t('welcome.title'));
    expect(body).toContain(t('welcome.feature.connections.title'));
    expect(body).toContain(t('welcome.feature.dashboard.title'));
    expect(body).toContain(t('welcome.feature.workflow.title'));
    expect(body).toContain(t('welcome.feature.ai.title'));
  });

  it('F5-E2E-003: 欢迎页 CTA 打开新建连接弹窗', async () => {
    await $('[data-testid="welcome-page"]').waitForDisplayed({ timeout: 15000 });
    const cta = await $('[data-testid="welcome-create-connection"]');
    await cta.click();
    await waitForNewConnectionDialog();
    await expect(await $('[data-testid="new-connection-dialog"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-save"]')).toBeDisplayed();
    await closeNewConnectionDialogFromUi();
  });

  it('F5-E2E-004: 保存首个连接后主窗进入 ConnectionPage 工作区', async () => {
    await $('[data-testid="welcome-page"]').waitForDisplayed({ timeout: 15000 });
    await $('[data-testid="welcome-create-connection"]').click();
    await waitForNewConnectionDialog();

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue(welcomeConnName);
    await clickNewConnectionSave();

    await browser.waitUntil(
      async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
      { timeout: 15000, timeoutMsg: '等待新建连接弹窗关闭超时' },
    );
    await browser.switchToWindow(mainWindow);

    const nav = await $('[data-testid="workspace-nav-connections"]');
    await nav.waitForDisplayed({ timeout: 15000 });
    await expect(nav).toBeDisplayed();
    await expect(await $('[data-testid="welcome-page"]')).not.toBeExisting();

    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 10000,
      timeoutMsg: '等待连接列表出现超时',
    });
  });

  it('F5-E2E-005: 删除最后一个连接后回到欢迎页', async () => {
    const conns = await invokeBackend<Conn[]>('get_connections');
    expect(conns.length).toBeGreaterThan(0);
    for (const c of conns) {
      await invokeBackend('delete_connection', { id: c.id });
    }

    await browser.execute(() => location.reload());
    await browser.pause(1500);

    const welcome = await $('[data-testid="welcome-page"]');
    await welcome.waitForDisplayed({ timeout: 15000 });
    await expect(welcome).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-connections"]')).not.toBeExisting();
  });
});
