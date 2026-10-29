import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

/**
 * Option C coverage gate (approved):
 * - Core: lib / stores / DataTable / ai components ≥80%
 * - Windows: Connection shell, Workflow package, SettingsContent, MainPage ≥80%
 * - Thin `src/commands/**` invoke wrappers and React chart shells stay
 *   out of the fail gate (logic covered via lib/chart + E2E).
 */
export default defineConfig({
  plugins: [react()],
  build: {
    assetsInlineLimit: 0,
  },
  resolve: {
    alias: {
      // form-split F6-c2: the @datazen/* alias table is retired — package
      // exports (workspace links) resolve every @datazen specifier, exactly
      // as vite and tsc now do. The one alias that remains is test
      // infrastructure, not package resolution: the shared test setup mocks
      // the Tauri window API for jsdom, and app-owned vitest configs keep
      // this mock alias per plan §5.6 item 6.
      '@tauri-apps/api/window': resolve(__dirname, 'packages/frontend/src/test/mocks/tauriWindow.ts'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./packages/frontend/src/test/setup.ts'],
    // Vitest's 5s default is not a defensible budget for this suite. Measured
    // per-test wall time over all 5702 Host tests (driver set `all`, macOS,
    // 8 cores) at three contention levels, via `--reporter=json`:
    //
    //   2 workers,  load 13  (a local --maxWorkers=2 low-load run)  p95 57ms  p99 170ms  max  1745ms  0 failures
    //   8 workers,  load  7  (mild oversubscription)                p95 118ms p99 336ms  max  4392ms  0 failures
    //   8 workers,  load 48  (6-7x oversubscription)                p95 329ms p99 918ms  max 11629ms  1 timeout
    //
    // The 8-worker / load-7 row is the one that matters: a single test already
    // sits at 88% of a 5s budget on this host, so further contention has no
    // headroom left. No multiple of any measured figure is quoted here, and
    // docs/development/ci-test-matrix.md §2.1.1 forbids rebuilding one in this
    // form: the load-7 row is one contention level on one host, so "10s is Nx
    // that" asserts a baseline this file does not control, and it would sit
    // right where "no headroom left" makes a reader take any number as overall
    // headroom. Scoped to the load-7 row, NOT a margin claim: 11629ms (worst
    // overall) exceeds 10s outright. This project does not control the GitHub
    // runner spec, so nothing here claims what a hosted runner looks like.
    //
    // SCOPE: this key lives in the root `test` block, so it applies to every
    // glob in `include` below — to `pnpm test:unit` (basic driver set) AND to
    // `pnpm test:unit:driver-set` (all path drivers). It is NOT scoped to `all`.
    //
    // Scope of the guarantee: testTimeout can only fire for tests that YIELD
    // to the event loop. In the load-48 run, DiffDetail (11629ms), resolve-pro
    // (6540ms) and tableDataStore (5236ms) all overran 5s and still PASSED,
    // because they are CPU-bound and never yield; the only test that actually
    // timed out was DataTransferWindow, which awaits. So this raise fixes the
    // awaiting-timeout flake class and deliberately leaves pathological 6x
    // oversubscription still failing — a hang must stay distinguishable from
    // slowness. Cost: a genuinely hung test now burns 10s instead of 5s before
    // reporting. See docs/development/ci-test-matrix.md.
    testTimeout: 10_000,
    // Runtime jitter is absorbed here, NOT by any assumption about how many
    // cores a hosted runner has. Value 2 rather than 1: the failure class is
    // contention, which can span a stretch of tests, so one immediate re-run
    // need not land after the contention passes. The cost is only paid when a
    // test has already failed, and it stays bounded — worst case a hanging
    // test reports after 3 x 10s = 30s rather than retrying forever.
    // COST, stated plainly: a test that fails and then passes on retry is
    // reported as PASSED, so an all-green run is NOT evidence that no
    // intermittent failure occurred. Do not read retry counts as zero from a
    // green exit code; that inference is invalid by construction.
    // Root-level, so — like testTimeout — it covers both CI unit-test steps.
    retry: 2,
    // Hooks need their own budget because they do categorically different work
    // from tests: they stage directory trees, shell out to git, and recursively
    // delete what they staged. Nothing in the per-test measurement above bounds
    // any of that.
    //
    // Measured, unloaded, single-file run on this host: removing the staging
    // tree in driver-init-rustfmt's `afterAll` took 13158ms — past Vitest's
    // 10000ms hook default with no contention at all. That suite therefore
    // reported `Test Files 1 failed` on every run while its 8 tests all passed.
    // A hook timeout that fires on a green suite is a false negative of exactly
    // the kind testTimeout above was raised to stop producing: it costs a full
    // re-run and trains people to re-run instead of read the result.
    //
    // 120_000 is NOT a margin claim derived from 13158ms, and nothing here
    // asserts that 10s is Nx anything — the measurement above is a single
    // unloaded sample and does not support a multiplier, for the same reason
    // docs/development/ci-test-matrix.md §2.1.1 rejects one for testTimeout. It
    // is the budget this repo already hands its slowest hooks explicitly
    // (driver-init-rustfmt's `beforeAll`, resolve-pro); aligning the global
    // default with that avoids inventing a second, differently-derived ceiling
    // for the same class of work.
    //
    // What this costs, stated plainly: a genuinely hung hook now takes up to 120s to
    // report instead of 10s, for every suite matched by `include`. That is the
    // price of not letting legitimate teardown be mistaken for a hang. An
    // explicit per-hook timeout, as on the `afterAll` above, still overrides
    // this and is the right tool for a hook with a known worst case.
    hookTimeout: 120_000,
    include: [
      'packages/frontend/src/**/*.test.{ts,tsx}',
      // form-split F6-c3: host↔driver integration fixtures (redis key web
      // context menu) live with the host package, outside src/ so the
      // host-production rules (R3/R6/R7) do not adjudicate test scaffolding.
      'packages/frontend/__tests__/**/*.test.{ts,tsx}',
      'apps/desktop/src/**/*.test.{ts,tsx}',
      'scripts/__tests__/**/*.test.{ts,mjs}',
      // SDK package tests run on the Host toolchain too (PRD F8).
      'packages/wapp-sdk/__tests__/**/*.test.{ts,tsx}',
      'packages/extension-points/__tests__/**/*.test.{ts,tsx}',
      'packages/extension-points/src/__tests__/**/*.test.{ts,tsx}',
      'packages/driver-sdk/__tests__/**/*.test.{ts,tsx}',
      'packages/backend-client/__tests__/**/*.test.{ts,tsx}',
      'packages/ui/src/__tests__/**/*.test.{ts,tsx}',
    ],
    coverage: {
      provider: 'v8',
      include: [
        'packages/frontend/src/lib/**/*.{ts,tsx}',
        'packages/frontend/src/stores/**/*.{ts,tsx}',
        'packages/frontend/src/components/DataTable/**/*.{ts,tsx}',
        'packages/frontend/src/components/ai/**/*.{ts,tsx}',
        'packages/frontend/src/windows/connection/ConnectionPage.tsx',
        'packages/frontend/src/windows/connection/ObjectBrowser.tsx',
        'packages/frontend/src/windows/connection/PrivilegeView.tsx',
        'packages/frontend/src/components/FilterEditor.tsx',
        // The filter editor's parts live next to it since 08 Step 9; the
        // coverage gate has to see them, or the split would quietly shrink
        // the denominator instead of moving it.
        'packages/frontend/src/components/filter/**/*.{ts,tsx}',
        'packages/frontend/src/components/query/BindParamPanel.tsx',
        'packages/frontend/src/components/connection/SshTunnelFields.tsx',
        'packages/frontend/src/windows/workflow/**/*.{ts,tsx}',
        'packages/frontend/src/windows/settings/SettingsContent.tsx',
        'packages/frontend/src/windows/main/MainPage.tsx',
        'packages/frontend/src/windows/welcome/WelcomePage.tsx',
        'packages/frontend/src/windows/dashboard/**/*.{ts,tsx}',
        'packages/frontend/src/windows/settings/sections/GeneralSection.tsx',
        'packages/frontend/src/windows/settings/sections/ConnectionsSection.tsx',
        'packages/frontend/src/windows/settings/sections/DriversSection.tsx',
        'packages/frontend/src/windows/settings/sections/AiSection.tsx',
        'packages/frontend/src/windows/settings/sections/PluginsSection.tsx',
        'packages/frontend/src/windows/settings/sections/ShortcutsSection.tsx',
        'packages/frontend/src/windows/settings/sections/AppearanceSection.tsx',
        'packages/frontend/src/windows/settings/sections/AboutSection.tsx',
        'packages/frontend/src/windows/settings/sections/TelemetrySection.tsx',
        'packages/frontend/src/windows/settings/sections/BackupSection.tsx',
        'packages/frontend/src/windows/settings/sections/McpSettingsSection.tsx',
      ],
      exclude: [
        'packages/frontend/src/**/__tests__/**',
        'packages/frontend/src/**/*.d.ts',
        'packages/frontend/src/test/**',
        'packages/frontend/src/plugins/**',
        'packages/frontend/src/locales/**',
        'packages/frontend/src/lib/chart/renderers/**',
        'packages/frontend/src/types/**',
      ],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 75,
        statements: 80,
      },
    },
  },
});
