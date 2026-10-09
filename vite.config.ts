import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { resolve } from 'path';

// Track B: the Pro extension is NOT statically bundled. PROD loads the staged
// builtin-ep bundle at runtime; DEV serves the extension source from disk via
// /@fs (see generated-pro.ts). No alias to the extension source here — a
// static alias would bundle a second copy of Pro and double-register it.
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
    // Force @codemirror's state/view to a single instance for BOTH the host
    // editor and @codemirror/lint, so the linter's ViewPlugin attaches to the
    // editor's own state/view. Without this, @codemirror/lint's nested copy of
    // @codemirror/view silently becomes a different instance and the linter is
    // never scheduled (no diagnostics, no squiggle).
    dedupe: ['@codemirror/state', '@codemirror/view'],
    alias: {
      '@datazen/backend-client': resolve(__dirname, 'packages/backend-client/src/index.ts'),
      '@datazen/driver-sdk': resolve(__dirname, 'packages/driver-sdk/src/index.ts'),
      '@datazen/extension-points': resolve(__dirname, 'packages/extension-points/src/index.ts'),
      '@datazen/wapp-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
      '@datazen/extension-sdk': resolve(__dirname, 'packages/wapp-sdk/src/index.ts'),
      '@datazen/ui': resolve(__dirname, 'packages/ui/src/index.ts'),
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
