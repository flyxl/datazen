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
    // per-test wall time over all 5691 Host tests (driver set `all`, macOS,
    // 8 cores) at three contention levels, via `--reporter=json`:
    //
    //   2 workers,  load 13  (shape of a 2-vCPU CI runner)  p95 57ms  p99 170ms  max  1745ms  0 failures
    //   8 workers,  load  7  (mild oversubscription)        p95 118ms p99 336ms  max  4392ms  0 failures
    //   8 workers,  load 48  (6-7x oversubscription)        p95 329ms p99 918ms  max 11629ms  1 timeout
    //
    // The 8-worker / load-7 row is the one that matters: a single test already
    // sits at 88% of a 5s budget on an 8-core dev box, so a GitHub-hosted
    // runner (2-4 vCPU) with any competing load has no headroom left. 10s is
    // ~2.3x that measured worst case and ~4.4x the load-7 p99.9 (2299ms).
    //
    // Scope of the guarantee: `testTimeout` can only fire for tests that YIELD
    // to the event loop. In the load-48 run, DiffDetail (11629ms), resolve-pro
    // (6540ms) and tableDataStore (5236ms) all overran 5s and still PASSED,
    // because they are CPU-bound and never yield; the only test that actually
    // timed out was DataTransferWindow, which awaits. So this raise fixes the
    // awaiting-timeout flake class and deliberately leaves pathological 6x
    // oversubscription still failing — a hang must stay distinguishable from
    // slowness. Cost: a genuinely hung test now burns 10s instead of 5s before
    // reporting. See docs/development/ci-test-matrix.md.
    testTimeout: 10_000,
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
