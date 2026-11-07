/**
 * Compile-time form determination (ruling #5).
 *
 * `TAURI_ENV_*` is injected by the Tauri CLI (`tauri dev` / `tauri build`)
 * and exposed to `import.meta.env` via the `envPrefix` in `vite.config.ts`.
 * A plain `vite dev` (browser mode) leaves it `undefined`, so this constant
 * folds at build time — the browser branch is dead-code-eliminated in a
 * desktop bundle and vice versa. Runtime `__TAURI_INTERNALS__` probing is
 * banned (form-split §4.2).
 */

export const IS_DESKTOP_BUILD: boolean =
  (import.meta.env as Record<string, string | undefined>).TAURI_ENV_PLATFORM !== undefined;
