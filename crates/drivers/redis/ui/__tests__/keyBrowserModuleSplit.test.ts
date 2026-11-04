/**
 * D-0 structural guard (键树 track).
 *
 * `RedisWorkbench.tsx` had grown to 705 lines of mixed concerns, which is exactly
 * the shape that makes the §3.2 rework land in one giant diff. These assertions
 * pin the *structure* produced by the split — file sizes and the existence of the
 * carved modules — so "the wall moved somewhere else" fails loudly instead of
 * passing review. No rendered copy is asserted (PRD §7-6 assertion policy).
 */
import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const KEY_BROWSER_DIR = path.resolve(import.meta.dirname, '../key-browser');

/** AGENTS.md code style: recommended ceiling for one source file. */
const SOFT_FILE_LIMIT = 800;
/** AGENTS.md code style: hard ceiling, "严禁出现超大单文件". */
const HARD_FILE_LIMIT = 800;
/** Line count `RedisWorkbench.tsx` had before the D-0 split. */
const PRE_SPLIT_WORKBENCH_LINES = 705;

function sourceFiles(dir: string): string[] {
  return fs
    .readdirSync(dir)
    .filter((name) => /\.(ts|tsx)$/.test(name))
    .map((name) => path.join(dir, name));
}

function lineCount(file: string): number {
  return fs.readFileSync(file, 'utf8').split('\n').length;
}

describe('key-browser module split (D-0)', () => {
  it('keeps every key-browser source under the per-file budget', () => {
    const offenders = sourceFiles(KEY_BROWSER_DIR)
      .map((file) => [path.basename(file), lineCount(file)] as const)
      .filter(([, lines]) => lines > HARD_FILE_LIMIT);
    expect(offenders).toEqual([]);
  });

  it('shrinks the workbench composition below its pre-split size and budget', () => {
    const lines = lineCount(path.join(KEY_BROWSER_DIR, 'RedisWorkbench.tsx'));
    expect(lines).toBeLessThanOrEqual(SOFT_FILE_LIMIT);
    expect(lines).toBeLessThan(PRE_SPLIT_WORKBENCH_LINES * 0.6);
  });

  it('gave each carved block of the wall its own module', () => {
    for (const module of [
      'workbenchDatabases.ts',
      'workbenchTypes.ts',
      'DbSidebar.tsx',
      'WorkbenchToolbar.tsx',
      'useWorkbenchSplit.ts',
      'useKeySelection.ts',
      'useKeyDetailState.ts',
      'useDbKeyCounts.ts',
      'useReJsonModules.ts',
      'useWorkbenchOverlays.ts',
    ]) {
      expect(fs.existsSync(path.join(KEY_BROWSER_DIR, module)), `${module} missing`).toBe(true);
    }
  });
});
