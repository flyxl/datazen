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
      '@datazen/app-sdk': resolve(__dirname, 'packages/app-sdk/src/index.ts'),
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk/index.ts'),
      '@datazen/extension-sdk': resolve(__dirname, 'packages/app-sdk/src/index.ts'),
      '@datazen/ui': resolve(__dirname, 'packages/ui/src/index.ts'),
      '@tauri-apps/api/window': resolve(__dirname, 'src/test/mocks/tauriWindow.ts'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: [
      'src/**/*.test.{ts,tsx}',
      'scripts/__tests__/**/*.test.{ts,mjs}',
      // SDK package tests run on the Host toolchain too (PRD F8).
      'packages/app-sdk/__tests__/**/*.test.{ts,tsx}',
      'packages/extension-points/__tests__/**/*.test.{ts,tsx}',
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
