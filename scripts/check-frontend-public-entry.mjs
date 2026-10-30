#!/usr/bin/env node
/**
 * @datazen/frontend public-entry guard (host-boundary follow-up F2).
 *
 * The package's sanctioned surface is its narrow entries — `.` (the light
 * platform / capability surface), `./app` (the app bootstrap), `./shell`
 * (the chrome slice), `./styles.css` — plus a test-only mirror
 * `./testing/*`. This script turns that contract into a build-time gate:
 *
 *   R1  `@datazen/frontend/src/` is forbidden everywhere outside
 *       `packages/frontend` itself (inside the package, modules import each
 *       other relatively). Scans **every string-literal specifier** — plain
 *       `import`, `export … from`, dynamic `import()`, `vi.mock()` /
 *       `vi.doMock()`, `require()` — for the same reason
 *       `check-driver-import-boundaries.mjs` scans literals: keyword-shaped
 *       matching is exactly how violations stayed invisible before the guards
 *       existed.
 *
 *   R2  `@datazen/frontend/testing/` is allowed only inside test files
 *       (`__tests__/` directories, `*.test.*` / `*.spec.*` basenames).
 *       Production code reaching for internals through the test mirror is
 *       the same disease as R1 with a friendlier prefix.
 *
 * Scope: `apps/`, `packages/` (minus `packages/frontend`) and `e2e/` — the
 * F2 plan's scope. `scripts/` is deliberately out of scope: its own tests
 * embed specifier strings as fixtures on purpose (see
 * `scripts/__tests__/check-driver-import-boundaries.test.mjs`).
 */
import { readdirSync } from 'fs';
import { extname, relative, resolve } from 'path';
import { scanCode } from './lib/scanSourceCode.mjs';
import {
  SCAN_EXTENSIONS,
  SKIP_DIR_NAMES,
  isSkippedPath,
  readScannedIfPresent,
} from './lib/scanTargets.mjs';

const ROOT = resolve(import.meta.dirname, '..');
const SCAN_ROOTS = ['apps', 'packages', 'e2e'];
const FRONTEND_PACKAGE_ROOT = 'packages/frontend';
const DEEP_PREFIX = '@datazen/frontend/src/';
const TESTING_PREFIX = '@datazen/frontend/testing/';

/** Test placement follows the repo's convention (vitest default globs). */
function isTestFile(repoRelPath) {
  const segments = repoRelPath.split('/');
  if (segments.includes('__tests__')) return true;
  const base = segments[segments.length - 1];
  return /\.(test|spec)\.[cm]?[jt]sx?$/.test(base);
}

/**
 * Every scannable source file under the scan roots, excluding the frontend
 * package itself. The read happens during the walk (same rationale as
 * `check-module-layers.mjs`: a collect-then-read window turns a file that
 * another process deletes into a guard abort, which is indistinguishable
 * from a real violation).
 */
function collectSourceFiles() {
  const out = [];
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      if (entry.isDirectory() && SKIP_DIR_NAMES.has(entry.name)) continue;
      const full = resolve(current, entry.name);
      const repoRel = relative(ROOT, full).split('\\').join('/');
      if (entry.isDirectory()) {
        if (isSkippedPath(repoRel)) continue;
        if (repoRel === FRONTEND_PACKAGE_ROOT) continue;
        walk(full);
      } else if (SCAN_EXTENSIONS.has(extname(entry.name))) {
        const content = readScannedIfPresent(full);
        if (content !== null) out.push({ full, repoRel, content });
      }
    }
  };
  for (const root of SCAN_ROOTS) walk(resolve(ROOT, root));
  return out;
}

const findings = [];
for (const file of collectSourceFiles()) {
  const { literals } = scanCode(file.content);
  const testFile = isTestFile(file.repoRel);
  for (const literal of literals) {
    const specifier = literal.value;
    if (specifier.startsWith(DEEP_PREFIX)) {
      findings.push(
        `${file.repoRel}:${literal.line}: deep import '${specifier}' — use the public entries ('.', './shell', './styles.css')${testFile ? " or the test-only './testing/*' mirror" : ''}`,
      );
    } else if (specifier.startsWith(TESTING_PREFIX) && !testFile) {
      findings.push(
        `${file.repoRel}:${literal.line}: test-only mirror '${specifier}' used outside a test file — production code must go through the public entries`,
      );
    }
  }
}

if (findings.length > 0) {
  console.error(
    `check-frontend-public-entry: ${findings.length} violation(s) against the @datazen/frontend public surface:`,
  );
  for (const f of findings) console.error(`  ${f}`);
  process.exit(1);
}

console.log('check-frontend-public-entry: public entry surface clean.');
