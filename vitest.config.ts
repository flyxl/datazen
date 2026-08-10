import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

/**
 * Option C coverage gate (approved):
 * - Core: lib / stores / DataTable / ai components ≥80%
 * - Windows: Connection shell, Workflow package, SettingsWindow, MainWindow ≥80%
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
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: [
      'src/**/*.test.{ts,tsx}',
      'scripts/__tests__/**/*.test.{ts,mjs}',
      'packages/drivers/**/*.test.{ts,tsx}',
    ],
    coverage: {
      provider: 'v8',
      include: [
        'src/lib/**/*.{ts,tsx}',
        'src/stores/**/*.{ts,tsx}',
        'src/components/DataTable/**/*.{ts,tsx}',
        'src/components/ai/**/*.{ts,tsx}',
        'src/windows/connection/ConnectionWindow.tsx',
        'src/windows/connection/ConnectionSettingsDialog.tsx',
        'src/windows/workflow/**/*.{ts,tsx}',
        'src/windows/settings/SettingsWindow.tsx',
        'src/windows/main/MainWindow.tsx',
      ],
      exclude: [
        '**/*.test.{ts,tsx}',
        '**/__tests__/**',
        'src/locales/**',
        'src/plugins/generated.ts',
        'src/test/**',
      ],
      // Approved Option C metric is ≥80% *lines* per scoped package.
      // Statements/functions/branches are softer on React UI surfaces (many
      // handler/error branches are E2E-owned); pure lib/stores stay stricter.
      thresholds: {
        'src/lib/**': { lines: 80, statements: 80, functions: 75, branches: 70 },
        'src/stores/**': { lines: 80, statements: 80, functions: 75, branches: 55 },
        'src/components/DataTable/**': { lines: 80, statements: 80, functions: 70, branches: 60 },
        'src/components/ai/**': { lines: 80, statements: 75, functions: 60, branches: 60 },
        'src/windows/connection/ConnectionWindow.tsx': { lines: 80 },
        'src/windows/connection/ConnectionSettingsDialog.tsx': { lines: 80 },
        'src/windows/workflow/**': { lines: 80 },
        'src/windows/settings/SettingsWindow.tsx': { lines: 80 },
        'src/windows/main/MainWindow.tsx': { lines: 80 },
      },
    },
  },
});
