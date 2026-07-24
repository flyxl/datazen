import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Cargo continuously replaces Windows executables while Tauri is compiling.
      // Watching those locked artifacts makes Node's FSWatcher fail with EBUSY.
      ignored: ['**/src-tauri/target/**'],
    },
  },
});
