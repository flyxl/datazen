/**
 * WebdriverIO config for DataZen Host E2E (Tauri embedded WebDriver plugin).
 *
 * ## Parallel execution (Tauri constraint)
 *
 * Native WDIO `maxInstances > 1` is intentionally NOT used:
 * - Each worker needs its own DataZen process, WebDriver port, and app-data dir.
 * - WDIO capabilities duplicate every spec across all capabilities (cross-browser
 *   model), which does not distribute load across Tauri instances.
 *
 * Parallelism is handled by `e2e/run.mjs --instances N` (N > 1):
 * 1. Starts N app binaries on consecutive ports (E2E_WD_PORT … E2E_WD_PORT+N-1).
 * 2. Isolates DATAZEN_DATA_DIR per worker (`e2e/.app-data-0` …).
 * 3. Round-robin splits specs into N independent WDIO processes (each maxInstances: 1).
 *
 * Scripts: `pnpm e2e:parallel`, `pnpm e2e:parallel:smoke`, `pnpm e2e:parallel:core`.
 * DB-heavy suites may conflict on shared test databases — prefer core/smoke for parallel runs.
 */
import {
  beginJourneySuite,
  beginJourneyTest,
  ensureScreenshotRoot,
  isScreenshotTraceEnabled,
  saveJourneyScreenshot,
} from './lib/screenshotTrace.js';
import {
  cleanupAppDataViaIpc,
  createWorkerDatabase,
  dropWorkerDatabase,
  seedDefaultPgConnection,
} from './lib/testDataLifecycle.js';
import { ensureMainWindowForIpc, invokeBackend } from './helpers.js';
import { browser } from '@wdio/globals';

const WD_PORT = parseInt(process.env.E2E_WD_PORT || '4445', 10);

/** Per-worker isolated database name — set in runSessionBootstrap, dropped in after. */
let _workerDb: string | undefined;

const capabilities: WebdriverIO.Capabilities[] = [{}];

async function runSessionBootstrap() {
  await browser.url('tauri://localhost');
  await browser.pause(2000);
  try {
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 10000 });
  } catch {
    await browser.url('tauri://localhost');
    await browser.pause(2000);
  }

  await browser.executeAsync((done: (r: unknown) => void) => {
    const inv = (window as any).__TAURI_INTERNALS__.invoke.bind(
      (window as any).__TAURI_INTERNALS__,
    );
    inv('get_settings')
      .then((settings: Record<string, unknown>) =>
        inv('save_settings', {
          settings: {
            ...settings,
            language: 'zh-CN',
            theme:
              settings.theme && typeof settings.theme === 'object'
                ? { ...(settings.theme as object), mode: 'dark' }
                : { mode: 'dark', packId: null },
            limitSelectResults: true,
            queryResultLimit: 1000,
            editorFontSize: 14,
            editorFontFamily: 'monospace',
            confirmOnDelete: true,
            autoCommit: true,
            safeMode: true,
            defaultPageSize: 50,
            sqlExecutionStrategy: 'entire_script',
            onboarding: { completed: true, version: 1 },
          },
        }),
      )
      .then(() => done(null))
      .catch((e: unknown) => done(String(e)));
  });

  _workerDb = createWorkerDatabase();
  await seedDefaultPgConnection(browser, _workerDb);
  await browser.execute(() => location.reload());
  await browser.pause(2000);
  try {
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 10000 });
  } catch {
    await browser.pause(2000);
  }

  await browser.execute(() => {
    document.querySelectorAll('[data-group-header]').forEach((el) => {
      const parent = el.closest('[data-group-name]');
      if (parent && !parent.querySelector('[data-conn-item]')) {
        (el as HTMLElement).click();
      }
    });
  });
  await browser.pause(500);
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  exclude: ['./specs/zz-screenshots.ts', './specs/demo-recording.ts', './specs/zz-diag.ts'],
  suites: {
    smoke: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/settings.ts',
      './specs/homepage-features.ts',
      './specs/connection-empty-state.ts',
      './specs/connection-window.ts',
      './specs/sql-query.ts',
      './specs/table-data.ts',
      './specs/table-filter.ts',
      './specs/table-edit.ts',
      './specs/export-import.ts',
      './specs/connection-search-group.ts',
      './specs/edit-delete-connection.ts',
      './specs/i18n-menu.ts',
      './specs/client-parity.ts',
      './specs/conn-ctx-menu-submenus.ts',
      './specs/object-browser.ts',
      './specs/wapps.spec.ts',
      './specs/workflow.ts',
      './specs/er-diagram.ts',
      './specs/multi-database.ts',
      './specs/backup-window.ts',
      './specs/ai-features.ts',
      './specs/app-data-backup.ts',
      './specs/path-ipc-hardening.ts',
      './specs/driver-commands.ts',
      './specs/drag-drop-groups.ts',
      './specs/unified-tab-bar.ts',
    ],
    screenshots: ['./specs/zz-screenshots.ts', './specs/demo-recording.ts'],
    core: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/edit-delete-connection.ts',
      './specs/connection-search-group.ts',
      './specs/settings.ts',
      './specs/i18n-menu.ts',
      './specs/homepage-features.ts',
      './specs/connection-empty-state.ts',
      './specs/connection-zero-state.ts',
      './specs/drag-drop-groups.ts',
      './specs/backup-database.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/data-sync-window.ts',
      './specs/window-operations.ts',
      './specs/unified-tab-bar.ts',
      './specs/wapps.spec.ts',
    ],
    db: [
      './specs/connection-window.ts',
      './specs/sql-query.ts',
      './specs/table-data.ts',
      './specs/table-filter.ts',
      './specs/table-indexes.ts',
      './specs/table-edit.ts',
      './specs/table-structure.ts',
      './specs/export-import.ts',
      './specs/object-browser.ts',
      './specs/data-types.ts',
      './specs/mysql.ts',
      './specs/multi-database.ts',
      './specs/data-sync-real.ts',
      './specs/data-sync-edge-cases.ts',
      './specs/client-parity.ts',
      './specs/host-contract-matrix.ts',
      './specs/schema-tree-completeness.ts',
      './specs/table-batch-ops.ts',
      './specs/workflow.ts',
      './specs/er-diagram.ts',
      './specs/data-transfer-window.ts',
      './specs/data-transfer-type-mapping.ts',
      './specs/journeys/data-transfer-type-mapping-journey.ts',
      './specs/data-transfer-diverse-types.ts',
      './specs/data-transfer-mode-paths.ts',
    ],
    contract: ['./specs/host-contract-matrix.ts'],
    redis: ['../packages/drivers/redis/e2e/*.ts'],
    'pro-sql-editor': ['../packages/pro-extensions/sql-editor-pro/e2e/specs/*.ts'],
    ai: [
      './specs/ai-features.ts',
      './specs/ai-context.ts',
      './specs/ai-context-tables.ts',
      './specs/ai-code-block.ts',
      './specs/ai-no-key-fallback.ts',
    ],
    'i18n-backup': [
      './specs/app-data-backup.ts',
      './specs/i18n-10-locales.ts',
      './specs/system-locale.ts',
      './specs/i18n-menu.ts',
    ],
    'path-ipc': [
      './specs/path-ipc-hardening.ts',
      './specs/workflow-window.ts',
      './specs/driver-commands.ts',
      './specs/app-data-backup.ts',
    ],
    dashboard: ['./specs/data-dashboard*.ts'],
    'data-transfer': [
      './specs/data-transfer-window.ts',
      './specs/data-transfer-type-mapping.ts',
      './specs/journeys/data-transfer-type-mapping-journey.ts',
      './specs/data-transfer-diverse-types.ts',
      './specs/data-transfer-mode-paths.ts',
      './specs/journeys/data-transfer-journey.ts',
      './specs/journeys/data-transfer-pg-mysql-journey.ts',
      './specs/journeys/data-transfer-mysql-pg-journey.ts',
    ],
    'schema-diff': [
      './specs/schema-diff-window.ts',
      './specs/schema-diff-diverse-types.ts',
      './specs/schema-diff-cross-dialect.ts',
      './specs/schema-diff-options-matrix.ts',
      './specs/journeys/schema-diff-journey.ts',
      './specs/journeys/schema-diff-pg-mysql-journey.ts',
      './specs/journeys/schema-diff-mysql-pg-journey.ts',
    ],
    journeys: [
      './specs/journeys/schema-diff-journey.ts',
      './specs/journeys/schema-diff-pg-mysql-journey.ts',
      './specs/journeys/schema-diff-mysql-pg-journey.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/journeys/data-transfer-journey.ts',
      './specs/journeys/data-transfer-pg-mysql-journey.ts',
      './specs/journeys/data-transfer-mysql-pg-journey.ts',
      './specs/journeys/data-transfer-type-mapping-journey.ts',
      './specs/connection-navigator-expansion.ts',
      './specs/journeys/zero-state-query-journey.ts',
      './specs/journeys/connection-create-journey.ts',
      './specs/journeys/tunnel-connection-journey.ts',
      './specs/journeys/connection-browse-journey.ts',
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-result-chart-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/query-row-limit-journey.ts',
      './specs/journeys/first-run-edge-journey.ts',
      './specs/journeys/visual-query-builder-journey.ts',
      './specs/journeys/visual-query-builder-edge-journey.ts',
      './specs/journeys/visual-query-builder-complex-journey.ts',
      './specs/journeys/visual-query-builder-clauses-journey.ts',
      './specs/journeys/onboarding-journey.ts',
    ],
    'query-builder': [
      './specs/journeys/visual-query-builder-journey.ts',
      './specs/journeys/visual-query-builder-edge-journey.ts',
      './specs/journeys/visual-query-builder-complex-journey.ts',
      './specs/journeys/visual-query-builder-clauses-journey.ts',
    ],
    'qb-regression': [
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/connection-navigator-expansion.ts',
      './specs/table-data.ts',
    ],
    onboarding: ['./specs/journeys/onboarding-journey.ts'],
    'journey-edge': [
      './specs/journeys/first-run-edge-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-edge-journey.ts',
    ],
    'connection-edge': ['./specs/connection-validation.ts', './specs/connection-edge-cases.ts'],
    'data-sync': [
      './specs/data-sync-window.ts',
      './specs/data-sync-edge-cases.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/data-sync-real.ts',
    ],
  },
  maxInstances: 1,
  capabilities,
  hostname: '127.0.0.1',
  port: WD_PORT,
  path: '/',
  logLevel: 'warn',
  waitforTimeout: 10000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 3,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 120000,
  },
  before: async function () {
    await runSessionBootstrap();
  },
  beforeSuite: async function (suite) {
    beginJourneySuite(suite.file);
    try {
      await browser.url('tauri://localhost');
      await browser.pause(400);
      const handles = await browser.getWindowHandles();
      const main = handles[0];
      for (const h of handles) {
        if (h === main) continue;
        try {
          await browser.switchToWindow(h);
          await browser.closeWindow();
        } catch {
          /* ignore */
        }
      }
      if (main) await browser.switchToWindow(main);
      await ensureMainWindowForIpc();
      await invokeBackend('get_settings');
      await browser.pause(600);
    } catch {
      /* ignore */
    }
  },
  beforeTest: async function (test) {
    beginJourneyTest(test.file, test.title);
  },
  afterTest: async function (_test, _context, { passed }) {
    if (!isScreenshotTraceEnabled()) return;
    if (passed) return;
    try {
      await saveJourneyScreenshot(browser, 'fail', 300, true);
    } catch (err) {
      console.warn('[e2e-screenshot]', err);
    }
  },
  after: async function () {
    try {
      await cleanupAppDataViaIpc(browser);
    } catch (err) {
      console.warn('[e2e-teardown]', err);
    }
    if (_workerDb) {
      dropWorkerDatabase(_workerDb);
      _workerDb = undefined;
    }
  },
};
