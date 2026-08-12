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
      all: false,
      include: ['e2e/contract/fixtures.ts', 'e2e/contract/journeys/plan.ts'],
      exclude: [
        '**/node_modules/**',
        '**/*.test.ts',
        '**/__tests__/**',
        'e2e/contract/open-fixture.ts',
        'e2e/contract/journeys/run-*.ts',
      ],
      thresholds: {
        'e2e/contract/fixtures.ts': {
          lines: 80,
          statements: 80,
          functions: 80,
          branches: 70,
        },
        'e2e/contract/journeys/plan.ts': {
          lines: 80,
          statements: 80,
          functions: 80,
          branches: 70,
        },
      },
    },
  },
});
