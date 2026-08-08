import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

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
    include: ['src/**/*.test.{ts,tsx}', 'scripts/__tests__/**/*.test.{ts,mjs}'],
    coverage: {
      provider: 'v8',
      include: [
        'src/lib/resolveUiLanguage.ts',
        'src/locales/index.ts',
      ],
      thresholds: {
        lines: 75,
        functions: 75,
        branches: 70,
        statements: 75,
      },
    },
  },
});
