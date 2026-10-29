import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

/** Path-driver UI unit tests — not part of Host `pnpm test:unit`. */
export default defineConfig({
  plugins: [react()],
  // form-split F6-c2: the @datazen/* alias table is retired. Driver UI
  // trees carry no package.json, so their `@datazen/*` imports resolve by
  // walking up to the repository root's node_modules, where the workspace
  // aggregator links live (root devDependencies). The host suite proves the
  // same resolution works for manifest-carrying consumers.
  test: {
    environment: 'jsdom',
    setupFiles: ['./packages/frontend/src/test/setup.ts', './packages/frontend/src/test/driverUiSetup.ts'],
    include: ['packages/drivers/**/*.test.{ts,tsx}', 'packages/drivers/**/__tests__/**/*.{ts,tsx}'],
    exclude: [
      '**/node_modules/**',
      'packages/drivers/kiwi/**',
      'packages/drivers/superset/**',
    ],
  },
});
