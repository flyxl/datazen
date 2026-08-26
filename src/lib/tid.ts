/**
 * E2E-only stable locator attributes.
 *
 * `tid(id)` spreads to `{ 'data-testid': id }` when the bundle is built with
 * `VITE_E2E=1` (E2E / webdriver builds, see scripts/e2e-tauri-build.mjs) and to
 * `{}` otherwise, so production builds render no test attributes at all.
 *
 * Naming convention: `<area>-<element>-<action>` in kebab-case
 * (e.g. `editor-execute-button`, `backup-run-button`).
 */
export type TidAttrs = { 'data-testid': string } | Record<string, never>;

export const tid = (id: string): TidAttrs =>
  import.meta.env.VITE_E2E ? { 'data-testid': id } : {};
