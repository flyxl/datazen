/**
 * Inventory guard for the driver-side reach of the `Dialog` close-label fix
 * (track `wave2/dialog-i18n`).
 *
 * `packages/ui/src/Dialog.tsx` used to hardcode `closeLabel = 'Close'`. This
 * suite counts, from the sources on disk, exactly which driver `<Dialog>` sites
 * therefore shipped an English accessible name no matter what language the app
 * was running in, and pins that inventory so the number in the track report
 * cannot silently drift.
 *
 * Why a source scan rather than eight hand-written render cases: the eight sites
 * need driver props, store bridges and mocked driver-SDK state to mount, and
 * each such harness pins a snapshot of that component's props — the classic way
 * this kind of reach claim rots. Scanning the call sites is stable, and
 * `dialogCloseLabelI18n.test.tsx` separately proves the shared `Dialog` code path
 * they all go through.
 *
 * Scope is deliberately the *driver* surface only. The host already routes every
 * `<Dialog>` through `src/components/ui/Dialog.tsx`, which injects
 * `closeLabel={props.closeLabel ?? t('common.close')}` and was never affected.
 */
import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

/**
 * Repo root, found by walking up from this file rather than by counting `..`
 * segments: `__tests__/ → ui/ → redis/ → drivers/ → packages/ → root`, and a
 * wrong count silently scans a neighbouring directory instead of failing.
 */
function findRepoRoot(from: string): string {
  let dir = from;
  for (let i = 0; i < 10; i += 1) {
    if (existsSync(join(dir, 'packages', 'drivers', 'redis'))) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(`could not locate the repo root above ${from}`);
}

const REPO_ROOT = findRepoRoot(import.meta.dirname);
const DRIVER_UI_DIR = resolve(REPO_ROOT, 'packages/drivers');

/**
 * Every `.tsx` under a driver's `ui/`, recursively. The redis dialogs live in
 * `ui/key-browser/`, `ui/observe/` and `ui/value-editors/`, so a single-level
 * read would find none of them and the inventory below would be vacuously
 * empty.
 */
function tsxFilesIn(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      // `__tests__` holds the suites, not the call sites under audit.
      if (entry.name === '__tests__') continue;
      out.push(...tsxFilesIn(full));
    } else if (entry.isFile() && entry.name.endsWith('.tsx') && !entry.name.includes('.test.')) {
      out.push(full);
    }
  }
  return out;
}

/** Driver UI sources, excluding tests and the gitignored driver checkouts. */
function driverUiSources(dir = DRIVER_UI_DIR): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    // `kiwi` / `superset` are git drivers, absent or excluded from
    // `vitest.drivers.config.ts`; skip any driver without a `ui/` directory.
    const uiDir = join(dir, entry.name, 'ui');
    try {
      if (!statSync(uiDir).isDirectory()) continue;
    } catch {
      continue;
    }
    out.push(...tsxFilesIn(uiDir));
  }
  return out;
}

/** Files that pull `Dialog` from the design system. */
function dialogConsumers(): string[] {
  return driverUiSources().filter((file) => {
    const src = readFileSync(file, 'utf8');
    // Matches `import { Dialog } from '@datazen/ui'`, `import { Button, Dialog,
    // useI18n } from '@datazen/ui'`, … but not `DraftLeaveDialog`, whose name
    // merely ends with the component name.
    return /import\s*\{[^}]*\bDialog\b[^}]*\}\s*from\s*'@datazen\/ui'/.test(src);
  });
}

/** Opening `<Dialog` JSX tags in a file. */
function dialogSites(src: string): number {
  return (src.match(/<Dialog(?=[\s/>])/g) ?? []).length;
}

const consumers = dialogConsumers();

/** repo-relative path, so a failure names a file a reader can open. */
const rel = (file: string): string => relative(REPO_ROOT, file);

describe('[tester] inventory of driver dialogs affected by the close-label fix', () => {
  it('finds exactly the four known driver consumers of @datazen/ui Dialog', () => {
    expect(consumers.map(rel).sort()).toEqual([
      'packages/drivers/redis/ui/key-browser/ImportExport.tsx',
      'packages/drivers/redis/ui/key-browser/KeyWorkbenchDialogs.tsx',
      'packages/drivers/redis/ui/observe/SlowlogPanel.tsx',
      'packages/drivers/redis/ui/value-editors/DraftLeaveDialog.tsx',
    ]);
  });

  it('exposes exactly 8 driver <Dialog> sites in total', () => {
    // The headline number: 1 + 5 + 1 + 1. Recomputed from source on every run,
    // so a driver adding a ninth dialog makes this go red instead of quietly
    // inheriting an unverified claim.
    const perFile: Array<[string, number]> = consumers.map((file) => [
      rel(file),
      dialogSites(readFileSync(file, 'utf8')),
    ]);
    const total = perFile.reduce((sum, [, count]) => sum + count, 0);
    expect(perFile).toEqual([
      ['packages/drivers/redis/ui/key-browser/ImportExport.tsx', 1],
      ['packages/drivers/redis/ui/key-browser/KeyWorkbenchDialogs.tsx', 5],
      ['packages/drivers/redis/ui/observe/SlowlogPanel.tsx', 1],
      ['packages/drivers/redis/ui/value-editors/DraftLeaveDialog.tsx', 1],
    ]);
    expect(total).toBe(8);
  });

  it('none of the 8 passes closeLabel, so all 8 relied on the hardcoded literal', () => {
    // If a site ever starts passing its own `closeLabel`, it stops depending on
    // `Dialog`'s default and the "8 affected" claim is no longer true. The
    // per-file counts are re-checked here so this cannot pass vacuously on an
    // empty consumer list.
    expect(consumers.length).toBe(4);
    for (const file of consumers) {
      expect(readFileSync(file, 'utf8'), `${rel(file)} passes closeLabel`).not.toContain(
        'closeLabel',
      );
    }
  });
});
