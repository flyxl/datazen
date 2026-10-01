import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

/** Path-driver UI unit tests — not part of Host `pnpm test:unit`. */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // `packages/driver-sdk/src/ipc/desktopBinding.ts` re-exports the
      // BackendClient surface, so driver UI tests transitively need this alias.
      // Without it every driver UI file that touches the ipc layer fails to
      // resolve the import — 48 files, while the host suite stays green.
      '@datazen/backend-client': resolve(__dirname, 'packages/backend-client/src/index.ts'),
      '@datazen/driver-sdk': resolve(__dirname, 'packages/driver-sdk/src/index.ts'),
      '@datazen/extension-points': resolve(__dirname, 'packages/extension-points/src/index.ts'),
      '@datazen/wapp-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
      '@datazen/ui': resolve(__dirname, 'packages/ui/src/index.ts'),
      '@datazen/extension-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts', './src/test/driverUiSetup.ts'],
    include: ['packages/drivers/**/*.test.{ts,tsx}', 'packages/drivers/**/__tests__/**/*.{ts,tsx}'],
    exclude: [
      '**/node_modules/**',
      'packages/drivers/kiwi/**',
      'packages/drivers/olap/**',
      'packages/drivers/superset/**',
    ],
  },
});
