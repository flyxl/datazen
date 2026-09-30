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
import { ensureMainWindowForIpc, ensureSeededPgSessionFresh, invokeBackend } from './helpers.js';
import { browser } from '@wdio/globals';

const WD_PORT = parseInt(process.env.E2E_WD_PORT || '4445', 10);

/**
 * Pin the main window for the whole run.
 *
 * Without this the window keeps whatever geometry the host monitor hands it, so
 * `saveScreenshot` returns a different pixel size on every machine and every
 * display. That is how site/assets/screenshots/ accumulated 12 different
 * resolutions — unusable for stitching into a video.
 *
 * ## The window size is a layout constraint, not a video constraint
 *
 * The Tauri webdriver returns a screenshot that is 1:1 with the window's CSS
 * size and ignores devicePixelRatio. Widening the window therefore does not buy
 * resolution, it buys *more UI in the frame* — and DataZen has a real minimum:
 * below about 1280x800 CSS the connection rail, navigator, toolbar, query
 * editor and result grid no longer all fit, and the capture silently clips the
 * right-hand panel. A clipped screenshot is worse than a wrong-sized one.
 *
 * So capture runs at `fit16x9` — the largest 16:9 viewport the display can show
 * (1440x810 on a 1440x900 screen). Only height is given up, which the app has
 * plenty of room for, so the whole app is in frame and the capture is already
 * a 16:9 image. `fullscreen` (1440x900) also works but is 16:10, so it has to be
 * cropped down to 16:9 later, which costs the title bar. Resizing every image to
 * 1920x1080 for the video is a separate, purely pixel-level step
 * (`scripts/normalize-screenshots.mjs`).
 *
 * Opt-in via E2E_WINDOW_SIZE, which is either the literal `fullscreen` or an
 * explicit "<w>x<h>" in CSS px. Behaviour specs leave it unset so
 * responsive-layout assertions keep testing the real default
 * (MAIN_WINDOW_DEFAULT_W x _H = 1280x900).
 */
const PINNED_WINDOW_SIZE = process.env.E2E_WINDOW_SIZE || '';

let _windowPinned = false;
async function pinWindowGeometry(): Promise<void> {
  if (!PINNED_WINDOW_SIZE || _windowPinned) return;
  _windowPinned = true;

  let w: number;
  let h: number;
  if (PINNED_WINDOW_SIZE === 'fullscreen' || PINNED_WINDOW_SIZE === 'fit16x9') {
    try {
      const screen = await browser.execute(() => ({
        w: window.screen.width,
        h: window.screen.height,
      }));
      w = Number(screen?.w);
      h = Number(screen?.h);
    } catch (e) {
      console.warn(`[wdio] could not read screen size: ${String(e)}`);
      return;
    }
    if (!w || !h) {
      console.warn('[wdio] screen size came back empty; leaving the window alone');
      return;
    }
    if (PINNED_WINDOW_SIZE === 'fit16x9') {
      // Largest 16:9 viewport the display can actually show. Only height is
      // given up (810 of 900 on a 1440x900 screen), which the app has plenty of
      // room for, so nothing clips — and the capture already *is* 16:9, so it
      // needs no cropping to become a video frame.
      h = Math.min(h, Math.round((w * 9) / 16));
    }
  } else {
    const m = /^(\d+)\s*[x×]\s*(\d+)$/.exec(PINNED_WINDOW_SIZE);
    if (!m) {
      console.warn(
        `[wdio] E2E_WINDOW_SIZE="${PINNED_WINDOW_SIZE}" is neither "fullscreen" nor "<w>x<h>"; ignoring.`,
      );
      return;
    }
    w = Number(m[1]);
    h = Number(m[2]);
  }

  try {
    await browser.setWindowSize(w, h);
    console.log(`[wdio] window pinned to ${w}x${h} (CSS px)`);
  } catch (e) {
    // The window may not be addressable yet on the very first suite.
    console.warn(`[wdio] could not pin window size: ${String(e)}`);
  }
}

/** Per-worker isolated database name — set in runSessionBootstrap, dropped in after. */
let _workerDb: string | undefined;

const capabilities: WebdriverIO.Capabilities[] = [{}];

async function runSessionBootstrap() {
  await browser.url('tauri://localhost');
  await browser.pause(2000);
  // Ensure we're on the main page — the app may start on settings
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

  // Database-fixture-only suites can opt out of global worker DB creation and
  // seeding. This keeps their writes scoped to their own unique-prefix tables.
  if (process.env.E2E_SKIP_WORKER_DATABASE !== '1') {
    _workerDb = createWorkerDatabase();
    await seedDefaultPgConnection(browser, _workerDb);
  }

  // The Tauri process (and its ConnectionManager) is reused across spec files,
  // so the live `conn_e2e_pg` session can still be bound to the previous
  // worker's database, which `after` just dropped. Backend `connect` hands that
  // still-alive session back unchanged, so specs that connect through the raw
  // IPC path (no workspace connect) would run against a database that no longer
  // exists. Specs that drive the workspace UI already re-bind via their own
  // connect; this covers the rest.
  await ensureSeededPgSessionFresh();

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
  // Gallery capture is a separate, deliberate task, not part of the suite.
  //
  // Every `*screenshot*` spec rewrites committed binaries under
  // site/assets/screenshots, so running one from a plain `pnpm e2e` would
  // silently republish whatever happened to be on the developer's screen. They
  // also want a hand-sized, maximized window and seeded demo data, which a
  // normal test run cannot arrange. Opt in with `--capture` (`pnpm e2e:shots`).
  exclude: process.env.E2E_CAPTURE
    ? []
    : [
        './specs/zz-screenshots.ts',
        './specs/demo-recording.ts',
        './specs/zz-diag.ts',
        './specs/*screenshot*.ts',
      ],
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
      './specs/connection-zero-state.ts',
      './specs/drag-drop-groups.ts',
      './specs/backup-database.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/data-sync-window.ts',
      './specs/window-operations.ts',
      './specs/unified-tab-bar.ts',
      './specs/wapps.spec.ts',
      './specs/ui-extract-dialogs.ts',
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
      './specs/data-sync-tuple-range.ts',
      './specs/data-sync-unknown-outcome.ts',
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
    // Driver-owned MySQL/PostgreSQL schema-tree object journeys. These create
    // isolated fixtures and clean them up in the driver E2E specs.
    'schema-tree-objects': [
      '../packages/drivers/mysql/e2e/schema-tree-objects.ts',
      '../packages/drivers/postgres/e2e/schema-tree-objects.ts',
    ],
    // Host-owned SQL editor gestures: Mod+D, multi-cursor, rectangular
    // selection (`pnpm e2e:sql-editor-prod`). Both specs are Community
    // behaviour, so this suite runs on any build. The Pro-only counterparts
    // (paste-as-IN, schema-tree drop) live in the Pro package's own e2e dir
    // and are covered by `pro-sql-editor` above.
    'sql-editor-prod': [
      './specs/sql-editor-productivity.ts',
      './specs/sql-editor-multicursor-gestures.ts',
    ],
    // SQL editor intelligence (completion, hover, inlay hints, star expansion)
    // and the AI error-diagnosis journey. These two specs were written but never
    // listed in any suite, so 27 cases had never executed — and 11 of their
    // assertions read `expect(typeof x).toBe('boolean')`, which holds for any
    // value, so even a run would have proved nothing. Whether the completion
    // popup opened had never actually been checked. They are listed now.
    //
    // Kept out of `sql-editor-prod` on purpose: that suite promises Community
    // behaviour. What remains here after the split is completion only — alias,
    // FROM-table and WHERE-column — all implemented in the host. The Pro half
    // (intentions, inlay hints, hover, definition navigation, FK JOIN
    // completion) has moved next to its implementation, to
    // `packages/pro-extensions/sql-editor-pro/e2e/specs/sql-editor-intelligence.ts`,
    // and is picked up by the `pro-sql-editor` glob above. This suite therefore
    // needs a Pro build too, because the AI error-diagnosis journey alongside it
    // asserts Pro behaviour: `pnpm e2e:pro:skip-build -- --suite sql-editor-intelligence`.
    'sql-editor-intelligence': [
      './specs/sql-editor-intelligence.ts',
      './specs/sql-editor-ai-error.ts',
    ],
    // SQL Editor Pro enhanced features (S4-A statement frame/gutter, S5-B bind-param panel),
    // migrated to the Pro extension's own e2e dir — requires a Pro build:
    // `pnpm e2e:pro:sql-editor`. Not part of the default Community run.
    // `!(*-screenshot)` keeps the gallery-capture spec below out of this
    // behaviour suite, matching the host-side `*screenshot*` exclusion.
    'pro-sql-editor': ['../packages/pro-extensions/sql-editor-pro/e2e/specs/!(*-screenshot).ts'],
    // Query Builder journeys belong to the Pro extension and require its test bridge.
    'pro-query-builder': [
      '../packages/pro-extensions/sql-editor-pro/e2e/specs/journeys/visual-query-builder-*.ts',
    ],
    // Release-gallery captures that need the Pro build but assert no behaviour
    // (`pnpm e2e:qb:shot`). Kept out of pro-sql-editor so a normal regression
    // run never rewrites files in site/assets/screenshots/.
    'pro-screenshots': ['../packages/pro-extensions/sql-editor-pro/e2e/specs/*-screenshot.ts'],
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
      './specs/journeys/data-transfer-tuple-recordset-journey.ts',
      './specs/journeys/data-transfer-fk-order-journey.ts',
    ],
    // Schema Diff only (`pnpm e2e:schema-diff`)
    'schema-diff': [
      './specs/schema-diff-window.ts',
      './specs/schema-diff-dependency-order.ts',
      './specs/schema-diff-unified-planner.ts',
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
      './specs/journeys/data-transfer-tuple-recordset-journey.ts',
      './specs/journeys/data-transfer-fk-order-journey.ts',
      './specs/journeys/data-transfer-type-mapping-journey.ts',
      './specs/connection-navigator-expansion.ts',
      './specs/journeys/zero-state-query-journey.ts',
      './specs/journeys/connection-create-journey.ts',
      './specs/journeys/connection-browse-journey.ts',
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-result-chart-journey.ts',
      './specs/journeys/query-recovery-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/query-row-limit-journey.ts',
      './specs/journeys/first-run-edge-journey.ts',
      // First-run journey (onboarding wizard). Runs last on purpose: its cases
      // rewrite the onboarding gate and restore it in `after`.
      './specs/journeys/onboarding-journey.ts',
    ],
    // Blast-radius guard for the Query Builder work (`pnpm e2e:qb:regression`).
    // The builder shares the query panel, the SQL editor host and the
    // navigator's `onSelectTable`, so these are the specs that would catch a
    // layout / execution-gate / navigation regression from it.
    // Run with one instance per spec (`--instances 5`): several of these specs
    // churn connections, and sharing one app process makes a later spec's schema
    // tree time out on state the earlier spec left behind.
    //
    // `sql-query.ts` is intentionally absent: its SQ-CTX-001 assertion fails on
    // a pristine checkout too (the database context selector reports the
    // default database instead of the qualified path's), so including it would
    // keep this guard permanently red. The Postgres double-quote lint that this
    // guard exists to protect is covered by unit tests in
    // `src/windows/connection/__tests__/query.modules.test.tsx`.
    'qb-regression': [
      './specs/journeys/connection-query-journey.ts',
      './specs/journeys/query-edge-journey.ts',
      './specs/journeys/query-toolbar-responsive-journey.ts',
      './specs/connection-navigator-expansion.ts',
      './specs/table-data.ts',
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
      './specs/data-sync-tuple-range.ts',
    ],
    // Real scheduled/unattended migration workflow journeys. Opt in with
    // E2E_MIGRATION_LIVE=1; PostgreSQL/MySQL are skipped explicitly when the
    // live fixture is unavailable.
    'migration-live-workflow': ['./specs/migration-live-workflow.ts'],
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
    await pinWindowGeometry();
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
    // Drop the per-worker isolated database.
    if (_workerDb) {
      dropWorkerDatabase(_workerDb);
      _workerDb = undefined;
    }
  },
};
