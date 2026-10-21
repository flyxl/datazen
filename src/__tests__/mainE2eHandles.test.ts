/**
 * The E2E suites read the table state through `window.__tableDataStore`
 * (e.g. `e2e/specs/table-cell-selection.ts`). A module-level assignment inside the store
 * file is tree-shaken by Vite, so the handle has to be registered in the **entry module** —
 * and it is gated on `VITE_E2E`, because the webdriver build (the only build that runs the
 * E2E suites, see `scripts/e2e-tauri-build.mjs`) is the only one that needs it.
 *
 * This is a wiring test: the failure mode it guards against is silent — the handle simply
 * is not there any more and every E2E case that reads the store fails with a null snapshot.
 */
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = path.resolve(import.meta.dirname, '..');

function readSrc(...parts: string[]) {
  return fs.readFileSync(path.join(ROOT, ...parts), 'utf8');
}

describe('E2E handle wiring', () => {
  it('registers __tableDataStore in the entry module behind the VITE_E2E gate', () => {
    const main = readSrc('main.tsx');
    expect(main).toMatch(
      /if\s*\(\s*import\.meta\.env\.VITE_E2E\s*\)\s*\{[\s\S]{0,300}?__tableDataStore\s*=/,
    );
  });

  it('never publishes the table store handle unconditionally', () => {
    const main = readSrc('main.tsx');
    // `__schemaStore` is published unconditionally; the table store must not copy that
    // pattern — a production build must not grow a global handle nobody asked for.
    expect(main).not.toMatch(/\(globalThis as any\)\.__tableDataStore/);
    expect(main).not.toMatch(/\(globalThis as Record<string, unknown>\)\.__tableDataStore/);
  });

  it('publishes undefined rather than the store outside an E2E build', () => {
    const main = readSrc('main.tsx');
    // The else branch keeps the handle name in the bundle (the assignment survives
    // tree-shaking) without exposing a live store in a production build.
    expect(main).toMatch(/__tableDataStore\s*=\s*undefined;/);
  });
});
