/**
 * E2E tests for the navigator's native HTML5 drag-and-drop connection ordering.
 *
 * The navigator uses `draggable` plus dragstart/dragover/drop/dragend handlers;
 * these tests dispatch that same event lifecycle instead of PointerEvents.
 */
import { expect, browser, $ } from '@wdio/globals';
import { expandAllGroups } from '../helpers.js';
import { t } from '../i18n.js';

const GROUP_A = 'DragTestGroupA';
const GROUP_B = 'DragTestGroupB';
const CONN_A_NAME = 'DragTestConnA';
const CONN_B_NAME = 'DragTestConnB';
const CONN_A_ID = 'drag_test_conn_a_e2e';
const CONN_B_ID = 'drag_test_conn_b_e2e';

type ConnectionSnapshot = { id: string; name: string; group?: string };

type Html5DragStartResult = {
  status: 'ok' | 'no-source' | 'unsupported' | 'failed';
  draggable: boolean;
  payload: string;
  types: string[];
};

type DndWindow = Window & { __datazenDndTransfer?: DataTransfer };

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (result: unknown) => void) => {
      (window as Window & {
        __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => Promise<unknown> };
      })
        .__TAURI_INTERNALS__.invoke(c, JSON.parse(a))
        .then((result) => done(result))
        .catch((error: unknown) => done({ __error: String(error) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function getConnections(): Promise<ConnectionSnapshot[]> {
  return invokeBackend<ConnectionSnapshot[]>('get_connections');
}

async function getConnectionOrder(): Promise<string[]> {
  return (await getConnections()).map((connection) => connection.id);
}

async function waitForRelativeOrder(firstId: string, secondId: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const order = await getConnectionOrder();
      return order.indexOf(firstId) < order.indexOf(secondId);
    },
    { timeout: 10000, interval: 200 },
  );
}

async function getVisibleTestConnectionOrder(): Promise<string[]> {
  return browser.execute((names: string[]) => {
    const visibleNames = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).map(
      (item) => item.dataset.connName,
    );
    return visibleNames.filter((name): name is string => name !== undefined && names.includes(name));
  }, [CONN_A_NAME, CONN_B_NAME]) as Promise<string[]>;
}

async function getRenderedGroupBounds(): Promise<{
  groupAIndex: number;
  groupBIndex: number;
  connectionIndices: number[];
}> {
  return browser.execute((groupA: string, groupB: string, names: string[]) => {
    const rows = Array.from(
      document.querySelectorAll<HTMLElement>('[data-group-header], [data-conn-item]'),
    );
    const groupAIndex = rows.findIndex(
      (row) => row.matches('[data-group-header]') && row.dataset.groupName === groupA,
    );
    const groupBIndex = rows.findIndex(
      (row) => row.matches('[data-group-header]') && row.dataset.groupName === groupB,
    );
    const connectionIndices = rows.flatMap((row, index) =>
      row.matches('[data-conn-item]') && names.includes(row.dataset.connName ?? '') ? [index] : [],
    );
    return { groupAIndex, groupBIndex, connectionIndices };
  }, GROUP_A, GROUP_B, [CONN_A_NAME, CONN_B_NAME]) as Promise<{
    groupAIndex: number;
    groupBIndex: number;
    connectionIndices: number[];
  }>;
}

/** Dispatch the product's actual dragstart event and retain its DataTransfer. */
async function startHtml5Drag(connectionName: string): Promise<Html5DragStartResult> {
  return browser.execute((name: string) => {
    const source = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
      (item) => item.dataset.connName === name,
    );
    if (!source) {
      return { status: 'no-source' as const, draggable: false, payload: '', types: [] };
    }
    if (typeof DataTransfer === 'undefined' || typeof DragEvent === 'undefined') {
      return { status: 'unsupported' as const, draggable: source.draggable, payload: '', types: [] };
    }

    try {
      const dataTransfer = new DataTransfer();
      (window as DndWindow).__datazenDndTransfer = dataTransfer;
      source.dispatchEvent(
        new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer }),
      );
      return {
        status: 'ok' as const,
        draggable: source.draggable,
        payload: dataTransfer.getData('text/plain'),
        types: Array.from(dataTransfer.types),
      };
    } catch {
      return { status: 'failed' as const, draggable: source.draggable, payload: '', types: [] };
    }
  }, connectionName) as Promise<Html5DragStartResult>;
}

/** Dispatch dragover with a coordinate on the requested side of the target row. */
async function dragOverHtml5(connectionName: string, position: 'before' | 'after'): Promise<boolean> {
  return browser.execute((name: string, dropPosition: 'before' | 'after') => {
    const target = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
      (item) => item.dataset.connName === name,
    );
    const dataTransfer = (window as DndWindow).__datazenDndTransfer;
    if (!target || !dataTransfer) return false;

    const rect = target.getBoundingClientRect();
    const clientY = dropPosition === 'before' ? rect.top + 1 : rect.bottom - 1;
    const event = new DragEvent('dragover', {
      bubbles: true,
      cancelable: true,
      clientY,
      dataTransfer,
    });
    target.dispatchEvent(event);
    return event.defaultPrevented;
  }, connectionName, position) as Promise<boolean>;
}

async function dropHtml5(connectionName: string): Promise<boolean> {
  return browser.execute((name: string) => {
    const target = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
      (item) => item.dataset.connName === name,
    );
    const dataTransfer = (window as DndWindow).__datazenDndTransfer;
    if (!target || !dataTransfer) return false;

    const dropEvent = new DragEvent('drop', {
      bubbles: true,
      cancelable: true,
      dataTransfer,
    });
    target.dispatchEvent(dropEvent);

    // Complete the native lifecycle. The React drop handler also clears its
    // internal state, but the browser sends dragend after drop as well.
    const source = document.querySelector<HTMLElement>(
      `[data-conn-item][data-conn-name="${dataTransfer.getData('text/plain')}"]`,
    );
    source?.dispatchEvent(new DragEvent('dragend', { bubbles: true, dataTransfer }));
    delete (window as DndWindow).__datazenDndTransfer;
    return dropEvent.defaultPrevented;
  }, connectionName) as Promise<boolean>;
}

async function endHtml5Drag(connectionName: string): Promise<void> {
  await browser.execute((name: string) => {
    const source = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
      (item) => item.dataset.connName === name,
    );
    const dataTransfer = (window as DndWindow).__datazenDndTransfer;
    source?.dispatchEvent(new DragEvent('dragend', { bubbles: true, dataTransfer }));
    delete (window as DndWindow).__datazenDndTransfer;
  }, connectionName);
}

describe('连接分组中的 HTML5 拖拽排序 (DND)', () => {
  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(500);

    const existingGroups = await invokeBackend<string[]>('get_groups');
    const newGroups = [...existingGroups.filter((g) => g !== GROUP_A && g !== GROUP_B), GROUP_A, GROUP_B];
    await invokeBackend('save_groups', { groups: newGroups });

    const existingConns = await getConnections();
    for (const id of [CONN_A_ID, CONN_B_ID]) {
      if (existingConns.some((connection) => connection.id === id)) {
        await invokeBackend('delete_connection', { id });
      }
    }

    const connectionBase = {
      databaseType: 'postgresql',
      host: 'localhost',
      port: 5432,
      database: 'test_dnd',
      username: 'test',
      sslMode: 'disable',
      group: GROUP_A,
    };
    await invokeBackend('save_connection', {
      config: { ...connectionBase, id: CONN_A_ID, name: CONN_A_NAME },
    });
    await invokeBackend('save_connection', {
      config: { ...connectionBase, id: CONN_B_ID, name: CONN_B_NAME },
    });

    await browser.execute(() => location.reload());
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(1500);
  });

  after(async () => {
    try {
      await invokeBackend('delete_connection', { id: CONN_A_ID });
      await invokeBackend('delete_connection', { id: CONN_B_ID });
    } catch { /* best-effort cleanup */ }
    try {
      const groups = await invokeBackend<string[]>('get_groups');
      await invokeBackend('save_groups', { groups: groups.filter((g) => g !== GROUP_A && g !== GROUP_B) });
    } catch { /* best-effort cleanup */ }
    await browser.execute(() => location.reload());
    await browser.pause(1000);
  });

  it('DND-001: 测试分组和连接已创建', async () => {
    const body = await $('body').getText();
    expect(body).toContain(GROUP_A);
    expect(body).toContain(GROUP_B);
    expect(body).toContain(CONN_A_NAME);
    expect(body).toContain(CONN_B_NAME);
  });

  it('DND-002: 测试连接初始都在 GroupA 且 A 排在 B 前', async () => {
    const connections = await getConnections();
    expect(connections.find((connection) => connection.id === CONN_A_ID)?.group).toBe(GROUP_A);
    expect(connections.find((connection) => connection.id === CONN_B_ID)?.group).toBe(GROUP_A);
    await waitForRelativeOrder(CONN_A_ID, CONN_B_ID);
  });

  it('DND-003: data-group-name 属性存在', async () => {
    const names = await browser.execute(() =>
      Array.from(document.querySelectorAll<HTMLElement>('[data-group-name]')).map(
        (element) => element.dataset.groupName,
      ),
    ) as Promise<Array<string | undefined>>;
    expect(names).toContain(GROUP_A);
    expect(names).toContain(GROUP_B);
  });

  it('DND-004: 通过 HTML5 拖拽将连接 A 移到连接 B 后方', async () => {
    const start = await startHtml5Drag(CONN_A_NAME);
    expect(start.status).toBe('ok');
    expect(start.draggable).toBe(true);
    expect(start.payload).toBe(CONN_A_ID);

    expect(await dragOverHtml5(CONN_B_NAME, 'after')).toBe(true);
    await browser.pause(100);
    expect(await dropHtml5(CONN_B_NAME)).toBe(true);
    await waitForRelativeOrder(CONN_B_ID, CONN_A_ID);
  });

  it('DND-005: 拖拽后 UI 在 GroupA 中显示 B、A 的顺序', async () => {
    expect(await getVisibleTestConnectionOrder()).toEqual([CONN_B_NAME, CONN_A_NAME]);
    const bounds = await getRenderedGroupBounds();
    expect(bounds.groupAIndex).toBeGreaterThanOrEqual(0);
    expect(bounds.groupBIndex).toBeGreaterThan(bounds.groupAIndex);
    expect(Math.min(...bounds.connectionIndices)).toBeGreaterThan(bounds.groupAIndex);
    expect(Math.max(...bounds.connectionIndices)).toBeLessThan(bounds.groupBIndex);
  });

  it('DND-006: 通过 HTML5 拖拽将连接 A 移回 B 前方', async () => {
    const start = await startHtml5Drag(CONN_A_NAME);
    expect(start.status).toBe('ok');
    expect(await dragOverHtml5(CONN_B_NAME, 'before')).toBe(true);
    await browser.pause(100);
    expect(await dropHtml5(CONN_B_NAME)).toBe(true);
    await waitForRelativeOrder(CONN_A_ID, CONN_B_ID);
    expect(await getVisibleTestConnectionOrder()).toEqual([CONN_A_NAME, CONN_B_NAME]);
  });

  it('DND-007: 拖拽到当前所在连接不应改变顺序', async () => {
    const before = await getConnectionOrder();
    const start = await startHtml5Drag(CONN_A_NAME);
    expect(start.status).toBe('ok');
    expect(await dragOverHtml5(CONN_A_NAME, 'after')).toBe(false);
    await browser.pause(100);
    expect(await dropHtml5(CONN_A_NAME)).toBe(true);
    await browser.pause(300);
    expect(await getConnectionOrder()).toEqual(before);
    expect(await getVisibleTestConnectionOrder()).toEqual([CONN_A_NAME, CONN_B_NAME]);
  });

  it('DND-008: dragstart 应使用原生 DataTransfer 传递连接 ID', async () => {
    const start = await startHtml5Drag(CONN_A_NAME);
    expect(start.status).toBe('ok');
    expect(start.draggable).toBe(true);
    expect(start.payload).toBe(CONN_A_ID);
    expect(start.types).toContain('text/plain');
    await endHtml5Drag(CONN_A_NAME);
  });
});
