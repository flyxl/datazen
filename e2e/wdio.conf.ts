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
import { cleanupAppDataViaIpc, seedDefaultPgConnection } from './lib/testDataLifecycle.js';
import { ensureMainWindowForIpc, invokeBackend } from './helpers.js';
import { browser } from '@wdio/globals';

const WD_PORT = parseInt(process.env.E2E_WD_PORT || '4445', 10);

const capabilities: WebdriverIO.Capabilities[] = [{}];

async function runSessionBootstrap() {
  await browser.url('tauri://localhost');
  await browser.pause(2000);
  // Ensure we're on the main page — the app may start on welcome/settings
  try {
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 10000 });
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
            sqlExecutionStrategy: 'entire_script',
            // Every suite except `onboarding-journey` asserts the workspace, and
            // a wiped `e2e/.app-data` is a fresh install whose first-run journey
            // would otherwise replace MainPage. The journey spec flips this back
            // to `{ completed: false }` for its own cases.
            onboarding: { completed: true, version: 1 },
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
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 10000 });
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
    // (excludes Data Migration triad: schema-diff, data-sync, data-transfer)
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
      './specs/connection-empty-state.ts',
      './specs/welcome.ts',
      './specs/drag-drop-groups.ts',
      './specs/backup-database.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/data-sync-window.ts',
      './specs/window-operations.ts',
      './specs/unified-tab-bar.ts',
      './specs/wapps.spec.ts',
    ],
    // Real-DB Host specs incl. the host contract matrix (was `pnpm e2e:db`)
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
    // Host contract matrix × PG/MySQL/SQLite (`pnpm e2e:contract:matrix`,
    // `pnpm e2e:contract:pg` adds --mochaOpts.grep 'Host contract @ postgres')
    contract: ['./specs/host-contract-matrix.ts'],
    // Redis driver's own E2E, not part of default full run (`pnpm e2e:redis`)
    redis: ['../packages/drivers/redis/e2e/*.ts'],
    // SQL Editor Pro enhanced features (S4-A statement frame/gutter, S5-B bind-param panel),
    // migrated to the Pro extension's own e2e dir — requires a Pro build:
    // `pnpm e2e:pro:sql-editor`. Not part of the default Community run.
    'pro-sql-editor': ['../packages/pro-extensions/sql-editor-pro/e2e/specs/*.ts'],
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
    dashboard: ['./specs/data-dashboard*.ts'],
    // Data Transfer only (`pnpm e2e:data-transfer`)
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
    // Cross-module user journeys (`pnpm e2e:journeys`)
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
      './specs/journeys/welcome-query-journey.ts',
      './specs/journeys/connection-create-journey.ts',
      './specs/journeys/connection-browse-journey.ts',
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-result-chart-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/first-run-edge-journey.ts',
      // First-run journey (onboarding wizard). Runs last on purpose: its cases
      // rewrite the onboarding gate and restore it in `after`.
      './specs/journeys/onboarding-journey.ts',
    ],
    // First-run journey only (`pnpm e2e:onboarding`). Self-contained: it flips
    // the onboarding gate itself and restores it afterwards.
    onboarding: ['./specs/journeys/onboarding-journey.ts'],
    // Continuous failure/recovery and state-boundary paths
    // (`pnpm e2e:journeys:edge`)
    'journey-edge': [
      './specs/journeys/first-run-edge-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-edge-journey.ts',
    ],
    // Connection-specific edge cases (`pnpm e2e:connection:edge`).
    // These are module-level boundary specs, not cross-module journeys.
    'connection-edge': ['./specs/connection-validation.ts', './specs/connection-edge-cases.ts'],
    // Data Sync: UI smoke + edge cases + IPC + full journey (`pnpm e2e:data-sync`)
    'data-sync': [
      './specs/data-sync-window.ts',
      './specs/data-sync-edge-cases.ts',
      './specs/journeys/data-sync-journey.ts',
      './specs/data-sync-real.ts',
    ],
  },
  // Always 1 per WDIO process; multi-process parallelism via run.mjs --instances N.
  maxInstances: 1,
  // specFileRetries: 1, // disabled — retries double the time for genuine failures
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
    // Same Tauri process is reused across spec files; close leftover sub-windows
    // so Host specs do not attach to a previous MultiDb / SQLite session.
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
  },
};
