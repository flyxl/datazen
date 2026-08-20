/**
 * Regression: production UI must use `components/ui/Select`, not native `<select>`.
 * Test mocks may still use `<select>`; exclude `__tests__` directories.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC_ROOT = join(__dirname, '../..');

function walkProductionSources(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const rel = relative(SRC_ROOT, full);
    if (rel.includes('__tests__')) continue;
    const st = statSync(full);
    if (st.isDirectory()) {
      walkProductionSources(full, out);
    } else if (/\.(tsx|ts)$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

describe('no native <select> in production code', () => {
  it('src/**/*.tsx|ts (excluding __tests__) has no <select', () => {
    const offenders: string[] = [];
    for (const file of walkProductionSources(SRC_ROOT)) {
      const src = readFileSync(file, 'utf-8');
      if (/<select\b/.test(src)) {
        offenders.push(relative(SRC_ROOT, file));
      }
    }
    expect(offenders, 'replace with components/ui/Select').toEqual([]);
  });
});
