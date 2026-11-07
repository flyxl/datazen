import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/**
 * Web host build config (WR-E1).
 *
 * The web form is decided at COMPILE time here, exactly as the desktop form
 * is decided by the Tauri CLI: this build never exposes `TAURI_ENV_*`
 * (`envPrefix` stays at Vite's default `VITE_`), so `IS_DESKTOP_BUILD`
 * (`packages/frontend/src/platform/env.ts`) folds to `false` and the desktop
 * branch is dead-code-eliminated. No runtime probing anywhere — ruling #5.
 *
 * `base: './'` keeps every asset reference relative, which is what the
 * same-origin static deployment (WR-E5: the server host serves `dist/`
 * directly) needs; a sub-path mount keeps working without a rebuild.
 *
 * The Tailwind pipeline is the shared one: `@datazen/frontend`'s
 * `styles/globals.css` (imported by `startSharedApp`) owns `@config` /
 * `@source`, pointing at the desktop tailwind config — the single styling
 * source of truth, so the web build cannot drift from the desktop build.
 */
export default defineConfig({
  base: './',
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  // Desktop dev server holds 5173 (strictPort); the web host dev server
  // takes the next port so the two forms can run side by side.
  server: {
    port: 5174,
    strictPort: true,
  },
  resolve: {
    // Same single-instance guarantee as packages/desktop/vite.config.ts: the
    // host editor and @codemirror/lint must share one @codemirror/state and
    // one @codemirror/view, or the linter's ViewPlugin attaches to a
    // different instance and never schedules.
    dedupe: ['@codemirror/state', '@codemirror/view'],
  },
  build: {
    outDir: 'dist',
  },
});
