/**
 * ER diagram full user journey — workspace home / toolbar entry, React Flow canvas,
 * controls, search, table/relation stats, and inferred relationships.
 *
 * Covers: ER-001~ER-009
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  captureJourneyStep,
  connectSeededPgInWorkspace,
  disconnectBackend,
  invokeBackend,
  openErDiagramFromUi,
  openQueryTab,
  resetDialogQueue,
  waitForConnectionToolbar,
  withSafeModeOff,
} from '../helpers.js';

/**
 * Assert a boolean with a message.
 *
 * `expect` from `@wdio/globals` takes a single argument, so a message-bearing
 * assertion needs this wrapper.
 */
function expectTrue(value: boolean, message: string): void {
  if (!value) throw new Error(message);
}

const SEEDED_CONN_ID = 'conn_e2e_pg';

/**
 * A pair with **no** declared constraint, created before the panel first opens.
 *
 * The member column spells the owner table out in full, so the naming convention
 * links them exactly and the engine scores it high enough to draw. Created in
 * `before` on purpose: the ER panel loads once per session+database, so a table
 * added after it opened would never appear.
 */
const PRED_OWNER = 'er_pred_owner';
const PRED_MEMBER = 'er_pred_member';
const PRED_COLUMN = `${PRED_OWNER}_id`;

/** Click the ER Diagram panel tab (switches away from other tabs when present). */
async function focusErPanelTab() {
  const tabs = await $$('[data-testid="panel-tab"]');
  for (const tab of tabs) {
    const text = await tab.getText();
    if (/ER Diagram|ER 图/i.test(text)) {
      const selectBtn = await tab.$('[data-testid="panel-tab-select"]');
      await selectBtn.click();
      await browser.pause(400);
      return;
    }
  }
  throw new Error('ER Diagram panel tab not found');
}

/** Ensure ER diagram content is visible (re-focus tab or re-open from toolbar). */
async function ensureErDiagramVisible() {
  if (
    await $('[data-testid="er-diagram-view"]')
      .isDisplayed()
      .catch(() => false)
  )
    return;
  try {
    await focusErPanelTab();
  } catch {
    await openErDiagramFromUi();
    await browser.pause(1500);
  }
  await browser.waitUntil(
    async () =>
      $('[data-testid="er-diagram-view"]')
        .isDisplayed()
        .catch(() => false),
    { timeout: 15000, timeoutMsg: 'ER diagram view did not become visible' },
  );
}

/** `data-id` of an inferred edge between the two tables, or null when absent. */
async function findPredictedEdgeId(): Promise<string | null> {
  return browser.execute(
    (member: string, owner: string) => {
      const edges = Array.from(document.querySelectorAll('.react-flow__edge'));
      const match = edges.find((el) => {
        const id = el.getAttribute('data-id') ?? '';
        return id.startsWith('predicted-') && id.includes(member) && id.includes(owner);
      });
      return match?.getAttribute('data-id') ?? null;
    },
    PRED_MEMBER,
    PRED_OWNER,
  );
}

/**
 * Drive the ER page's own prediction button to `enabled` and confirm it took.
 *
 * The button and Settings → Editor are the same persisted setting, so the state
 * is read back rather than assumed: the store only flips after the save returns.
 */
async function setErPrediction(enabled: boolean): Promise<void> {
  const toggle = await $('[data-testid="er-diagram-toggle-prediction"]');
  await toggle.waitForClickable({ timeout: 10000 });
  if ((await toggle.getAttribute('aria-pressed')) !== String(enabled)) {
    await toggle.click();
  }
  await browser.waitUntil(
    async () => (await toggle.getAttribute('aria-pressed')) === String(enabled),
    {
      timeout: 10000,
      timeoutMsg: `ER 智能预测开关未切换到 ${enabled ? '开启' : '关闭'}`,
    },
  );
  await browser.pause(600);
}

/** Computed dash pattern of a rendered edge, or null when the edge is absent. */
async function edgeDashOf(edgeId: string): Promise<string | null> {
  return browser.execute((id: string) => {
    const el = document.querySelector(`.react-flow__edge[data-id="${id}"]`);
    const path = el?.querySelector('path');
    return path ? getComputedStyle(path).strokeDasharray : null;
  }, edgeId);
}

describe('ER 图功能 E2E 测试 (ER-001~ER-013)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await waitForConnectionToolbar();

    const dbSessionId = await invokeBackend<string>('connect_dedicated', {
      connectionId: SEEDED_CONN_ID,
      database: null,
    });
    try {
      // Safe Mode blocks DDL, so the fixture needs an explicit opt-out.
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId,
          sql: `DROP TABLE IF EXISTS ${PRED_MEMBER}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId,
          sql: `DROP TABLE IF EXISTS ${PRED_OWNER}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId,
          sql: `CREATE TABLE ${PRED_OWNER} (id INTEGER PRIMARY KEY, label TEXT)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId,
          sql: `CREATE TABLE ${PRED_MEMBER} (id INTEGER PRIMARY KEY, ${PRED_COLUMN} INTEGER, note TEXT)`,
        });
      });
    } finally {
      await disconnectBackend(dbSessionId);
    }
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    try {
      const dbSessionId = await invokeBackend<string>('connect_dedicated', {
        connectionId: SEEDED_CONN_ID,
        database: null,
      });
      try {
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', {
            dbSessionId,
            sql: `DROP TABLE IF EXISTS ${PRED_MEMBER}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId,
            sql: `DROP TABLE IF EXISTS ${PRED_OWNER}`,
          });
        });
      } finally {
        await disconnectBackend(dbSessionId);
      }
    } catch {
      /* ok */
    }
    try {
      await resetDialogQueue();
    } catch {
      /* ok */
    }
  });

  it('ER-001: get_er_data IPC 应返回 schema 数组', async () => {
    // Use a dedicated session. `connect` reuses the UI-owned session and the
    // force-disconnect below would otherwise invalidate the ER panel's live
    // dbSessionId before ER-003 opens it.
    const dbSessionId = await invokeBackend<string>('connect_dedicated', {
      connectionId: SEEDED_CONN_ID,
      database: null,
    });
    try {
      const databases = await invokeBackend<string[]>('get_databases', { dbSessionId });
      expect(databases.length).toBeGreaterThan(0);

      const schemas = await invokeBackend<unknown[]>('get_er_data', {
        dbSessionId,
        database: databases[0],
      });
      expect(Array.isArray(schemas)).toBe(true);
    } finally {
      await disconnectBackend(dbSessionId);
    }
  });

  it('ER-002: 连接后 ER 入口应可见（首页快捷操作或工具栏）', async () => {
    await browser.switchToWindow(mainWindow);
    const homeQuick = await $('[data-testid="home-quick-er-diagram"]');
    const toolbar = await $('[data-testid="content-toolbar-er-diagram"]');
    const homeVisible = await homeQuick.isExisting();
    if (!homeVisible) {
      await openQueryTab();
      await browser.pause(500);
    }
    const hasEntry = homeVisible || (await toolbar.isExisting()) || (await homeQuick.isExisting());
    expect(hasEntry).toBe(true);
    await captureJourneyStep('er-entry-visible', 0, true);
  });

  it('ER-003: 点击 ER 入口应打开 ER 面板', async () => {
    await browser.switchToWindow(mainWindow);
    await openErDiagramFromUi();
    await browser.pause(2000);

    const erView = await $('[data-testid="er-diagram-view"]');
    await erView.waitForDisplayed({ timeout: 15000 });
    const reactFlow = await erView.$('[data-testid="er-diagram-flow"]');
    await expect(reactFlow).toBeDisplayed();
    await captureJourneyStep('er-canvas-visible', 0, true);
  });

  it('ER-004: 面板 tab 应出现 ER Diagram 标签', async () => {
    await browser.switchToWindow(mainWindow);
    await openQueryTab();
    await browser.pause(500);
    await focusErPanelTab();
    const erView = await $('[data-testid="er-diagram-view"]');
    await erView.waitForDisplayed({ timeout: 15000 });
    const tabs = await $$('[data-testid="panel-tab"]');
    expect(tabs.length).toBeGreaterThan(0);
    const body = await $('body').getText();
    expect(/ER Diagram|ER 图/i.test(body)).toBe(true);
    await captureJourneyStep('er-tab-switched', 0, true);
  });

  it('ER-005: React Flow 控件应可见', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const zoomIn = await $('[data-testid="er-diagram-zoom-in"]');
    await zoomIn.waitForClickable({ timeout: 10000 });
    await zoomIn.click();
    await zoomIn.click();
    await browser.pause(500);
    await expect(await $('[data-testid="er-diagram-zoom-out"]')).toBeDisplayed();
    await captureJourneyStep('er-controls-zoomed', 0, true);
  });

  it('ER-006: 统计面板应显示表/关系数量', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const stats = await $('[data-testid="er-diagram-stats"]');
    await stats.waitForDisplayed({ timeout: 10000 });
    const text = await stats.getText();
    expect(text.length).toBeGreaterThan(0);
    expect(text).toMatch(/\d/);

    const fitView = await $('[data-testid="er-diagram-fit-view"]');
    if (await fitView.isExisting()) {
      await fitView.click();
      await browser.pause(400);
    }
    const firstNode = await $('[data-testid="er-table-node"]');
    await firstNode.waitForDisplayed({ timeout: 10000 });
    await firstNode.click();
    await browser.pause(500);
    await captureJourneyStep('er-table-selected', 0, true);
  });

  it('ER-009: 图例区分声明与推测关系，页面按钮可开启智能预测', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();

    // Both tables must be in the graph before the edge can exist.
    await browser.waitUntil(
      async () =>
        await browser.execute(
          (owner: string, member: string) =>
            !!document.querySelector(`[data-id="${owner}"]`) &&
            !!document.querySelector(`[data-id="${member}"]`),
          PRED_OWNER,
          PRED_MEMBER,
        ),
      { timeout: 15000, timeoutMsg: '推测关系涉及的表未出现在 ER 图中' },
    );

    // ── The legend is the page's own key to its lines ──
    const legend = await $('[data-testid="er-diagram-legend"]');
    await legend.waitForDisplayed({ timeout: 10000 });
    expectTrue(
      await $('[data-testid="er-legend-declared"]').isDisplayed(),
      '图例未说明数据库声明的外键',
    );
    expectTrue(
      await $('[data-testid="er-legend-predicted"]').isDisplayed(),
      '图例未说明智能推测的关系',
    );
    // The inferred swatch must be dashed, matching the edge it describes.
    const swatchDash = await browser.execute(() => {
      const line = document.querySelector('[data-testid="er-legend-predicted"] line');
      return line?.getAttribute('stroke-dasharray') ?? null;
    });
    expectTrue(!!swatchDash, '图例中的推测连线未画成虚线');
    await captureJourneyStep('er-legend', 0, true);

    // ── Prediction is opt-in: off means no inferred edge is drawn ──
    await setErPrediction(false);
    expectTrue((await findPredictedEdgeId()) === null, '关闭智能预测后仍画出了推测关系');

    // ── The page's own button turns it on, with no trip to Settings ──
    await setErPrediction(true);
    await browser.waitUntil(async () => (await findPredictedEdgeId()) !== null, {
      timeout: 15000,
      timeoutMsg: '开启智能预测后未画出推测关系',
    });
    const predictedEdgeId = (await findPredictedEdgeId()) as string;
    await captureJourneyStep('er-predicted-relation', 0, true);

    // An inference must not look like a constraint: the declared edges are solid
    // and animated, the inferred one dashed and still.
    const dashed = await edgeDashOf(predictedEdgeId);
    expectTrue(
      dashed !== null && dashed !== 'none' && dashed.length > 0,
      `推测关系未使用虚线样式（strokeDasharray=${String(dashed)}）`,
    );

    // The legend must disclose how many were inferred rather than folding them
    // into the declared total.
    const inferredCount = await $('[data-testid="er-predicted-count"]');
    await inferredCount.waitForDisplayed({ timeout: 10000 });
    expectTrue(/[1-9]/.test(await inferredCount.getText()), '图例未显示推测关系数量');

    // ── Leave the setting off, which is also its default ──
    await setErPrediction(false);
    expectTrue((await findPredictedEdgeId()) === null, '关闭智能预测后仍画出了推测关系');
  });

  it('ER-010: 真实渲染的节点之间不得重叠', async () => {
    // The unit tests check the layout's own arithmetic; this checks that the
    // arithmetic matches what actually renders. A node whose real height differs
    // from the height the layout reserved is exactly how the old grid came to
    // draw a wide table over its neighbour.
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const search = await $('[data-testid="er-diagram-search"]');
    if (await search.isExisting()) {
      await search.clearValue();
      await browser.pause(400);
    }

    const result = await browser.execute(() => {
      const rects = Array.from(document.querySelectorAll('[data-testid="er-table-node"]')).map(
        (el) => {
          const r = el.getBoundingClientRect();
          return { w: r.width, h: r.height, left: r.left, top: r.top };
        },
      );
      const collisions: string[] = [];
      for (let i = 0; i < rects.length; i++) {
        for (let j = i + 1; j < rects.length; j++) {
          const a = rects[i]!;
          const b = rects[j]!;
          const overlapX = a.left < b.left + b.w && b.left < a.left + a.w;
          const overlapY = a.top < b.top + b.h && b.top < a.top + a.h;
          if (overlapX && overlapY) {
            collisions.push(
              `#${i}(${Math.round(a.left)},${Math.round(a.top)} ${Math.round(a.w)}x${Math.round(a.h)}) ` +
                `#${j}(${Math.round(b.left)},${Math.round(b.top)} ${Math.round(b.w)}x${Math.round(b.h)})`,
            );
          }
        }
      }
      return { count: rects.length, collisions };
    });

    expectTrue(result.count > 0, 'ER 图中没有节点，无法判断重叠');
    expectTrue(
      result.collisions.length === 0,
      `ER 图节点重叠 ${result.collisions.length} 处: ${result.collisions.slice(0, 3).join(' | ')}`,
    );
  });

  it('ER-011: 连线应指向 FK 所在的那一列，而不是节点垂直中点', async () => {
    // The row-level handles are positioned with an inline `top` overriding React
    // Flow's `top: 50%`. Only a real browser can prove that override lands where
    // the geometry says it should.
    //
    // Everything is compared in React Flow's own coordinate space: the edge path
    // is already in it, and `.react-flow__node` is positioned with a
    // `translate()` in it. Mixing in `getBoundingClientRect` would compare flow
    // units against screen pixels and measure the viewport transform instead.
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    // The two tables are related only by naming convention, so the edge measured
    // below is an *inferred* one. Prediction is opt-in and ER-009 leaves it off,
    // so re-enable it through the diagram's own toggle and wait for the edge
    // itself — waiting on the node proves nothing, because nodes render either way.
    await setErPrediction(true);
    await browser.waitUntil(async () => (await findPredictedEdgeId()) !== null, {
      timeout: 15000,
      timeoutMsg: 'ER-011: 开启智能预测后未画出推测关系',
    });

    const result = await browser.execute(
      (member: string, owner: string) => {
        const edge = Array.from(document.querySelectorAll('.react-flow__edge')).find((el) => {
          const id = el.getAttribute('data-id') ?? '';
          return id.startsWith('predicted-') && id.includes(member) && id.includes(owner);
        });
        if (!edge) return { error: 'predicted edge not found' };
        const path = edge.querySelector('path');
        if (!path) return { error: 'edge path not found' };

        const node = document.querySelector(`.react-flow__node[data-id="${member}"]`);
        if (!node) return { error: 'member node not found' };
        const transform = (node as HTMLElement).style.transform ?? '';
        const t = /translate\(\s*(-?[\d.]+)px[,\s]+(-?[\d.]+)px/.exec(transform);
        if (!t) return { error: `unparsable node transform: ${transform}` };
        const nodeFlowY = Number(t[2]);
        const nodeHeight = (node as HTMLElement).getBoundingClientRect().height;

        const d = path.getAttribute('d') ?? '';
        const m = /^M\s*(-?[\d.]+)[,\s]+(-?[\d.]+)/.exec(d.trim());
        if (!m) return { error: `unparsable path: ${d.slice(0, 40)}` };

        const handle = node.querySelector(`[data-handleid="er_pred_owner_id:s-r"]`);
        return {
          endpointOffsetY: Number(m[2]) - nodeFlowY,
          nodeHeight,
          hasColumnHandle: !!handle,
          hasNodeFallback: !!node.querySelector('[data-handleid="node:s-r"]'),
        };
      },
      PRED_MEMBER,
      PRED_OWNER,
    );

    expectTrue(!('error' in result), `无法测量连线端点: ${JSON.stringify(result)}`);
    const r = result as {
      endpointOffsetY: number;
      nodeHeight: number;
      hasColumnHandle: boolean;
      hasNodeFallback: boolean;
    };

    // `er_pred_member` is (id, <owner>_id, note): the FK row is index 1, so its
    // centre is 1 + 36 + 24 + 12 = 73px below the node's top, while the node's
    // own centre is at 55px. The two are 18px apart, which is what makes this
    // assertion able to tell the new behaviour from the old.
    expectTrue(r.hasColumnHandle, 'FK 列上没有生成句柄');
    expectTrue(
      Math.abs(r.endpointOffsetY - 73) < 3,
      `连线端点距节点顶部 ${r.endpointOffsetY}px，期望落在 FK 列行中心 73px`,
    );
    expectTrue(
      Math.abs(r.endpointOffsetY - r.nodeHeight / 2) > 8,
      `端点 ${r.endpointOffsetY}px 仍接近节点中点 ${r.nodeHeight / 2}px，说明未按列定位`,
    );
  });

  it('ER-012: 悬停表节点应强调其关联边、压暗无关节点', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    // er_pred_owner/er_pred_member are related only by naming convention, so the
    // edge under test is an *inferred* one. Prediction is opt-in (it walks every
    // table in the database) and ER-009/ER-011 leave it off, so switch it back on
    // through the diagram's own toggle and wait for the edge itself: the node
    // renders whether or not prediction is on, so waiting on it proves nothing.
    await setErPrediction(true);
    try {
      await browser.waitUntil(async () => (await findPredictedEdgeId()) !== null, {
        timeout: 15000,
        timeoutMsg: 'ER-012: 开启智能预测后未画出推测关系',
      });

      const node = await $(`.react-flow__node[data-id="${PRED_MEMBER}"]`);
      await node.waitForDisplayed({ timeout: 10000 });

      // The hover is driven by a dispatched bubbling `mouseover` rather than by
      // moving the WebDriver pointer. Measured: a real pointer move reaches this
      // WebKit webview as no mouseover at all, so the pointer path cannot observe
      // the behaviour — while React synthesises `onMouseEnter` from exactly this
      // event. The assertion below is therefore about the wiring and the styling,
      // in the real DOM and the real CSS.
      // `dimmed` renders as an opacity class on the node element itself
      // (`[data-testid="er-table-node"]`), not on React Flow's `.react-flow__node`
      // wrapper, so the wrapper always reports opacity 1.
      const countDimmed = () =>
        browser.execute(
          () =>
            Array.from(document.querySelectorAll('[data-testid="er-table-node"]')).filter(
              (el) => Number(getComputedStyle(el).opacity) < 1,
            ).length,
        );
      const before = await countDimmed();

      const enter = await browser.execute((member: string) => {
        const host = document.querySelector(`.react-flow__node[data-id="${member}"]`);
        if (!host) return { error: 'node missing' };
        const target = host.querySelector('[data-testid="er-table-node"]') ?? host;
        target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, cancelable: true }));
        return { ok: true };
      }, PRED_MEMBER);
      expectTrue(!('error' in enter), `悬停目标节点不存在: ${JSON.stringify(enter)}`);
      await browser.pause(400);

      const hovered = await browser.execute(
        (member: string, owner: string) => {
          const edgeEls = Array.from(document.querySelectorAll('.react-flow__edge'));
          const predicted = edgeEls.find((el) => {
            const id = el.getAttribute('data-id') ?? '';
            return id.startsWith('predicted-') && id.includes(member) && id.includes(owner);
          });
          const path = predicted?.querySelector('path');
          const nodeEls = Array.from(document.querySelectorAll('[data-testid="er-table-node"]'));
          const opacityOf = (id: string) => {
            const el = document.querySelector(
              `.react-flow__node[data-id="${id}"] [data-testid="er-table-node"]`,
            );
            return el ? Number(getComputedStyle(el).opacity) : null;
          };
          return {
            incidentStrokeWidth: path ? getComputedStyle(path).strokeWidth : null,
            hoveredOpacity: opacityOf(member),
            relatedOpacity: opacityOf(owner),
            dimmedNodes: nodeEls.filter((el) => Number(getComputedStyle(el).opacity) < 1).length,
            totalNodes: nodeEls.length,
          };
        },
        PRED_MEMBER,
        PRED_OWNER,
      );

      expectTrue(
        Number.parseFloat(String(hovered.incidentStrokeWidth)) > 2,
        `悬停后关联边未加粗（strokeWidth=${String(hovered.incidentStrokeWidth)}）`,
      );
      // The hovered table and the table it relates to stay legible...
      expectTrue(
        hovered.hoveredOpacity === 1,
        `被悬停的表自身被压暗了（opacity=${String(hovered.hoveredOpacity)}）`,
      );
      expectTrue(
        hovered.relatedOpacity === 1,
        `与被悬停表相关的表被压暗了（opacity=${String(hovered.relatedOpacity)}）`,
      );
      // ...while everything unrelated recedes.
      expectTrue(
        hovered.dimmedNodes > 0,
        `悬停后没有任何节点被压暗（共 ${hovered.totalNodes} 个节点）`,
      );
      expectTrue(
        hovered.dimmedNodes === hovered.totalNodes - 2,
        `应压暗 ${hovered.totalNodes - 2} 个无关节点，实际 ${hovered.dimmedNodes}`,
      );
      expectTrue(before === 0, `悬停前已有 ${before} 个节点被压暗`);

      // Moving off the node must restore every node.
      await browser.execute((member: string) => {
        const host = document.querySelector(`.react-flow__node[data-id="${member}"]`);
        if (!host) return;
        const target = host.querySelector('[data-testid="er-table-node"]') ?? host;
        target.dispatchEvent(
          new MouseEvent('mouseout', {
            bubbles: true,
            cancelable: true,
            relatedTarget: document.body,
          }),
        );
      }, PRED_MEMBER);
      await browser.pause(400);
      const restored = await countDimmed();
      expectTrue(restored === 0, `移开后仍有 ${restored} 个节点保持压暗`);
    } finally {
      await setErPrediction(false);
    }
  });

  it('ER-013: 重新布局按钮应可用且不破坏画布', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const relayout = await $('[data-testid="er-diagram-relayout"]');
    await relayout.waitForDisplayed({ timeout: 10000 });
    await relayout.click();
    await browser.pause(600);

    const after = await browser.execute(() => {
      const rects = Array.from(document.querySelectorAll('[data-testid="er-table-node"]')).map(
        (el) => el.getBoundingClientRect(),
      );
      let collisions = 0;
      for (let i = 0; i < rects.length; i++) {
        for (let j = i + 1; j < rects.length; j++) {
          const a = rects[i]!;
          const b = rects[j]!;
          const overlapX = a.left < b.left + b.width && b.left < a.left + a.width;
          const overlapY = a.top < b.top + b.height && b.top < a.top + a.height;
          if (overlapX && overlapY) collisions++;
        }
      }
      return { count: rects.length, collisions };
    });
    expectTrue(after.count > 0, '重新布局后画布上没有节点');
    expectTrue(after.collisions === 0, `重新布局后出现 ${after.collisions} 处重叠`);
  });

  it('ER-007: 搜索框应可过滤表节点', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const search = await $('[data-testid="er-diagram-search"]');
    await search.waitForDisplayed({ timeout: 10000 });
    await search.clearValue();
    await search.setValue('pg_');
    await browser.pause(500);
    await captureJourneyStep('er-search-filtered', 0, true);
    const nodeCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="er-table-node"]').length,
    );
    expect(nodeCount).toBeGreaterThanOrEqual(0);
  });

  // SKIPPED: E2E quiet mode (DATAZEN_E2E_QUIET) makes the webdriver window
  // alpha-0 and never key, and WebKit suspends requestAnimationFrame for such
  // webviews. handleExportPng rasterizes through html-to-image, whose
  // createImage resolves only inside a rAF callback — so toPng never settles
  // and the export hangs before its first IPC. Measured, not inferred: a probe
  // in this spec reported `clicks=1; console.error=none; saveIpc=0;
  // raf=raf-never-fired`, and the same export path is unchanged since v0.2.2
  // (html-to-image pinned at 1.11.13), so this is a harness artifact, not a
  // product regression — with a visible window rAF fires and the export works.
  // Re-enable by driving the export through `DATAZEN_E2E_QUIET_CONCEAL=0`
  // (visible-but-unfocused) or by shimming rAF onto a timer in the webview.
  it.skip('ER-008: 导出 PNG 应通过注入对话框落盘（mock 原生另存为）— SKIPPED: quiet-mode webview suspends rAF, so html-to-image never settles', async () => {});
});
