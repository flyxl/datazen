import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  resolve: {
    alias: {
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk'),
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
