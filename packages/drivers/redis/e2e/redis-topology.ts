/**
 * Optional Redis Cluster / Sentinel topology E2E (E3).
 *
 * Skips unless env endpoints are set (see e2e/.env.example). Default CI stays green
 * without cluster/sentinel infrastructure.
 */
import { createConnection } from 'node:net';
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../../../e2e/i18n.js';
import { closeExtraWindows, switchToNewWindow } from '../../../e2e/helpers.js';

const CLUSTER_CONN = 'E2E-Redis-Cluster';
const SENTINEL_CONN = 'E2E-Redis-Sentinel';

const CLUSTER_NODES_RAW = process.env.E2E_REDIS_CLUSTER_NODES?.trim() ?? '';
const CLUSTER_PASSWORD =
  process.env.E2E_REDIS_CLUSTER_PASSWORD ?? process.env.E2E_REDIS_PASSWORD ?? '';

const SENTINEL_NODES_RAW = process.env.E2E_REDIS_SENTINEL_NODES?.trim() ?? '';
const SENTINEL_MASTER = process.env.E2E_REDIS_SENTINEL_MASTER_NAME?.trim() ?? '';
const SENTINEL_PASSWORD =
  process.env.E2E_REDIS_SENTINEL_PASSWORD ?? process.env.E2E_REDIS_PASSWORD ?? '';
const SENTINEL_NODE_PASSWORD = process.env.E2E_REDIS_SENTINEL_NODE_PASSWORD ?? '';

function skipRequested(): boolean {
  return process.env.E2E_SKIP_REDIS === '1';
}

function parseNodeList(raw: string): string[] {
  if (!raw) return [];
  return raw
    .split(/[,\n]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function parseHostPort(node: string): { host: string; port: number } | null {
  const idx = node.lastIndexOf(':');
  if (idx <= 0) return null;
  const host = node.slice(0, idx);
  const port = Number(node.slice(idx + 1));
  if (!host || !Number.isFinite(port) || port <= 0) return null;
  return { host, port };
}

async function nodeReachable(node: string, timeoutMs = 2000): Promise<boolean> {
  const hp = parseHostPort(node);
  if (!hp) return false;
  return new Promise((resolve) => {
    const sock = createConnection({ host: hp.host, port: hp.port });
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

async function openRedisNewConnectionDialog(mainWindow: string) {
  const newConnBtn = await $(`button*=${t('action.newConnection')}`);
  await newConnBtn.click();
  await browser.waitUntil(
    async () => await $('[data-testid="new-connection-dialog"]').isExisting(),
    { timeout: 15000, timeoutMsg: 'new-connection dialog did not open' },
  );

  const redisBtn = await $('button*=Redis');
  await redisBtn.click();
  await browser.pause(300);

  return mainWindow;
}

async function setConnectionName(name: string) {
  const nameInput = await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`);
  await nameInput.setValue(name);
}

async function selectWizardTopology(kind: 'cluster' | 'sentinel') {
  const label =
    kind === 'cluster' ? t('redis.wizard.topologyCluster') : t('redis.wizard.topologySentinel');
  const trigger = await $('[data-testid="redis-topology"] button');
  await trigger.waitForDisplayed({ timeout: 10000 });
  await trigger.click();
  await browser.pause(200);
  const opt = await $(`div=${label}`);
  await opt.waitForDisplayed({ timeout: 5000 });
  await opt.click();
  await browser.pause(200);
}

async function setWizardMonoTextarea(value: string) {
  const ok = await browser.execute((val: string) => {
    const textareas = Array.from(document.querySelectorAll('textarea'));
    const el = textareas.find((ta) => ta.className.includes('font-mono')) as
      | HTMLTextAreaElement
      | undefined;
    if (!el) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
    setter?.call(el, val);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }, value);
  if (!ok) throw new Error('Redis wizard node textarea not found');
}

async function setWizardInputByPlaceholder(placeholder: string, value: string) {
  const ok = await browser.execute(
    (ph: string, val: string) => {
      const inputs = Array.from(document.querySelectorAll('input'));
      const el = inputs.find((i) => i.getAttribute('placeholder') === ph) as
        | HTMLInputElement
        | undefined;
      if (!el) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(el, val);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    },
    placeholder,
    value,
  );
  if (!ok) throw new Error(`Redis wizard input placeholder="${placeholder}" not found`);
}

async function setPasswordField(value: string) {
  const pwInputs = await $$('input[type="password"]');
  if (pwInputs.length === 0) throw new Error('password input not found');
  // Master/redis password is typically the last password field on endpoints step.
  const pwInput = pwInputs[pwInputs.length - 1];
  await pwInput.setValue(value);
}

async function clickTestConnection() {
  const testBtn = await $(`button*=${t('newConn.testConnection')}`);
  await testBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(t('newConn.testSuccess')) || body.includes('Driver error');
    },
    { timeout: 20000, timeoutMsg: 'Timed out waiting for Redis topology test connection' },
  );
}

async function assertTestSuccess() {
  const body = await $('body').getText();
  if (body.includes('Driver error')) {
    throw new Error('Redis topology test connection failed: ' + body.slice(0, 500));
  }
  expect(body).toContain(t('newConn.testSuccess'));
}

describe('Redis Cluster topology (E3, optional)', () => {
  let mainWindow: string;
  let shouldSkip = false;

  before(async function () {
    const nodes = parseNodeList(CLUSTER_NODES_RAW);
    if (skipRequested()) {
      console.warn('⏩ Skipping Redis Cluster E2E: E2E_SKIP_REDIS=1');
      shouldSkip = true;
      return;
    }
    if (nodes.length === 0) {
      console.warn('⏩ Skipping Redis Cluster E2E: E2E_REDIS_CLUSTER_NODES not set');
      shouldSkip = true;
      return;
    }
    if (!(await nodeReachable(nodes[0]!))) {
      console.warn(`⏩ Skipping Redis Cluster E2E: ${nodes[0]} unreachable`);
      shouldSkip = true;
      return;
    }

    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(500);
  });

  beforeEach(function () {
    if (shouldSkip) this.skip();
  });

  afterEach(async () => {
    if (shouldSkip) return;
    try {
      await browser.closeWindow();
    } catch {
      /* ignore */
    }
    try {
      await browser.switchToWindow(mainWindow);
    } catch {
      /* ignore */
    }
  });

  it('应能通过集群拓扑测试连接 (RD-CL-001)', async () => {
    const nodes = parseNodeList(CLUSTER_NODES_RAW);
    await openRedisNewConnectionDialog(mainWindow);
    await setConnectionName(CLUSTER_CONN);
    await selectWizardTopology('cluster');
    await setWizardMonoTextarea(nodes.join('\n'));
    if (CLUSTER_PASSWORD) {
      await setPasswordField(CLUSTER_PASSWORD);
    }
    await clickTestConnection();
    await assertTestSuccess();
  });
});

describe('Redis Sentinel topology (E3, optional)', () => {
  let mainWindow: string;
  let shouldSkip = false;

  before(async function () {
    const nodes = parseNodeList(SENTINEL_NODES_RAW);
    if (skipRequested()) {
      console.warn('⏩ Skipping Redis Sentinel E2E: E2E_SKIP_REDIS=1');
      shouldSkip = true;
      return;
    }
    if (nodes.length === 0 || !SENTINEL_MASTER) {
      console.warn(
        '⏩ Skipping Redis Sentinel E2E: E2E_REDIS_SENTINEL_NODES / E2E_REDIS_SENTINEL_MASTER_NAME not set',
      );
      shouldSkip = true;
      return;
    }
    if (!(await nodeReachable(nodes[0]!))) {
      console.warn(`⏩ Skipping Redis Sentinel E2E: ${nodes[0]} unreachable`);
      shouldSkip = true;
      return;
    }

    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(500);
  });

  beforeEach(function () {
    if (shouldSkip) this.skip();
  });

  afterEach(async () => {
    if (shouldSkip) return;
    try {
      await browser.closeWindow();
    } catch {
      /* ignore */
    }
    try {
      await browser.switchToWindow(mainWindow);
    } catch {
      /* ignore */
    }
  });

  it('应能通过哨兵拓扑测试连接 (RD-SN-001)', async () => {
    const nodes = parseNodeList(SENTINEL_NODES_RAW);
    await openRedisNewConnectionDialog(mainWindow);
    await setConnectionName(SENTINEL_CONN);
    await selectWizardTopology('sentinel');
    await setWizardInputByPlaceholder('mymaster', SENTINEL_MASTER);
    await setWizardMonoTextarea(nodes.join('\n'));
    if (SENTINEL_NODE_PASSWORD) {
      const pwInputs = await $$('input[type="password"]');
      if (pwInputs.length > 0) {
        await pwInputs[0].setValue(SENTINEL_NODE_PASSWORD);
      }
    }
    if (SENTINEL_PASSWORD) {
      await setPasswordField(SENTINEL_PASSWORD);
    }
    await clickTestConnection();
    await assertTestSuccess();
  });
});
