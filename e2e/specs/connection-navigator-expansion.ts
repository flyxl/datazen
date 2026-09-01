import { expect, browser, $ } from '@wdio/globals';
import { expandAllGroups } from '../helpers.js';
import { t } from '../i18n.js';

const GROUP_A = 'E2E-Navigator-Group-A';
const GROUP_B = 'E2E-Navigator-Group-B';
const RECENT_ID = 'e2e-navigator-recent';
const OTHER_ID = 'e2e-navigator-other';
const RECENT_NAME = 'E2E Navigator Recent';
const OTHER_NAME = 'E2E Navigator Other';

type ConnectionConfig = {
  id: string;
  name: string;
  databaseType: 'postgresql';
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  group: string;
  sslMode: 'disable';
  lastConnectedAt?: string;
};

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (command: string, serializedArgs: string, done: (value: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(command, JSON.parse(serializedArgs))
        .then((value: unknown) => done(value))
        .catch((error: unknown) => done({ __error: String(error) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error(String((result as Record<string, unknown>).__error));
  }
  return result as T;
}

function connectionConfig(
  id: string,
  name: string,
  group: string,
  lastConnectedAt?: string,
): ConnectionConfig {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    database: process.env.E2E_PG_DB || 'postgres',
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    group,
    sslMode: 'disable',
    ...(lastConnectedAt ? { lastConnectedAt } : {}),
  };
}

async function dispatchRowAction(
  group: string,
  name: string,
  action: 'click' | 'doubleClick',
): Promise<boolean> {
  return browser.execute(
    (sectionGroup: string, connName: string, eventName: string) => {
      const row = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
        (item) => item.dataset.connGroup === sectionGroup && item.dataset.connName === connName,
      );
      if (!row) return false;
      if (eventName === 'click') {
        row.querySelector<HTMLElement>('button')?.click();
      } else {
        row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      }
      return true;
    },
    group,
    name,
    action,
  );
}

async function waitForRow(group: string, name: string): Promise<void> {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (sectionGroup: string, connName: string) =>
          Boolean(
            Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
              (item) =>
                item.dataset.connGroup === sectionGroup && item.dataset.connName === connName,
            ),
          ),
        group,
        name,
      ),
    { timeout: 10000, timeoutMsg: `等待连接行 ${group}/${name} 超时` },
  );
}

async function getExpanded(group: string, name: string): Promise<string | null> {
  return browser.execute(
    (sectionGroup: string, connName: string) => {
      const row = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
        (item) => item.dataset.connGroup === sectionGroup && item.dataset.connName === connName,
      );
      return row?.querySelector('button')?.getAttribute('aria-expanded') ?? null;
    },
    group,
    name,
  );
}

async function waitForConnected(group: string, name: string): Promise<void> {
  const connectedTitle = t('conn.connected');
  await browser.waitUntil(
    async () => {
      const status = await browser.execute(
        (sectionGroup: string, connName: string, title: string) => {
          const row = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
            (item) => item.dataset.connGroup === sectionGroup && item.dataset.connName === connName,
          );
          return row?.querySelector(`[title="${title}"]`) != null;
        },
        group,
        name,
        connectedTitle,
      );
      return status;
    },
    { timeout: 30000, timeoutMsg: `等待连接 ${name} 建立连接超时` },
  );
}

async function waitForConnectingWithoutExpandedConnection(
  group: string,
  name: string,
): Promise<void> {
  const connectingTitle = t('conn.connecting');
  await browser.waitUntil(
    async () =>
      browser.execute(
        (sectionGroup: string, connName: string, title: string) => {
          const rows = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]'));
          const row = rows.find(
            (item) => item.dataset.connGroup === sectionGroup && item.dataset.connName === connName,
          );
          const isConnecting = row?.querySelector(`[title="${title}"]`) != null;
          const hasExpandedConnection = rows.some(
            (item) => item.querySelector('button')?.getAttribute('aria-expanded') === 'true',
          );
          return isConnecting && !hasExpandedConnection;
        },
        group,
        name,
        connectingTitle,
      ),
    { timeout: 10000, timeoutMsg: `连接 ${name} 未进入连接中状态` },
  );
}

describe('连接导航树单连接展开 (NAV-EXPAND)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();

    const existingGroups = await invokeBackend<string[]>('get_groups');
    await invokeBackend('save_groups', {
      groups: [
        ...existingGroups.filter((group) => group !== GROUP_A && group !== GROUP_B),
        GROUP_A,
        GROUP_B,
      ],
    });
    for (const id of [RECENT_ID, OTHER_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* best effort */
      }
    }

    await invokeBackend('save_connection', {
      config: connectionConfig(RECENT_ID, RECENT_NAME, GROUP_A, new Date().toISOString()),
    });
    await invokeBackend('save_connection', {
      config: connectionConfig(OTHER_ID, OTHER_NAME, GROUP_B),
    });

    await browser.execute(() => location.reload());
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await expandAllGroups();
    await waitForRow('__recent__', RECENT_NAME);
    await waitForRow(GROUP_A, RECENT_NAME);
    await waitForRow(GROUP_B, OTHER_NAME);
  });

  after(async () => {
    await browser.switchToWindow(mainWindow);
    for (const id of [RECENT_ID, OTHER_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* best effort */
      }
    }
    try {
      const groups = await invokeBackend<string[]>('get_groups');
      await invokeBackend('save_groups', {
        groups: groups.filter((group) => group !== GROUP_A && group !== GROUP_B),
      });
    } catch {
      /* best effort */
    }
    await browser.execute(() => location.reload());
    await browser.pause(800);
  });

  it('第一次打开最近连接时不会展开其他分组的连接', async () => {
    expect(await getExpanded('__recent__', RECENT_NAME)).toBe(null);
    expect(await getExpanded(GROUP_A, RECENT_NAME)).toBe(null);
    expect(await getExpanded(GROUP_B, OTHER_NAME)).toBe(null);

    expect(await dispatchRowAction('__recent__', RECENT_NAME, 'doubleClick')).toBe(true);
    await waitForConnected('__recent__', RECENT_NAME);
    await browser.waitUntil(async () => (await getExpanded('__recent__', RECENT_NAME)) === 'true', {
      timeout: 30000,
      timeoutMsg: '最近连接未展开',
    });

    expect(await getExpanded(GROUP_A, RECENT_NAME)).not.toBe('true');
    expect(await getExpanded(GROUP_B, OTHER_NAME)).not.toBe('true');
  });

  it('从一个分组切换到另一个连接时始终只保留一个展开项', async () => {
    expect(await dispatchRowAction(GROUP_A, RECENT_NAME, 'click')).toBe(true);
    await browser.waitUntil(async () => (await getExpanded(GROUP_A, RECENT_NAME)) === 'true', {
      timeout: 10000,
      timeoutMsg: '分组内连接未展开',
    });
    expect(await getExpanded('__recent__', RECENT_NAME)).toBe('false');

    // Reproduce the slow-connect race from a fully collapsed tree. While the
    // target is connecting, no unrelated shortcut may be expanded.
    expect(await dispatchRowAction(GROUP_A, RECENT_NAME, 'click')).toBe(true);
    await browser.waitUntil(async () => (await getExpanded(GROUP_A, RECENT_NAME)) === 'false', {
      timeout: 10000,
      timeoutMsg: '连接未收起，无法验证慢连接时序',
    });

    await browser.execute((connectionId: string) => {
      (
        window as Window & {
          __DATAZEN_E2E_CONNECT_DELAY_MS__?: Record<string, number>;
        }
      ).__DATAZEN_E2E_CONNECT_DELAY_MS__ = { [connectionId]: 1200 };
    }, OTHER_ID);
    try {
      expect(await dispatchRowAction(GROUP_B, OTHER_NAME, 'doubleClick')).toBe(true);
      await waitForConnectingWithoutExpandedConnection(GROUP_B, OTHER_NAME);
      await waitForConnected(GROUP_B, OTHER_NAME);
    } finally {
      await browser.execute(() => {
        delete (
          window as Window & {
            __DATAZEN_E2E_CONNECT_DELAY_MS__?: Record<string, number>;
          }
        ).__DATAZEN_E2E_CONNECT_DELAY_MS__;
      });
    }
    await browser.waitUntil(async () => (await getExpanded(GROUP_B, OTHER_NAME)) === 'true', {
      timeout: 30000,
      timeoutMsg: '其他分组连接成功后未自动展开',
    });

    expect(await getExpanded('__recent__', RECENT_NAME)).toBe('false');
    expect(await getExpanded(GROUP_A, RECENT_NAME)).toBe('false');
    expect(await getExpanded(GROUP_B, OTHER_NAME)).toBe('true');
  });
});
