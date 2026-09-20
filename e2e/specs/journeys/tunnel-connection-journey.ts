/**
 * Tunnel form + optional live tunnel Test Connection journey.
 *
 * Always exercises the connection dialog tunnel UI (SSH / HTTP proxy / WebSocket).
 * When the corresponding E2E_* env vars from e2e/.env are set, also runs a real
 * "Test Connection" through that tunnel against the configured DB.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  closeNewConnectionDialogFromUi,
  expandNewConnectionAdvanced,
  openConnectionsWorkspace,
  openNewConnectionDialogFromUi,
  selectDzOptionInWrap,
  clickNewConnectionTest,
} from '../../helpers.js';
import { t } from '../../i18n.js';
import {
  fillPostgresConnectionForm,
  waitForConnectionTestResult,
} from './connectionJourneyHelpers.js';

const SSH_HOST = process.env.E2E_SSH_HOST || '';
const HTTP_PROXY_HOST = process.env.E2E_HTTP_PROXY_HOST || '';
const WS_URL = process.env.E2E_WS_TUNNEL_URL || '';

const TUNNEL_DB = {
  host: process.env.E2E_TUNNEL_DB_HOST || process.env.E2E_PG_HOST || '127.0.0.1',
  port: process.env.E2E_TUNNEL_DB_PORT || process.env.E2E_PG_PORT || '5432',
  database: process.env.E2E_TUNNEL_DB_NAME || process.env.E2E_PG_DB || 'postgres',
  username: process.env.E2E_TUNNEL_DB_USER || process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_TUNNEL_DB_PASSWORD || process.env.E2E_PG_PASSWORD || '',
};

async function expandTunnelSection(): Promise<void> {
  await expandNewConnectionAdvanced();
  const toggle = await $('[data-testid="new-conn-tunnel-toggle"]');
  await toggle.waitForDisplayed({ timeout: 8000 });
  const expanded = await toggle.getAttribute('aria-expanded').catch(() => null);
  if (expanded !== 'true') {
    await toggle.click();
    await browser.pause(300);
  }
}

async function selectTunnelKind(optionKey: string): Promise<void> {
  await expandTunnelSection();
  await selectDzOptionInWrap('new-conn-tunnel-kind', t(optionKey as Parameters<typeof t>[0]));
  await browser.pause(250);
}

async function fillHttpProxyFromEnv(): Promise<void> {
  const host = await $('[data-testid="new-conn-http-proxy-host"]');
  await host.waitForDisplayed({ timeout: 5000 });
  await host.clearValue();
  await host.setValue(HTTP_PROXY_HOST);
  const port = await $('[data-testid="new-conn-http-proxy-port"]');
  await port.clearValue();
  await port.setValue(process.env.E2E_HTTP_PROXY_PORT || '8080');
  const user = process.env.E2E_HTTP_PROXY_USER || '';
  if (user) {
    const u = await $('[data-testid="new-conn-http-proxy-username"]');
    await u.clearValue();
    await u.setValue(user);
  }
  const pw = process.env.E2E_HTTP_PROXY_PASSWORD || '';
  if (pw) {
    const p = await $('[data-testid="new-conn-http-proxy-password"]');
    await p.clearValue();
    await p.setValue(pw);
  }
}

async function fillWebSocketFromEnv(): Promise<void> {
  const url = await $('[data-testid="new-conn-ws-url"]');
  await url.waitForDisplayed({ timeout: 5000 });
  await url.clearValue();
  await url.setValue(WS_URL);
  const token = process.env.E2E_WS_TUNNEL_TOKEN || '';
  if (token) {
    const tok = await $('[data-testid="new-conn-ws-token"]');
    await tok.clearValue();
    await tok.setValue(token);
  }
}

async function fillSshFromEnv(): Promise<void> {
  const hostInput = await $('input[placeholder="ssh.example.com"]');
  await hostInput.waitForDisplayed({ timeout: 5000 });
  await hostInput.clearValue();
  await hostInput.setValue(SSH_HOST);

  const userInput = await $('input[placeholder="root"]');
  if (await userInput.isExisting()) {
    await userInput.clearValue();
    await userInput.setValue(process.env.E2E_SSH_USER || '');
  }

  if (process.env.E2E_SSH_PASSWORD) {
    const pw = await $('[data-testid="new-connection-dialog"] input[type="password"]');
    if (await pw.isExisting()) {
      await pw.clearValue();
      await pw.setValue(process.env.E2E_SSH_PASSWORD);
    }
  }
}

describe('隧道连接完整用户旅程 (TUNNEL-CONNECTION-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openConnectionsWorkspace(mainWindow);
  });

  afterEach(async () => {
    await closeNewConnectionDialogFromUi();
    await closeExtraWindows(mainWindow);
  });

  it('完整旅程：切换隧道类型 → 条件字段显示 → 切换无误', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(`E2E Tunnel UI ${Date.now().toString(36)}`, TUNNEL_DB);
    await expandTunnelSection();
    await captureJourneyStep('tunnel-section-open');

    await selectTunnelKind('newConn.tunnelHttpProxy');
    await expect(await $('[data-testid="new-conn-http-proxy-fields"]')).toBeDisplayed();
    await captureJourneyStep('tunnel-http-proxy-fields');

    await selectTunnelKind('newConn.tunnelWebSocket');
    await expect(await $('[data-testid="new-conn-ws-fields"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-http-proxy-fields"]')).not.toBeDisplayed();
    await captureJourneyStep('tunnel-websocket-fields');

    await selectTunnelKind('newConn.tunnelSsh');
    await expect(await $('[data-testid="new-conn-http-proxy-fields"]')).not.toBeDisplayed();
    await expect(await $('[data-testid="new-conn-ws-fields"]')).not.toBeDisplayed();
    await captureJourneyStep('tunnel-ssh-fields');

    await selectTunnelKind('newConn.tunnelNone');
    await captureJourneyStep('tunnel-none');
  });

  it('HTTP 代理：必填缺失时测试连接不崩溃', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(`E2E Tunnel HTTP empty ${Date.now().toString(36)}`, TUNNEL_DB);
    await selectTunnelKind('newConn.tunnelHttpProxy');
    await clickNewConnectionTest();
    await browser.pause(1500);
    await expect(await $('[data-testid="new-conn-test-connection"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-http-proxy-fields"]')).toBeDisplayed();
  });

  (HTTP_PROXY_HOST
    ? it
    : it.skip)('可选：经 HTTP CONNECT 代理测试连接（需 E2E_HTTP_PROXY_*）', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(`E2E Tunnel HTTP live ${Date.now().toString(36)}`, TUNNEL_DB);
    await selectTunnelKind('newConn.tunnelHttpProxy');
    await fillHttpProxyFromEnv();
    await captureJourneyStep('tunnel-http-proxy-filled');
    await clickNewConnectionTest();
    await waitForConnectionTestResult('success', 60000);
    await captureJourneyStep('tunnel-http-proxy-test-ok');
  });

  (WS_URL
    ? it
    : it.skip)('可选：经 WebSocket 隧道测试连接（需 E2E_WS_TUNNEL_*）', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(`E2E Tunnel WS live ${Date.now().toString(36)}`, TUNNEL_DB);
    await selectTunnelKind('newConn.tunnelWebSocket');
    await fillWebSocketFromEnv();
    await captureJourneyStep('tunnel-ws-filled');
    await clickNewConnectionTest();
    await waitForConnectionTestResult('success', 60000);
    await captureJourneyStep('tunnel-ws-test-ok');
  });

  (SSH_HOST
    ? it
    : it.skip)('可选：经 SSH 隧道测试连接（需 E2E_SSH_*）', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(`E2E Tunnel SSH live ${Date.now().toString(36)}`, TUNNEL_DB);
    await selectTunnelKind('newConn.tunnelSsh');
    await fillSshFromEnv();
    await captureJourneyStep('tunnel-ssh-filled');
    await clickNewConnectionTest();
    await waitForConnectionTestResult('success', 90000);
    await captureJourneyStep('tunnel-ssh-test-ok');
  });
});
