import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname, '../../..');

/** Extract the file list of a named suite from e2e/wdio.conf.ts source. */
function suiteEntries(conf: string, name: string): string {
  const re = new RegExp(`'?${name}'?:\\s*\\[([\\s\\S]*?)\\]`);
  const m = conf.match(re);
  if (!m) throw new Error(`suite "${name}" not found in e2e/wdio.conf.ts`);
  return m[1];
}

describe('e2e contract scripts (F4)', () => {
  const pkg = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as {
    scripts: Record<string, string>;
  };
  const conf = readFileSync(resolve(root, 'e2e/wdio.conf.ts'), 'utf8');

  it('exposes e2e:contract:matrix and e2e:contract:pg on the contract suite', () => {
    expect(pkg.scripts['e2e:contract:matrix']).toContain('--suite contract');
    expect(pkg.scripts['e2e:contract:pg']).toContain('--suite contract');
    expect(pkg.scripts['e2e:contract:pg']).toMatch(/postgres/);
  });

  it('includes host-contract-matrix in the contract and db suites', () => {
    expect(suiteEntries(conf, 'contract')).toContain('host-contract-matrix');
    expect(suiteEntries(conf, 'db')).toContain('host-contract-matrix');
  });

  it('exposes unit coverage script for contract modules', () => {
    expect(pkg.scripts['test:unit:e2e-contract']).toContain('vitest.e2e-contract.config.ts');
    expect(pkg.scripts['test:unit:e2e-contract:coverage']).toContain('--coverage');
  });
});
