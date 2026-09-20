/**
 * WebdriverIO config for DataZen Host E2E (Tauri embedded WebDriver plugin).
 *
 * ## Parallel execution (Tauri constraint)
 *
 * Tauri's WebDriver server accepts only one session at a time.  Parallelism is
 * therefore implemented *outside* WDIO:
 *
 * 1. `e2e/run.mjs` boots one `cargo run --features e2e` process per worker.
 * 2. Each worker gets its own free TCP port and isolated `DATAZEN_DATA_DIR`.
 * 3. Round-robin splits specs into N independent WDIO processes (each maxInstances: 1).
 *
 * Inside a single WDIO process, `maxInstances` must stay 1 — setting it higher
 * would open concurrent sessions against the same Tauri binary and hang.
 *
 * ## Spec suites (`--suite <name>`)
 *
 * Prefer named suites over long `--spec` lists so CI and local runs share the
 * same groupings.  See `suites` below.
 */

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Options } from '@wdio/types';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const WD_PORT = parseInt(process.env.E2E_WD_PORT || '4445', 10);
const HEADLESS = process.env.E2E_HEADLESS === '1' || process.env.CI === 'true';

export const config: Options.Testrunner = {
  runner: 'local',
  autoCompileOpts: {
    autoCompile: true,
    tsNodeOpts: {
      project: './tsconfig.json',
      transpileOnly: true,
    },
  },

  specs: ['./specs/**/*.ts'],
  exclude: ['./specs/zz-screenshots.ts', './specs/demo-recording.ts', './specs/zz-diag.ts'],

  /**
   * Named suites — referenced by `pnpm e2e -- --suite <name>` and by the run.mjs
   * config file (same resolution as `specs`). Keep in sync with docs:
   */
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
    connection: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/edit-delete-connection.ts',
      './specs/connection-search-group.ts',
      './specs/connection-empty-state.ts',
      './specs/connection-window.ts',
      './specs/connection-validation.ts',
      './specs/connection-edge-cases.ts',
      './specs/connection-navigator-expansion.ts',
      './specs/connection-zero-state.ts',
      './specs/drag-drop-groups.ts',
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
      './specs/journeys/zero-state-query-journey.ts',
      './specs/journeys/connection-create-journey.ts',
      './specs/journeys/tunnel-connection-journey.ts',
      './specs/journeys/connection-browse-journey.ts',
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-result-chart-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/visual-query-builder-journey.ts',
      './specs/journeys/first-run-edge-journey.ts',
      './specs/journeys/onboarding-journey.ts',
    ],
    onboarding: ['./specs/journeys/onboarding-journey.ts'],
    'journey-edge': [
      './specs/journeys/first-run-edge-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-edge-journey.ts',
    ],
    'connection-edge': ['./specs/connection-validation.ts', './specs/connection-edge-cases.ts'],
  },

  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      browserName: 'chrome',
      'goog:chromeOptions': {
        args: HEADLESS
          ? ['--headless=new', '--window-size=1440,900', '--disable-gpu']
          : ['--window-size=1440,900'],
      },
    },
  ],

  logLevel: (process.env.E2E_LOG_LEVEL as Options.Testrunner['logLevel']) || 'warn',
  bail: 0,
  baseUrl: `http://127.0.0.1:${WD_PORT}`,
  waitforTimeout: 15000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,

  hostname: '127.0.0.1',
  port: WD_PORT,
  path: '/',

  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 180000,
  },
};
