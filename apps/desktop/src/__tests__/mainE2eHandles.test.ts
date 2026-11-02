/**
 * The E2E suites read the table state through `window.__tableDataStore`
 * (e.g. `e2e/specs/table-cell-selection.ts`); the FK-navigation and panel
 * specs read `__fkNavigationStore` / `__panelStore` the same way
 * (`e2e/specs/table-fk-navigation.ts`). A module-level assignment inside a
 * store file is tree-shaken by Vite, so the handles have to be registered in
 * the **bootstrap module that the entry evaluates** — since host-boundary
 * follow-up F2 that is the shared app bootstrap
 * (`packages/frontend/src/bootstrap/startSharedApp.tsx`), called by
 * `apps/desktop/src/main.tsx`. The handles are live in the two builds that
 * need them: the webdriver build (`VITE_E2E=1`, see
 * `scripts/e2e-tauri-build.mjs`) and the dev server (`DEV=true`); a
 * production build publishes `undefined` instead.
 *
 * This is a wiring test: the failure modes it guards against are silent —
 * a handle simply is not there any more and every E2E case that reads it
 * fails with a null snapshot, or a **second owner** overwrites the first
 * (an imported module body evaluates before the importer's). The
 * assertions are **scope-aware** rather than window-regex: the if/else
 * blocks are extracted by a brace-balanced scanner, so comments and new
 * handles may grow the gate freely, while an assignment moved out of the
 * gate — or a registration added to a store file — goes red with a
 * precise message instead of relying on a `[\s\S]{0,300}` proximity window.
 */
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const APP_SRC = path.resolve(import.meta.dirname, '..');
const FRONTEND_SRC = path.resolve(import.meta.dirname, '../../../../packages/frontend/src');

function readApp(...parts: string[]) {
  return fs.readFileSync(path.join(APP_SRC, ...parts), 'utf8');
}

function readFrontend(...parts: string[]) {
  return fs.readFileSync(path.join(FRONTEND_SRC, ...parts), 'utf8');
}

/** The gated handles: global name, the live store hook, and the file that owns the store. */
const GATED_HANDLES = [
  { global: '__tableDataStore', store: 'useTableDataStore', file: 'stores/tableDataStore.ts' },
  {
    global: '__fkNavigationStore',
    store: 'useFkNavigationStore',
    file: 'stores/fkNavigationStore.ts',
  },
  { global: '__panelStore', store: 'usePanelStore', file: 'stores/panelStore.ts' },
];

/**
 * Slice the body of the brace block whose `{` sits at `open`, skipping
 * comments and string/template literals so braces inside them cannot skew
 * the depth count. Fails (throws) on unbalanced input — a wiring test must
 * not silently fall back to a looser check.
 */
function balancedBlockBody(source: string, open: number): string {
  let depth = 0;
  let i = open;
  while (i < source.length) {
    const ch = source[i];
    const next = source[i + 1];
    if (ch === '/' && next === '/') {
      i = source.indexOf('\n', i);
      if (i < 0) throw new Error('unterminated line comment');
      continue;
    }
    if (ch === '/' && next === '*') {
      i = source.indexOf('*/', i + 2);
      if (i < 0) throw new Error('unterminated block comment');
      i += 2;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      i += 1;
      while (i < source.length && source[i] !== ch) {
        if (source[i] === '\\') i += 1;
        i += 1;
      }
      if (i >= source.length) throw new Error(`unterminated ${ch} literal`);
      i += 1;
      continue;
    }
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
    i += 1;
  }
  throw new Error('unbalanced braces');
}

/** Index of the `{` that closes `match` (whose pattern ends with the brace). */
function openBraceIndex(match: RegExpExecArray): number {
  return match.index + match[0].length - 1;
}

describe('E2E handle wiring', () => {
  const bootstrap = readFrontend('bootstrap/startSharedApp.tsx');

  // The gate: `if (import.meta.env.VITE_E2E || import.meta.env.DEV) {` —
  // operand order deliberately unconstrained (a reorder is not a wiring
  // change), but the set is: exactly these two flags.
  const gate = /if\s*\(\s*import\.meta\.env\.(VITE_E2E|DEV)\s*\|\|\s*import\.meta\.env\.(VITE_E2E|DEV)\s*\)\s*\{/;
  const gateMatch = gate.exec(bootstrap);
  expect(gateMatch, 'expected the E2E/dev gate in startSharedApp.tsx').not.toBeNull();
  const gatedBranch = balancedBlockBody(bootstrap, openBraceIndex(gateMatch!));
  expect(
    new Set([gateMatch![1], gateMatch![2]]),
    'the gate must test exactly VITE_E2E and DEV',
  ).toEqual(new Set(['VITE_E2E', 'DEV']));

  // The else branch: `} else {` immediately after the gated block.
  const afterGate = gateMatch!.index + gateMatch![0].length + gatedBranch.length;
  const elseMatch = /\}\s*else\s*\{/.exec(bootstrap.slice(afterGate));
  expect(elseMatch, 'expected an else branch publishing undefined').not.toBeNull();
  const productionBranch = balancedBlockBody(
    bootstrap,
    afterGate + elseMatch!.index + elseMatch![0].length - 1,
  );

  it.each(GATED_HANDLES)(
    'publishes $global live inside the E2E/dev branch only',
    ({ global: handle, store }) => {
      expect(gatedBranch).toMatch(new RegExp(`${handle}\\s*=\\s*${store}\\b`));
      // Every assignment lives inside the gate — single owner, and nothing
      // publishes the live store outside the two branches that own it.
      const assignments = bootstrap.match(new RegExp(`${handle}\\s*=`, 'g')) ?? [];
      expect(assignments.length, `${handle} must be assigned in exactly the two branches`).toBe(2);
    },
  );

  it.each(GATED_HANDLES)('$global publishes undefined in a production build', ({ global: handle }) => {
    expect(productionBranch).toMatch(new RegExp(`${handle}\\s*=\\s*undefined;`));
  });

  it.each(GATED_HANDLES)('keeps a single owner: $file must not register the handle', ({ global: handle, file }) => {
    // Two owners race: the imported module body evaluates before the importer's, so a second
    // registration would overwrite the first one — in a dev build that meant `undefined`
    // over the real store.
    expect(readFrontend(file), `${file} must not mention ${handle}`).not.toContain(handle);
  });

  it('keeps __schemaStore unconditional (the documented contrast)', () => {
    // `__schemaStore` is deliberately published outside the gate — production
    // included. Pin the contrast so the gated handles cannot quietly copy it.
    expect(bootstrap).toMatch(/__schemaStore\s*=\s*useSchemaStore/);
    expect(gatedBranch).not.toContain('__schemaStore');
    expect(productionBranch).not.toContain('__schemaStore');
  });

  it('wires the shared bootstrap into the desktop entry', () => {
    // The registration lives in the bootstrap; it only runs for the desktop when the
    // entry actually calls `startSharedApp` from '@datazen/frontend/app' (F2).
    const main = readApp('main.tsx');
    expect(main).toMatch(/from '@datazen\/frontend\/app'/);
    expect(main).toMatch(/startSharedApp\(/);
  });
});
