import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

/** Path-driver UI unit tests — not part of Host `pnpm test:unit`. */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: [
      'packages/drivers/**/*.test.{ts,tsx}',
      'packages/drivers/**/__tests__/**/*.{ts,tsx}',
    ],
    exclude: [
      '**/node_modules/**',
      'packages/drivers/kiwi/**',
      'packages/drivers/olap/**',
      'packages/drivers/superset/**',
    ],
  },
});
