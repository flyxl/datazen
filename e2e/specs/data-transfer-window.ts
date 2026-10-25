import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

/**
 * Data Transfer window smoke (DTW-001~DTW-003).
 * Full cross-dialect execute paths are not covered here.
 */

describe('数据传输窗口 (DTW-001~DTW-003)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DTW-001: 应能通过 URL 打开数据传输窗口', async () => {
    await openDataTransferWindow();
    const root = await $('[data-testid="data-transfer-window"]');
    await expect(root).toBeDisplayed();
    await captureJourneyStep('transfer-window-open');
    const body = await $('body').getText();
    expect(body).toContain(t('transfer.source'));
    expect(body).toContain(t('transfer.target'));
  });

  it('DTW-002: 应显示端点步骤；模式选择在 setup 步', async () => {
    await openDataTransferWindow();
    await expect(await $('[data-testid="data-transfer-step-endpoints"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('transfer.source'));
    expect(body).toContain(t('transfer.target'));
  });

  it('DTW-003: 未选两端点时 Next 应禁用', async () => {
    await openDataTransferWindow();
    const next = await $('[data-testid="data-transfer-next"]');
    await next.waitForDisplayed({ timeout: 8000 });
    expect(await next.isEnabled()).toBe(false);
  });

  it('DTW-004: 应通过弹窗显示当前版本限制说明', async () => {
    await openDataTransferWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="data-transfer-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });
    const panel = await dialog.$('[data-testid="data-transfer-limitations"]');
    await panel.waitForDisplayed({ timeout: 8000 });
    expect(await panel.getText()).toContain(t('transfer.limitations.noFkIndexes'));
    // Limitations render inside the modal dialog, not as an embedded wizard panel.
    const panels = await $$('[data-testid="data-transfer-limitations"]');
    expect(panels).toHaveLength(1);
    await expect(await $('[data-testid="data-transfer-source"]')).toBeDisplayed();
  });

  it('DTW-005: 勾选「不再显示此提示」后再次打开不应弹出限制说明', async () => {
    await openDataTransferWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="data-transfer-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });

    const dismiss = await $('[data-testid="data-transfer-limitations-dismiss"]');
    await dismiss.click();
    const closeBtn = await $('[data-testid="data-transfer-limitations-close"]');
    await closeBtn.click();
    await browser.waitUntil(async () => !(await dialog.isDisplayed().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: '等待限制说明弹窗关闭超时',
    });

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await openDataTransferWindow({ dismissLimitations: false, clearLimitationsPref: false });

    const dialogAgain = await $('[data-testid="data-transfer-limitations-dialog"]');
    expect(await dialogAgain.isExisting().catch(() => false)).toBe(false);
  });
});

/**
 * Data Transfer 真实迁移闭环（DTW-CL-00x）。
 * PG→PG：`datazen_sync_src` → `datazen_sync_tgt`（由 setup-sync-dbs.sh 保证存在），
 * 走 UI 向导（endpoints → setup → objects → mapping → preview/execute → result），
 * 以落库查询断言目标表行数。需要已运行 `e2e/setup-sync-dbs.sh` 且 PG 可写。
 */
describe('数据传输真实迁移 (DTW-CL)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_xfer_src_${STAMP}`;
  const TGT_ID = `e2e_xfer_tgt_${STAMP}`;
  const SRC_NAME = `XferSrc-${STAMP}`;
  const TGT_NAME = `XferTgt-${STAMP}`;
  const TABLE = `xfer_e2e_${STAMP}`;

  const pgConfig = (id: string, name: string, database: string) => ({
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  });

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    // 两个后端连接 + 源表数据
    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
    });
    const srcDbSessionId = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const tgtDbSessionId = await invokeBackend<string>('connect', { connectionId: TGT_ID });

    try {
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: srcDbSessionId,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtDbSessionId,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: srcDbSessionId,
          sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: srcDbSessionId,
          sql: `INSERT INTO ${TABLE} (id, name, qty) VALUES (1,'a',10),(2,'b',20),(3,'c',30)`,
        });
        // 目标预建同结构空表，使 insert 模式可直接批量写
        await invokeBackend('execute_query', {
          dbSessionId: tgtDbSessionId,
          sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
        });
      });
    } finally {
      await disconnectBackend(srcDbSessionId);
      await disconnectBackend(tgtDbSessionId);
    }

    await closeExtraWindows(mainWindow);
  });

  after(async () => {
    try {
      const srcDbSessionId = await invokeBackend<string>('connect', { connectionId: SRC_ID });
      const tgtDbSessionId = await invokeBackend<string>('connect', { connectionId: TGT_ID });
      try {
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', {
            dbSessionId: srcDbSessionId,
            sql: `DROP TABLE IF EXISTS ${TABLE}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId: tgtDbSessionId,
            sql: `DROP TABLE IF EXISTS ${TABLE}`,
          });
        });
      } finally {
        await disconnectBackend(srcDbSessionId);
        await disconnectBackend(tgtDbSessionId);
      }
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: SRC_ID });
    } catch {}
    try {
      await invokeBackend('delete_connection', { id: TGT_ID });
    } catch {}
    await closeExtraWindows(mainWindow);
  });

  async function selectTransferEndpoints(): Promise<void> {
    await selectDzOptionInWrap('data-transfer-source', SRC_NAME);
    await selectDzOptionInWrap('data-transfer-target', TGT_NAME);
    await browser.pause(1500);
  }

  async function clickNext(label = 'transfer-wizard-next'): Promise<void> {
    const next = await $('[data-testid="data-transfer-next"]');
    await next.waitForClickable({ timeout: 8000 });
    await next.click();
    await browser.pause(1200);
    await captureJourneyStep(label);
  }

  /** 每个用例都用一扇**干净的**迁移窗口。
   *
   * 用例之间不关窗口时，上一轮的迁移窗口还活着：`$('[data-testid=...]')` 命中
   * 的是当前窗口里的第一个匹配，而当前窗口可能停在上一轮的对象步骤上。全量跑
   * 时 DTW-X-002 就因此读到空正文（`Received string: ""`），单独复跑却正常——
   * 差别只在前面累积了几扇窗口。 */
  async function openFreshTransferWindow(): Promise<void> {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await openDataTransferWindow();
  }

  /** 对象步骤只勾本用例自己那张表。
   *
   * 源库 `datazen_sync_src` 是多个 spec 共用的，整包跑到这里时里面已经堆了几十
   * 张别人留下的表（实测映射步第一屏是 `dz_sync_uncertain_*`）。默认全选会把
   * 它们一起带进映射步，本用例的表被挤到虚拟列表外，`qty` 列自然读不到——单跑
   * 时库里干净，所以只在整包跑里红。 */
  async function selectOnlyOwnTable(): Promise<void> {
    await browser.waitUntil(
      async () => (await $$('[data-testid="data-transfer-table-row"]')).length > 0,
      { timeout: 30000, timeoutMsg: 'ID:data-transfer-window|对象步骤没有表行' },
    );
    const rows = await $$('[data-testid="data-transfer-table-row"]');
    let matched = false;
    for (const row of rows) {
      const text = await row.getText();
      const checkbox = await row.$('input[type="checkbox"]');
      const shouldSelect = text.includes(TABLE);
      if (shouldSelect) matched = true;
      const selected = await checkbox.isSelected().catch(() => false);
      if (selected && !shouldSelect) await checkbox.click();
      if (!selected && shouldSelect) await checkbox.click();
    }
    if (!matched) throw new Error(`对象步骤里没有本用例的表 ${TABLE}`);
    await browser.pause(500);
  }

  it('DT-CL-001: 选择两端点并进入下一步', async () => {
    await openFreshTransferWindow();
    await selectTransferEndpoints();

    await expect(await $('[data-testid="data-transfer-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-target-database"]')).toBeDisplayed();
    // 支持组合提示应出现（同族 direct）
    await clickNext('transfer-step-endpoints');
  });

  it('DTW-X-002: 选择 data 模式并推进到对象/数据迁移', async () => {
    await openFreshTransferWindow();
    await selectTransferEndpoints();

    const sourceSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const targetSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
    try {
      const inspected = await invokeBackend<
        Array<{ sourceTable: string; sourceColumns: string[]; targetColumns: string[] }>
      >('inspect_data_transfer', {
        sourceDbSessionId: sourceSession,
        targetDbSessionId: targetSession,
        sourceDatabase: 'datazen_sync_src',
        targetDatabase: 'datazen_sync_tgt',
        mode: 'data',
        tables: null,
      });
      const fixture = inspected.find((table) => table.sourceTable === TABLE);
      if (!fixture) {
        throw new Error(
          `inspect_data_transfer omitted ${TABLE}; returned source tables: ${inspected
            .map((table) => table.sourceTable)
            .join(', ')}`,
        );
      }
      expect(fixture?.sourceColumns ?? []).toContain('qty');
      expect(fixture?.targetColumns).toContain('qty');
    } finally {
      await disconnectBackend(sourceSession);
      await disconnectBackend(targetSession);
    }

    await clickNext('transfer-step-endpoints'); // endpoints → setup

    // 默认 mode = data，直接 Next 经过 setup
    await clickNext('transfer-step-setup'); // setup → objects
    // Objects 步：只勾本用例的表，再把无关的残留表挡在映射之外
    await browser.pause(1000);
    await selectOnlyOwnTable();
    await clickNext('transfer-step-objects'); // 触发 inspect → mapping

    // 映射步要等 inspect 返回才出现，固定 pause 会随机抢跑：等步骤本身出现。
    await browser.waitUntil(
      async () => await $('[data-testid="data-transfer-mapping-step"]').isExisting(),
      { timeout: 30000, timeoutMsg: '跳转后未出现映射步骤' },
    );

    // 映射明细比步骤本身晚一步：步骤容器出现时表清单和列还在渲染，固定断言
    // 会读到空文本（本轮实测 `Received string: ""`）。等到「列输入框」或
    // 「正文出现列名」任一成立再判定，两条证据都不猜。
    // 全量跑时 `datazen_sync_src` 里堆积了前面所有 spec 留下的表，对象步骤的
    // inspect 明显更慢，20s 仍会抢跑（整包实测超时，单跑却通过）。60s 之外
    // 把失败现场一起抛出来：步骤正文、对象面板、错误块。
    await browser
      .waitUntil(
        async () => {
          if (await $('[data-testid="data-transfer-target-col-qty"]').isExisting()) return true;
          const body = await $('body').getText();
          return body.toLowerCase().includes('qty');
        },
        {
          timeout: 60000,
          timeoutMsg: 'ID:data-transfer-window|映射明细未渲染',
        },
      )
      .catch(async (error: unknown) => {
        const detail = await browser.execute(() => {
          const pick = (selector: string) =>
            Array.from(document.querySelectorAll(selector))
              .map((node) => `${selector}: ${(node.textContent ?? '').slice(0, 160)}`)
              .join(' | ');
          return [
            pick('[data-testid="data-transfer-mapping-step"]'),
            pick('[data-testid="data-transfer-objects-step"]'),
            pick('[data-testid="data-transfer-preview-error"]'),
          ]
            .filter(Boolean)
            .join(' || ');
        });
        throw new Error(`${(error as Error).message} :: ${detail || '(无相关节点文本)'}`);
      });

    // 映射步的列名是输入框的 value，不在 body 文本里：先按稳定 testid 找映射列，
    // 只有拿不到时才退回整页文本断言。
    const mappedColumn = await $('[data-testid="data-transfer-target-col-qty"]');
    if (await mappedColumn.isExisting()) {
      expect(await mappedColumn.isDisplayed()).toBe(true);
      expect(await mappedColumn.getValue()).toBe('qty');
    } else {
      const body = await $('body').getText();
      expect(body.toLowerCase()).toContain('qty');
    }
  });

  it('DTW-X-003: 预览存在后可执行并断言落库行数=3', async () => {
    await openFreshTransferWindow();
    await selectTransferEndpoints();

    // 兜底：若尚未到 preview/execute，持续推进。对象步骤先只勾本用例的表——源库
    // 与别的 spec 共用，默认全选会把残留表一起搬走（与 DTW-X-002 同一处坑）。
    for (let i = 0; i < 8; i++) {
      const stepExecute = await $('[data-testid="data-transfer-execute"]');
      if (await stepExecute.isExisting().catch(() => false)) break;
      const next = await $('[data-testid="data-transfer-next"]');
      const disabled = (await next.getAttribute('disabled')) === 'true';
      if (disabled) await browser.pause(1000);
      const tableRows = await $$('[data-testid="data-transfer-table-row"]');
      if (tableRows.length > 0) await selectOnlyOwnTable();
      await clickNext();
    }

    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 15000 });
    await execute.click();
    await captureJourneyStep('transfer-executed');
    await browser.pause(1500);

    // 断言结果面板出现
    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 15000 });

    // 落库断言：目标库该表应有 3 行
    const tgtConn = (await invokeBackend<string>('connect', { connectionId: TGT_ID })) ?? '';
    expect(tgtConn).toBeTruthy();
    try {
      const rows = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: tgtConn,
        sql: `SELECT count(*)::int AS c FROM ${TABLE}`,
      });
      expect(queryScalar(rows, 'c')).toBe(3);
    } finally {
      await disconnectBackend(tgtConn);
    }
  });
});
