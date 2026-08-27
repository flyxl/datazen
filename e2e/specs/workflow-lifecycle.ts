/**
 * Workflow full lifecycle — create, execute, check history, delete.
 * Exercises both GUI and IPC paths.
 *
 * Covers: TC-WF-007 ~ TC-WF-012
 */
import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows, openConnectionWindow } from '../helpers.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
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

const WF_ID = 'e2e-wf-lifecycle';
const WF_NAME = 'E2E-工作流生命周期测试';

describe('工作流完整生命周期 (TC-WF-007~012)', () => {
  let mainWindow: string;
  let dbSessionId: string;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;
    dbSessionId = (await browser.execute(() => {
      const params = new URLSearchParams(window.location.search);
      return params.get('connectionId') || '';
    })) as string;
  });

  after(async () => {
    try {
      await invokeBackend('workflow_delete', { workflowId: WF_ID });
    } catch {
      /* */
    }
    await closeExtraWindows(mainWindow);
  });

  it('TC-WF-007: 创建新工作流应出现在工作流列表', async () => {
    await invokeBackend('workflow_save', {
      workflow: {
        id: WF_ID,
        name: WF_NAME,
        description: 'E2E lifecycle test',
        variables: [],
        steps: [{ type: 'query', id: 'step1', sql: 'SELECT 1 AS wf_step1' }],
      },
    });
    const list = await invokeBackend<Array<{ id: string; name: string }>>('workflow_list');
    expect(list.some((w) => w.id === WF_ID && w.name === WF_NAME)).toBe(true);
  });

  it('TC-WF-008: 工作流详情应包含步骤定义', async () => {
    const wf = await invokeBackend<{ id: string; name: string; steps: unknown[] }>('workflow_get', {
      workflowId: WF_ID,
    });
    expect(wf.name).toBe(WF_NAME);
    expect(wf.steps.length).toBeGreaterThan(0);
  });

  it('TC-WF-009: 执行工作流应产生执行记录', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);
    const result = await invokeBackend<{ success: boolean; totalTimeMs: number }>(
      'workflow_execute',
      {
        workflowId: WF_ID,
        variables: {},
        connectionId: dbSessionId,
      },
    );
    expect(result.success).toBe(true);
  });

  it('TC-WF-010: 执行历史详情应包含步骤结果', async () => {
    const history = await invokeBackend<Array<{ id: string; workflowId: string }>>(
      'workflow_history_list',
      {
        workflowId: WF_ID,
      },
    );
    expect(history.length).toBeGreaterThan(0);
    const detail = await invokeBackend<{ result: { steps: unknown[] } }>('workflow_history_get', {
      historyId: history[0].id,
    });
    expect(detail.result.steps).toBeTruthy();
  });

  it('TC-WF-011: 工作流列表右键不应有"立即运行"菜单项', async () => {
    // Navigate to workflow tab in main window
    await browser.switchToWindow(mainWindow);
    await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const btn = btns.find((b) => {
        const text = b.textContent || '';
        return text.includes('工作流') || text.includes('Workflow');
      });
      if (btn) (btn as HTMLElement).click();
    });
    await browser.pause(1500);

    const rightClicked = await browser.execute(() => {
      const items = document.querySelectorAll('[data-workflow-item], [class*="workflow"] li');
      if (items.length > 0) {
        (items[0] as HTMLElement).dispatchEvent(
          new MouseEvent('contextmenu', { bubbles: true, cancelable: true }),
        );
        return true;
      }
      return false;
    });
    if (rightClicked) {
      await browser.pause(500);
      const body = await $('body').getText();
      const hasRun = body.includes('立即运行') || body.includes('Run Now');
      expect(hasRun).toBe(false);
    }
  });

  it('TC-WF-012: 删除工作流应从列表中移除', async () => {
    await invokeBackend('workflow_delete', { workflowId: WF_ID });
    const list = await invokeBackend<Array<{ id: string }>>('workflow_list');
    expect(list.some((w) => w.id === WF_ID)).toBe(false);
  });
});
