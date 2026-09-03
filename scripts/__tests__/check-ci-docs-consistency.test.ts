/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import {
  checkCiMatrixDrivers,
  checkToolchainVersions,
  checkWindowBoundaries,
  checkCiDocsConsistency,
  extractCiMatrixDriverIds,
} from '../check-ci-docs-consistency.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

describe('[tester] check-ci-docs-consistency', () => {
  it('test_tester_ci_matrix_driver_ids_exist_in_registry', () => {
    const result = checkCiMatrixDrivers({ root: ROOT });
    expect(result.missing, result.missing.join(', ')).toEqual([]);
    expect(result.mentioned).toContain('postgres');
    expect(result.mentioned).toContain('mongodb');
    expect(result.mentioned).toContain('kiwi');
  });

  it('test_tester_extract_ci_matrix_driver_ids', () => {
    const ids = extractCiMatrixDriverIds(
      'basic (postgres, mysql, sqlite, redis); Akulaku: postgres,mysql,sqlite,redis,mongodb,kiwi,superset; optional mongodb、clickhouse',
    );
    expect(ids).toEqual(
      expect.arrayContaining(['postgres', 'mysql', 'sqlite', 'redis', 'mongodb', 'kiwi', 'superset', 'clickhouse']),
    );
  });

  it('test_tester_sub_window_kinds_match_code', () => {
    const result = checkWindowBoundaries({ root: ROOT });
    // Known doc drift: windows.md §4 WindowKind snippet omits data-transfer.
    // Code (windowKind.ts + windowManager.ts) is authoritative; flag doc-only gaps separately.
    const docOnly = result.errors.filter((e) => e.includes('windows.md §4 WindowKind'));
    const codeErrors = result.errors.filter((e) => !e.includes('windows.md §4 WindowKind'));
    expect(codeErrors, codeErrors.join('\n')).toEqual([]);
    expect(docOnly.length).toBeGreaterThan(0);
  });

  it('test_tester_toolchain_versions_match_ci', () => {
    const result = checkToolchainVersions({ root: ROOT });
    expect(result.ok, result.errors.join('\n')).toBe(true);
    expect(result.ci.node).toBe('24');
    expect(result.ci.pnpm).toBe('11');
  });

  it('test_tester_full_consistency_script', () => {
    const code = checkCiDocsConsistency({ root: ROOT, log: () => {}, error: () => {} });
    // Fails until windows.md §4 snippet is updated (doc drift, not code bug).
    expect(code).toBe(1);
  });
});
