import { browser, $ } from '@wdio/globals';
import {
  clickNewConnectionSave,
  clickNewConnectionTest,
  invokeBackend,
  selectNewConnectionDriver,
} from '../../helpers.js';
import { t } from '../../i18n.js';
import { seedDefaultPgConnection } from '../../lib/testDataLifecycle.js';

export interface JourneyConnection {
  id: string;
  name: string;
}

export interface PostgresFormOptions {
  host?: string;
  port?: string;
  database?: string;
  username?: string;
  password?: string;
}

export const PG_FORM_DEFAULTS = {
  host: process.env.E2E_PG_HOST || '127.0.0.1',
  port: process.env.E2E_PG_PORT || '5432',
  database: process.env.E2E_PG_DB || 'postgres',
  username: process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_PG_PASSWORD || '',
};

export async function getJourneyConnections(): Promise<JourneyConnection[]> {
  return invokeBackend<JourneyConnection[]>('get_connections');
}

export async function deleteAllJourneyConnections(): Promise<void> {
  const connections = await getJourneyConnections();
  for (const connection of connections) {
    await invokeBackend('delete_connection', { id: connection.id });
  }
}

export async function deleteJourneyConnectionsByName(...names: string[]): Promise<void> {
  const wanted = new Set(names);
  const connections = await getJourneyConnections();
  for (const connection of connections) {
    if (wanted.has(connection.name)) {
      await invokeBackend('delete_connection', { id: connection.id });
    }
  }
}

export async function restoreDefaultJourneyConnection(): Promise<void> {
  await seedDefaultPgConnection(browser);
}

export async function setPostgresPort(value: string): Promise<void> {
  const dialog = await $('[data-testid="new-connection-dialog"]');
  const inputs = await dialog.$$('input');
  for (const input of inputs) {
    const type = (await input.getAttribute('type')) || 'text';
    const placeholder = (await input.getAttribute('placeholder')) || '';
    if (type !== 'password' && placeholder === '') {
      await input.clearValue();
      await input.setValue(value);
      return;
    }
  }
  throw new Error('未找到连接端口输入框');
}

export async function fillPostgresConnectionForm(
  name: string,
  options: PostgresFormOptions = {},
): Promise<void> {
  const values = { ...PG_FORM_DEFAULTS, ...options };
  await selectNewConnectionDriver('postgresql');
  await browser.pause(250);

  const nameInput = await $('input[placeholder="例如：主数据库"]');
  await nameInput.clearValue();
  await nameInput.setValue(name);

  const hostInput = await $('input[placeholder="prod-db.example.com"]');
  await hostInput.clearValue();
  if (values.host) await hostInput.setValue(values.host);

  await setPostgresPort(values.port);

  const databaseInput = await $('input[placeholder="myapp_production"]');
  await databaseInput.clearValue();
  if (values.database) await databaseInput.setValue(values.database);

  const usernameInput = await $('input[placeholder="postgres"]');
  await usernameInput.clearValue();
  if (values.username) await usernameInput.setValue(values.username);

  const passwordInput = await $('input[type="password"]');
  await passwordInput.clearValue();
  if (values.password) await passwordInput.setValue(values.password);
}

export async function waitForConnectionTestResult(
  expected: 'success' | 'failure',
  timeout = expected === 'failure' ? 45000 : 30000,
): Promise<void> {
  const message = expected === 'success' ? t('newConn.testSuccess') : t('newConn.testFailed');
  await browser.waitUntil(
    async () => {
      const body = await $('[data-testid="new-connection-dialog"]').getText();
      if (expected === 'success') return body.includes(message);
      return body.includes(message) || /failed|refused|error|失败|拒绝/i.test(body);
    },
    { timeout, timeoutMsg: `等待连接测试${expected === 'success' ? '成功' : '失败'}超时` },
  );
}

export async function testAndSavePostgresConnection(name: string): Promise<void> {
  await clickNewConnectionTest();
  await waitForConnectionTestResult('success');
  await clickNewConnectionSave();
  await browser.waitUntil(
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    { timeout: 10000, timeoutMsg: '保存连接后弹窗未关闭' },
  );
  await waitForConnectionListed(name);
}

export async function waitForConnectionListed(name: string, timeout = 10000): Promise<void> {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (connectionName: string) =>
          Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).some(
            (item) => item.dataset.connName === connectionName,
          ),
        name,
      ),
    { timeout, timeoutMsg: `连接列表未出现 "${name}"` },
  );
}

export async function openConnectionContextMenu(name: string): Promise<void> {
  const opened = await browser.execute((connectionName: string) => {
    const row = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
      (item) => item.dataset.connName === connectionName,
    );
    if (!row) return false;
    const rect = row.getBoundingClientRect();
    row.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + Math.min(rect.width / 2, 120),
        clientY: rect.top + rect.height / 2,
      }),
    );
    return true;
  }, name);
  if (!opened) throw new Error(`未找到连接 "${name}" 的导航行`);
  await $('[data-testid="web-context-menu"]').waitForDisplayed({ timeout: 5000 });
}
