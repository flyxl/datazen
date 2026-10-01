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
      '@datazen/driver-sdk': resolve(__dirname, 'packages/driver-sdk/src/index.ts'),
      '@datazen/extension-points': resolve(__dirname, 'packages/extension-points/src/index.ts'),
      '@datazen/wapp-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
      '@datazen/extension-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
      '@datazen/ui': resolve(__dirname, 'packages/ui/src/index.ts'),
      '@tauri-apps/api/window': resolve(__dirname, 'src/test/mocks/tauriWindow.ts'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    // Vitest's 5s default is not a defensible budget for this suite. Measured
    // per-test wall time over all 5714 Host tests (driver set `all`, macOS,
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
    include: [
      'src/**/*.test.{ts,tsx}',
      'scripts/__tests__/**/*.test.{ts,mjs}',
      // SDK package tests run on the Host toolchain too (PRD F8).
      'packages/wapp-sdk/__tests__/**/*.test.{ts,tsx}',
      'packages/extension-points/__tests__/**/*.test.{ts,tsx}',
      'packages/extension-points/src/__tests__/**/*.test.{ts,tsx}',
      'packages/driver-sdk/__tests__/**/*.test.{ts,tsx}',
      'packages/ui/src/__tests__/**/*.test.{ts,tsx}',
    ],
    coverage: {
      provider: 'v8',
      include: [
        'src/lib/**/*.{ts,tsx}',
        'src/stores/**/*.{ts,tsx}',
        'src/components/DataTable/**/*.{ts,tsx}',
        'src/components/ai/**/*.{ts,tsx}',
        'src/windows/connection/ConnectionPage.tsx',
        'src/windows/connection/ConnectionSettingsDialog.tsx',
        'src/windows/connection/ObjectBrowser.tsx',
        'src/windows/connection/PrivilegeView.tsx',
        'src/components/FilterEditor.tsx',
        'src/components/query/BindParamPanel.tsx',
        'src/components/connection/SshTunnelFields.tsx',
        'src/windows/workflow/**/*.{ts,tsx}',
        'src/windows/settings/SettingsContent.tsx',
        'src/windows/main/MainPage.tsx',
        'src/windows/welcome/WelcomePage.tsx',
        'src/windows/dashboard/**/*.{ts,tsx}',
        'src/windows/settings/sections/GeneralSection.tsx',
        'src/windows/settings/sections/ConnectionsSection.tsx',
        'src/windows/settings/sections/DriversSection.tsx',
        'src/windows/settings/sections/AiSection.tsx',
        'src/windows/settings/sections/PluginsSection.tsx',
        'src/windows/settings/sections/ShortcutsSection.tsx',
        'src/windows/settings/sections/AppearanceSection.tsx',
        'src/windows/settings/sections/AboutSection.tsx',
        'src/windows/settings/sections/TelemetrySection.tsx',
        'src/windows/settings/sections/BackupSection.tsx',
        'src/windows/settings/sections/McpSettingsSection.tsx',
      ],
      exclude: [
        'src/**/__tests__/**',
        'src/**/*.d.ts',
        'src/test/**',
        'src/plugins/**',
        'src/locales/**',
        'src/lib/chart/renderers/**',
        'src/types/**',
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
