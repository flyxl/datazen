import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { resolve } from 'path';
import { existsSync } from 'fs';

const proLocalPkg = resolve(__dirname, 'packages/pro-extensions/sql-editor-pro/src/index.ts');
const proTmpFallback = resolve('/tmp/datazen-extension-sql-editor-pro/src/index.ts');
const proPath = existsSync(proLocalPkg) ? proLocalPkg : proTmpFallback;

export default defineConfig({
  // Tauri serves the webview from a custom protocol; absolute `/assets/...`
  // URLs can miss the asset handler and hit ipc:// (GET → "only POST and OPTIONS are allowed").
  base: './',
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  resolve: {
    alias: {
      '@datazen/driver-sdk': resolve(__dirname, 'packages/driver-sdk/src/index.ts'),
      '@datazen/extension-points': resolve(__dirname, 'packages/extension-points/src/index.ts'),
      '@datazen/app-sdk': resolve(__dirname, 'packages/app-sdk/src/index.ts'),
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk/index.ts'),
      '@datazen/extension-sdk': resolve(__dirname, 'packages/app-sdk/src/index.ts'),
      '@datazen/ui': resolve(__dirname, 'packages/ui/src/index.ts'),
      '@datazen/extension-sql-editor-pro': proPath,
      '@host/sql-editor': resolve(__dirname, 'src/components/sql-editor'),
      '@host': resolve(__dirname, 'src'),
    },
  },
  // Main window: index.html (with splash). Sub-windows: window.html (no splash).
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        window: resolve(__dirname, 'window.html'),
      },
    },
  },
});
