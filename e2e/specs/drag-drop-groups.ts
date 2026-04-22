/**
 * E2E tests for pointer-based drag-and-drop connections between groups.
 *
 * Creates two test groups and one test connection, verifies
 * moving the connection between groups via drag, and cleans up afterwards.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { expandAllGroups } from '../helpers.js';

const GROUP_A = 'DragTestGroupA';
const GROUP_B = 'DragTestGroupB';
const CONN_NAME = 'DragTestConn';
const CONN_ID = 'drag_test_conn_e2e';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (result: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function getGroupForConn(connId: string): Promise<string | undefined> {
  const conns = await invokeBackend<Array<{ id: string; group?: string }>>('get_connections');
  return conns.find((c) => c.id === connId)?.group;
}

/** Simulate a full pointer drag from a source element to a target element */
async function pointerDrag(
  srcSelector: string,
  srcText: string,
  targetGroupName: string,
) {
  return browser.execute(
    (sel: string, text: string, tgName: string) => {
      const items = document.querySelectorAll(sel);
      let src: HTMLElement | null = null;
      for (const item of items) {
        if (item.textContent?.includes(text)) {
          src = item as HTMLElement;
          break;
        }
      }
      if (!src) return 'no-source';

      const targetEl = document.querySelector(`[data-group-name="${tgName}"]`) as HTMLElement;
      if (!targetEl) return 'no-target';

      const srcRect = src.getBoundingClientRect();
      const targetRect = targetEl.getBoundingClientRect();

      const startX = srcRect.left + srcRect.width / 2;
      const startY = srcRect.top + srcRect.height / 2;
      const endX = targetRect.left + targetRect.width / 2;
      const endY = targetRect.top + targetRect.height / 2;

      // pointerdown on source
      src.dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true, cancelable: true, clientX: startX, clientY: startY, button: 0,
      }));

      // pointermove with enough distance to trigger drag
      const steps = 5;
      for (let i = 1; i <= steps; i++) {
        const x = startX + (endX - startX) * (i / steps);
        const y = startY + (endY - startY) * (i / steps);
        window.dispatchEvent(new PointerEvent('pointermove', {
          bubbles: true, cancelable: true, clientX: x, clientY: y,
        }));
      }

      // pointerup on final position
      window.dispatchEvent(new PointerEvent('pointerup', {
        bubbles: true, cancelable: true, clientX: endX, clientY: endY, button: 0,
      }));

      return 'ok';
    },
    srcSelector,
    srcText,
    targetGroupName,
  );
}

describe('拖拽连接到不同分组 - Pointer 事件 (DND)', () => {
  before(async () => {
    await $('input[placeholder="查找连接…"]').waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(500);

    const existingGroups = await invokeBackend<string[]>('get_groups');
    const newGroups = [...existingGroups.filter((g) => g !== GROUP_A && g !== GROUP_B), GROUP_A, GROUP_B];
    await invokeBackend('save_groups', { groups: newGroups });

    const existingConns = await invokeBackend<Array<{ id: string }>>('get_connections');
    if (existingConns.find((c) => c.id === CONN_ID)) {
      await invokeBackend('delete_connection', { id: CONN_ID });
    }

    await invokeBackend('save_connection', {
      config: {
        id: CONN_ID,
        name: CONN_NAME,
        databaseType: 'postgresql',
        host: 'localhost',
        port: 5432,
        database: 'test_dnd',
        username: 'test',
        sslMode: 'disable',
        group: GROUP_A,
      },
    });

    await browser.execute(() => location.reload());
    await $('input[placeholder="查找连接…"]').waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(1500);
  });

  after(async () => {
    try { await invokeBackend('delete_connection', { id: CONN_ID }); } catch { /* ok */ }
    try {
      const groups = await invokeBackend<string[]>('get_groups');
      await invokeBackend('save_groups', { groups: groups.filter((g) => g !== GROUP_A && g !== GROUP_B) });
    } catch { /* ok */ }
    await browser.execute(() => location.reload());
    await browser.pause(1000);
  });

  it('DND-001: 测试分组和连接已创建', async () => {
    const body = await $('body').getText();
    expect(body).toContain(GROUP_A);
    expect(body).toContain(GROUP_B);
    expect(body).toContain(CONN_NAME);
  });

  it('DND-002: 连接初始在 GroupA 中', async () => {
    expect(await getGroupForConn(CONN_ID)).toBe(GROUP_A);
  });

  it('DND-003: data-group-name 属性存在', async () => {
    const names = await browser.execute(() => {
      const els = document.querySelectorAll('[data-group-name]');
      return Array.from(els).map((el) => (el as HTMLElement).dataset.groupName);
    });
    expect(names).toContain(GROUP_A);
    expect(names).toContain(GROUP_B);
  });

  it('DND-004: 通过 pointer 拖拽将连接从 A 移到 B', async () => {
    const result = await pointerDrag('[data-conn-item]', CONN_NAME, GROUP_B);
    expect(result).toBe('ok');
    await browser.pause(2000);
    expect(await getGroupForConn(CONN_ID)).toBe(GROUP_B);
  });

  it('DND-005: 拖拽后 UI 显示连接在 GroupB 中', async () => {
    const inGroupB = await browser.execute((gName: string, cName: string) => {
      const headers = document.querySelectorAll('[data-group-header]');
      for (const h of headers) {
        if (!h.textContent?.includes(gName)) continue;
        const container = h.parentElement;
        if (!container) continue;
        const items = container.querySelectorAll('[data-conn-item]');
        for (const item of items) {
          if (item.textContent?.includes(cName)) return true;
        }
      }
      return false;
    }, GROUP_B, CONN_NAME);
    expect(inGroupB).toBe(true);
  });

  it('DND-006: 通过 pointer 拖拽将连接从 B 移回 A', async () => {
    const result = await pointerDrag('[data-conn-item]', CONN_NAME, GROUP_A);
    expect(result).toBe('ok');
    await browser.pause(2000);
    expect(await getGroupForConn(CONN_ID)).toBe(GROUP_A);
  });

  it('DND-007: 拖拽到当前所在分组不应触发移动', async () => {
    // Already in GROUP_A, drag to GROUP_A
    const result = await pointerDrag('[data-conn-item]', CONN_NAME, GROUP_A);
    expect(result).toBe('ok');
    await browser.pause(1000);
    // Should still be in GROUP_A (no unnecessary save)
    expect(await getGroupForConn(CONN_ID)).toBe(GROUP_A);
  });

  it('DND-008: 拖拽时应显示拖拽幽灵元素', async () => {
    // Start drag: pointerdown + pointermove
    await browser.execute((connName: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      let src: HTMLElement | null = null;
      for (const item of items) {
        if (item.textContent?.includes(connName)) { src = item as HTMLElement; break; }
      }
      if (!src) return;
      const rect = src.getBoundingClientRect();
      const startX = rect.left + rect.width / 2;
      const startY = rect.top + rect.height / 2;

      src.dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true, cancelable: true, clientX: startX, clientY: startY, button: 0,
      }));
      window.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true, cancelable: true, clientX: startX + 30, clientY: startY + 30,
      }));
    }, CONN_NAME);

    // Wait for React to render the ghost
    await browser.pause(300);

    const ghostText = await browser.execute(() => {
      const ghost = document.querySelector('.pointer-events-none.fixed');
      return ghost?.textContent ?? null;
    });

    // Clean up: pointerup
    await browser.execute(() => {
      window.dispatchEvent(new PointerEvent('pointerup', {
        bubbles: true, cancelable: true, clientX: 0, clientY: 0, button: 0,
      }));
    });

    expect(ghostText).toContain(CONN_NAME);
    await browser.pause(500);
  });
});
