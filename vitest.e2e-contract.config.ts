import { defineConfig } from 'vitest/config';
import { resolve } from 'path';

/**
 * Unit + coverage gate for Host Connection Contract pure modules (≥80% lines).
 */
export default defineConfig({
  resolve: {
    alias: {
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk'),
    },
  },
  test: {
    environment: 'node',
    include: ['e2e/contract/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      include: ['e2e/contract/**/*.ts'],
      exclude: ['e2e/contract/**/*.test.ts', 'e2e/contract/**/__tests__/**'],
      thresholds: {
        lines: 80,
        statements: 80,
        functions: 80,
        branches: 70,
      },
    },
  },
});
