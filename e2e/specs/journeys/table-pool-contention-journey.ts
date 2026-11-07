import { expect, browser, $, $$ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickTableInSidebar,
  clickNavigatorRefresh,
  closeExtraWindows,
  connectBackend,
  connectSeededPgInWorkspace,
  disconnectBackend,
  executeQuery,
  expandSchemaTableCategory,
  executeSQL,
  invokeBackend,
  openQueryTab,
  parseQueryRows,
  withSafeModeOff,
} from '../../helpers.js';

/**
 * CM-33 at the real UI layer: table churn must not become "one connection per tab",
 * and the editors must be unaffected by it.
 *
 * Why the count comes from the runtime and not from PostgreSQL: `pg_stat_activity` reports
 * server connections, but not the runtime pool's admission and lease state. The runtime
 * exposes its own ledger through `runtime_observability_snapshot`
 * (`packages/runtime/src/resource/observability.rs`), and that is what this journey asserts:
 * the physical total stays inside the budget and does not grow with the number of table tabs,
 * while the editor keeps working with the same observed context.
 *
 * The numeric half of the contract (a shared short-operation pool with a hard cap) is pinned
 * at the H layer by `packages/runtime/tests/pool_capacity_budget.rs`; what this spec adds is
 * the part fixtures cannot show — a real UI doing the churn.
 */

type PoolEntry = {
  poolKeyDigest: string;
  model: string;
  maxIdle: number;
  inUse: number;
  idle: number;
  closing: number;
  quarantined: number;
  issued: number;
};

type Report = {
  physicalLimit: number;
  physicalTotal: number;
  inUseTotal: number;
  idleTotal: number;
  closingTotal: number;
  quarantinedTotal: number;
  budgetInUse: number;
  budgetGrantedTotal: number;
  pools: PoolEntry[];
};

const ADMIN_ID = 'e2e_pg_contention_admin';
/**
 * Probe tables are created **unqualified** (so the server's search_path decides,
 * normally `public`) and the schema they actually landed in is read back from
 * `pg_tables` afterwards. Hard-coding a schema name is how this journey first
 * failed: `E2E_WORKER_SCHEMA` is set by the launcher but that schema need not
 * exist in every worker database.
 */
const PROBE_TABLES = ['journey_pool_a', 'journey_pool_b', 'journey_pool_c', 'journey_pool_d'];
/**
 * 收尾（settled）允许的余量：刷新结束后只应剩编辑器 retained session 那一档。
 */
const SETTLED_SLACK = 2;

/**
 * 刷新**进行中**允许的瞬时余量。
 *
 * 一次表格刷新的过渡里，台账会同时计入：正在交出的旧租约、刚签发的新租约、
 * 导航树刷新的元数据租约——它们都很短，但确实同时存在；如果基线里没有空闲
 * 资源可以复用，这三个都会以新物理资源的形式出现。实测（连跑 6 次，基线 4~5）
 * 峰值落在 `基线 + 1` 到 `基线 + 2`；整包跑时撞到过 `基线 + 3`（基线 4、峰值 7，
 * 收尾回到 0）。所以瞬时余量取 3；「不随 tab 数增长」由收尾断言兜住（4 个 tab 刷完还挂着
 * `基线 + tab 数` 就是按 tab 留连接，收尾那条断言会直接抓住）。
 */
const TRANSIENT_SLACK = 3;

function readFirstResultCell(): Promise<string | null> {
  return browser.execute(() => {
    const cell = document.querySelector('[data-testid="data-table-cell"]');
    return cell?.textContent?.trim() ?? null;
  });
}

async function snapshot(): Promise<Report> {
  return invokeBackend<Report>('runtime_observability_snapshot');
}

describe('表格并发不按 tab 增连接 (TABLE-POOL-CONTENTION-JOURNEY)', function () {
  this.timeout(300000);
  let mainWindow: string;
  let admin: string | undefined;
  let queryTabLabel = '';
  /** Schema the probe tables actually landed in — read back from PostgreSQL. */
  let probeSchema = '';
  const facts = {
    limit: 0,
    baseline: 0,
    maxDuringChurn: 0,
    settled: 0,
    tableTabs: 0,
    contextBefore: '',
    contextAfter: '',
    budgetInUse: 0,
    /** 峰值那一刻每个池的样子：只报总数看不出「多出来的那个是谁」。 */
    poolsAtMax: '',
  };
  let stage = 'setup';

  async function adminSql(sql: string): Promise<void> {
    if (!admin) throw new Error('observation session unavailable');
    await executeQuery(admin, sql);
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // The editor first: the worker database name is only knowable from the live
    // seeded session, and the observer must land on that SAME database or the
    // probe tables would be created where the navigator cannot see them.
    await connectSeededPgInWorkspace();
    await openQueryTab();
    const active = await $('[data-testid="panel-tab-select"][aria-selected="true"]');
    queryTabLabel = (await active.getAttribute('aria-label')) ?? '';
    await executeSQL('SELECT current_database() AS ctx_before');
    await browser.waitUntil(async () => (await readFirstResultCell()) !== null, {
      timeout: 20000,
      timeoutMsg: 'the editor never rendered its first result',
    });
    const workerDatabase = (await readFirstResultCell()) ?? '';
    if (!workerDatabase) throw new Error('editor database unknown');
    facts.contextBefore = workerDatabase;

    await invokeBackend('save_connection', {
      config: {
        id: ADMIN_ID,
        name: 'E2E-PG-Contention-Observer',
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: workerDatabase,
        group: 'E2E 测试',
        colorTag: 'pink',
        sslMode: 'disable',
        options: {},
      },
    });
    admin = await connectBackend(ADMIN_ID);
    await withSafeModeOff(async () => {
      for (const table of PROBE_TABLES) {
        await adminSql(`DROP TABLE IF EXISTS ${table}`);
        await adminSql(`CREATE TABLE ${table} (id integer PRIMARY KEY, payload text)`);
        await adminSql(`INSERT INTO ${table} VALUES (1, 'probe')`);
      }
    });
    const schemaProbe = await executeQuery(
      admin,
      `SELECT schemaname FROM pg_tables WHERE tablename = '${PROBE_TABLES[0]}'`,
    );
    probeSchema = String(parseQueryRows(schemaProbe)[0]?.[0] ?? '');
    if (!probeSchema) throw new Error('probe tables were not created');
  });

  after(async () => {
    try {
      await withSafeModeOff(async () => {
        for (const table of PROBE_TABLES) {
          await adminSql(`DROP TABLE IF EXISTS ${table}`).catch(() => undefined);
        }
      });
    } catch {
      /* best-effort fixture cleanup */
    }
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* window already gone */
    }
    if (admin) await disconnectBackend(admin);
    await invokeBackend('delete_connection', { id: ADMIN_ID }).catch(() => undefined);
  });

  it('表格刷新期间物理资源不越预算、不随 tab 数增长，编辑器不受影响', async () => {
    try {
      stage = 'baseline-snapshot';
      expect(facts.contextBefore).not.toBe('');
      const baseline = await snapshot();
      facts.limit = baseline.physicalLimit;
      facts.baseline = baseline.physicalTotal;
      expect(facts.limit).toBeGreaterThan(0);
      expect(facts.baseline).toBeGreaterThan(0);
      expect(facts.baseline).toBeLessThanOrEqual(facts.limit);
      facts.maxDuringChurn = facts.baseline;

      stage = 'table-churn';
      // The tables were created just before this connect; refresh so the tree is
      // not showing a listing that predates them, then expand their schema.
      await clickNavigatorRefresh();
      await expandSchemaTableCategory(probeSchema);
      for (const table of PROBE_TABLES) {
        await clickTableInSidebar(table);
        const refresh = await $('[data-testid="table-data-refresh"]');
        await refresh.waitForDisplayed({ timeout: 20000 });
        await refresh.click();
        facts.tableTabs += 1;
        const sample = await snapshot();
        if (sample.physicalTotal > facts.maxDuringChurn) {
          facts.maxDuringChurn = sample.physicalTotal;
          facts.poolsAtMax = sample.pools
            .map(
              (pool) =>
                `${pool.poolKeyDigest}/${pool.model}[inUse=${pool.inUse},idle=${pool.idle},` +
                `issued=${pool.issued},closing=${pool.closing}]`,
            )
            .join(' ');
        }
        expect(sample.physicalTotal).toBeLessThanOrEqual(sample.physicalLimit);
        // `expect` here takes a single argument: the message must live in the
        // staged failure report, not as a second parameter.
        expect(sample.physicalTotal).toBeLessThanOrEqual(facts.baseline + TRANSIENT_SLACK);
      }
      await captureJourneyStep('table-pool-churn');

      stage = 'editor-unaffected';
      const tabs = await $$('[data-testid="panel-tab-select"]');
      for (const tab of tabs) {
        if ((await tab.getAttribute('aria-label')) !== queryTabLabel) continue;
        await tab.click();
        break;
      }
      await executeSQL('SELECT current_database() AS ctx_after');
      await browser.waitUntil(async () => (await readFirstResultCell()) === facts.contextBefore, {
        timeout: 20000,
        timeoutMsg: 'the editor context moved during table churn',
      });
      facts.contextAfter = (await readFirstResultCell()) ?? '';

      stage = 'settled-snapshot';
      const settled = await snapshot();
      facts.settled = settled.physicalTotal;
      facts.budgetInUse = settled.budgetInUse;
      // 「不为每 tab 建连接」在这里看最准：刚刚逐个打开并刷新了 tableTabs 个表 tab，
      // 如果每个 tab 各自留一条连接，收尾必然还挂着 `基线 + tab 数` 那么多资源；
      // 只要回落到基线 + 2 以内，就说明表格共用短操作池、没有按 tab 留存。
      expect(settled.physicalTotal).toBeLessThanOrEqual(facts.baseline + SETTLED_SLACK);
      expect(settled.physicalTotal).toBeLessThanOrEqual(settled.physicalLimit);
      expect(settled.budgetInUse).toBeLessThanOrEqual(settled.physicalLimit);
      for (const pool of settled.pools) {
        expect(pool.poolKeyDigest).not.toBe('');
        expect(pool.issued).toBe(pool.inUse + pool.idle + pool.closing + pool.quarantined);
      }
      await captureJourneyStep('table-pool-settled');
    } catch (error) {
      const category =
        error instanceof Error &&
        ['WebDriverError', 'TimeoutError', 'AssertionError'].includes(error.name)
          ? error.name
          : 'Error';
      const detail = (error instanceof Error ? error.message : String(error)).slice(0, 160);
      throw new Error(
        `table pool contention journey failed at ${stage} (${category}); detail=${detail}; ` +
          `limit=${facts.limit}; baseline=${facts.baseline}; maxDuringChurn=${facts.maxDuringChurn}; ` +
          `settled=${facts.settled}; tableTabs=${facts.tableTabs}; ` +
          `ctxBefore=${facts.contextBefore}; ctxAfter=${facts.contextAfter}; ` +
          `budgetInUse=${facts.budgetInUse}; maxPools=${facts.poolsAtMax}`,
      );
    }
  });
});
