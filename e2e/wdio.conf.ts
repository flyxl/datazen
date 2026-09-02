import {
  beginJourneySuite,
  beginJourneyTest,
  ensureScreenshotRoot,
  isScreenshotTraceEnabled,
  saveJourneyScreenshot,
} from './lib/screenshotTrace.js';
import { cleanupAppDataViaIpc, seedDefaultPgConnection } from './lib/testDataLifecycle.js';
import { ensureMainWindowForIpc, invokeBackend } from './helpers.js';
import { browser } from '@wdio/globals';

const instanceCount = parseInt(process.env.E2E_INSTANCES || '1', 10);
const multiInstance = instanceCount > 1;
const ports = multiInstance
  ? (process.env.E2E_WD_PORTS || '4445').split(',').map((p) => parseInt(p.trim(), 10))
  : [parseInt(process.env.E2E_WD_PORT || '4445', 10)];
const dataDirs = multiInstance
  ? (process.env.E2E_DATA_DIRS || '').split(',').map((d) => d.trim())
  : [process.env.DATAZEN_DATA_DIR || ''];

/**
 * One capability entry per app instance so WDIO assigns specs across workers.
 * Connection options (hostname/port) live at the capability top level — WDIO v9
 * merges them into each worker config (see multiremote / per-capability port docs).
 */
const capabilities: WebdriverIO.Capabilities[] = multiInstance
  ? ports.map((port) => ({
      hostname: '127.0.0.1',
      port,
      path: '/',
      'wdio:maxInstances': 1,
    }))
  : [{}];

async function runSessionBootstrap() {
  await browser.url('tauri://localhost');
  await browser.pause(2000);
  // Ensure we're on the main page — the app may start on welcome/settings
  try {
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 10000 });
  } catch {
    // Retry navigation if the element didn't appear
    await browser.url('tauri://localhost');
    await browser.pause(2000);
  }

  // Force language to zh-CN so all Chinese selectors work
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
          },
        }),
      )
      .then(() => done(null))
      .catch((e: unknown) => done(String(e)));
  });

  await seedDefaultPgConnection(browser);

  // Reload page so the new language and seeded connections take effect
  await browser.execute(() => location.reload());
  await browser.pause(2000);
  try {
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 10000 });
  } catch {
    // App may still be loading
    await browser.pause(2000);
  }

  // Expand all connection groups so items are visible
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
  /**
   * Named groups run via `pnpm e2e:<group>` (package.json) → `--suite <group>`.
   * Single source of truth for group membership; paths are relative to this
   * config file (same resolution as `specs`). Keep in sync with docs:
   * docs/development/e2e-testing.md §2.
   */
  suites: {
    // Fast regression subset (~30 specs, target <10 min) — `pnpm e2e:smoke`
    smoke: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/settings.ts',
      './specs/homepage-features.ts',
      './specs/connection-window.ts',
      './specs/sql-query.ts',
      './specs/table-data.ts',
      './specs/table-filter.ts',
      './specs/table-edit.ts',
      './specs/export-import.ts',
      './specs/connection-search-group.ts',
      './specs/edit-delete-connection.ts',
      './specs/i18n-menu.ts',
      './specs/connection-edge-cases.ts',
      './specs/client-parity.ts',
      './specs/conn-ctx-menu-submenus.ts',
      './specs/object-browser.ts',
      './specs/plugins.spec.ts',
      './specs/workflow.ts',
      './specs/er-diagram.ts',
      './specs/multi-database.ts',
      './specs/data-sync-window.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/ai-features.ts',
      './specs/app-data-backup.ts',
      './specs/path-ipc-hardening.ts',
      './specs/driver-commands.ts',
      './specs/drag-drop-groups.ts',
      './specs/unified-tab-bar.ts',
    ],
    // Manual screenshot / demo capture — `pnpm e2e -- --suite screenshots`
    screenshots: ['./specs/zz-screenshots.ts', './specs/demo-recording.ts'],
    // Core UI, no real DB required (was `pnpm e2e:core`)
    core: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/edit-delete-connection.ts',
      './specs/connection-search-group.ts',
      './specs/settings.ts',
      './specs/i18n-menu.ts',
      './specs/homepage-features.ts',
      './specs/drag-drop-groups.ts',
      './specs/backup-database.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/data-sync-window.ts',
      './specs/connection-edge-cases.ts',
      './specs/window-operations.ts',
      './specs/unified-tab-bar.ts',
      './specs/plugins.spec.ts',
    ],
    // Real-DB Host specs incl. the host contract matrix (was `pnpm e2e:db`)
    db: [
      './specs/connection-window.ts',
      './specs/connection-navigator-expansion.ts',
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
      './specs/sql-multi-tab.ts',
      './specs/schema-tree-completeness.ts',
      './specs/table-batch-ops.ts',
      './specs/workflow.ts',
      './specs/er-diagram.ts',
      './specs/data-transfer-window.ts',
      './specs/data-transfer-type-mapping.ts',
      './specs/data-transfer-type-mapping-mysql-pg.ts',
      './specs/data-transfer-diverse-types.ts',
      './specs/data-transfer-mode-paths.ts',
      './specs/journeys/schema-diff-journey.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/journeys/data-transfer-journey.ts',
      './specs/journeys/data-transfer-pg-mysql-journey.ts',
      './specs/journeys/data-transfer-mysql-pg-journey.ts',
    ],
    // Host contract matrix × PG/MySQL/SQLite (`pnpm e2e:contract:matrix`,
    // `pnpm e2e:contract:pg` adds --mochaOpts.grep 'Host contract @ postgres')
    contract: ['./specs/host-contract-matrix.ts'],
    // Redis driver's own E2E, not part of default full run (`pnpm e2e:redis`)
    redis: ['../packages/drivers/redis/e2e/*.ts'],
    // AI features (`pnpm e2e:ai`)
    ai: [
      './specs/ai-features.ts',
      './specs/ai-context.ts',
      './specs/ai-context-tables.ts',
      './specs/ai-code-block.ts',
      './specs/ai-no-key-fallback.ts',
    ],
    // App-data backup + i18n locales (`pnpm e2e:i18n-backup`)
    'i18n-backup': [
      './specs/app-data-backup.ts',
      './specs/i18n-10-locales.ts',
      './specs/system-locale.ts',
      './specs/i18n-menu.ts',
    ],
    // Path IPC hardening + workflow / driver commands (`pnpm e2e:path-ipc`)
    'path-ipc': [
      './specs/path-ipc-hardening.ts',
      './specs/workflow-window.ts',
      './specs/driver-commands.ts',
      './specs/app-data-backup.ts',
    ],
    // Dashboard (`pnpm e2e:dashboard`)
    dashboard: ['./specs/data-dashboard.ts'],
    // Data Transfer only (`pnpm e2e:data-transfer`)
    'data-transfer': [
      './specs/data-transfer-window.ts',
      './specs/data-transfer-type-mapping.ts',
      './specs/data-transfer-type-mapping-mysql-pg.ts',
      './specs/data-transfer-diverse-types.ts',
      './specs/data-transfer-mode-paths.ts',
      './specs/journeys/data-transfer-journey.ts',
      './specs/journeys/data-transfer-pg-mysql-journey.ts',
      './specs/journeys/data-transfer-mysql-pg-journey.ts',
    ],
    // Schema Diff only (`pnpm e2e:schema-diff`)
    'schema-diff': [
      './specs/schema-diff-window.ts',
      './specs/schema-diff-diverse-types.ts',
      './specs/schema-diff-cross-dialect.ts',
      './specs/schema-diff-options-matrix.ts',
      './specs/journeys/schema-diff-journey.ts',
      './specs/journeys/schema-diff-pg-mysql-journey.ts',
      './specs/journeys/schema-diff-mysql-pg-journey.ts',
    ],
    // Full user journeys for hidden v0.1.0 windows (`pnpm e2e:journeys`)
    journeys: [
      './specs/journeys/schema-diff-journey.ts',
      './specs/journeys/schema-diff-pg-mysql-journey.ts',
      './specs/journeys/schema-diff-mysql-pg-journey.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/journeys/data-transfer-journey.ts',
      './specs/journeys/data-transfer-pg-mysql-journey.ts',
      './specs/journeys/data-transfer-mysql-pg-journey.ts',
      './specs/data-transfer-type-mapping.ts',
      './specs/data-transfer-type-mapping-mysql-pg.ts',
      './specs/data-transfer-diverse-types.ts',
      './specs/data-transfer-mode-paths.ts',
    ],
    // Data Sync: UI smoke + edge cases + IPC + full journey (`pnpm e2e:data-sync`)
    'data-sync': [
      './specs/data-sync-window.ts',
      './specs/data-sync-edge-cases.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/data-sync-real.ts',
    ],
  },
  maxInstances: multiInstance ? instanceCount : 1,
  specFileRetries: 1,
  capabilities,
  ...(multiInstance
    ? {}
    : {
        hostname: '127.0.0.1',
        port: ports[0],
        path: '/',
      }),
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
  onWorkerStart(cid, _caps, _specs, args) {
    if (!multiInstance) return;
    const idx = parseInt(cid.split('-')[0], 10);
    const port = ports[idx] ?? ports[0];
    const dataDir = dataDirs[idx] ?? dataDirs[0];
    args.hostname = '127.0.0.1';
    args.port = port;
    args.path = '/';
    if (dataDir) {
      process.env.DATAZEN_DATA_DIR = dataDir;
    }
  },
  beforeSession(_config, caps) {
    if (!multiInstance) return;
    const capPort =
      typeof caps === 'object' && caps !== null && 'port' in caps
        ? Number((caps as { port?: number }).port)
        : NaN;
    const idx = Number.isFinite(capPort) ? ports.indexOf(capPort) : -1;
    const dataDir = idx >= 0 ? dataDirs[idx] : dataDirs[0];
    if (dataDir) {
      process.env.DATAZEN_DATA_DIR = dataDir;
    }
  },
  before: async function () {
    await runSessionBootstrap();
  },
  beforeSuite: async function (suite) {
    beginJourneySuite(suite.file);
    try {
      const handles = await browser.getWindowHandles();
      if (handles.length > 1) {
        // Only close sub-windows when there are leftover ones
        await browser.url('tauri://localhost');
        await browser.pause(200);
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
      }
      await ensureMainWindowForIpc();
      await invokeBackend('get_settings');
      await browser.pause(300);
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
  },
};
