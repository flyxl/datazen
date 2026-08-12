import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname, '../../..');

describe('e2e contract scripts (F4)', () => {
  const pkg = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as {
    scripts: Record<string, string>;
  };

  it('exposes e2e:contract:matrix and e2e:contract:pg', () => {
    expect(pkg.scripts['e2e:contract:matrix']).toContain('host-contract-matrix.ts');
    expect(pkg.scripts['e2e:contract:pg']).toContain('host-contract-matrix.ts');
    expect(pkg.scripts['e2e:contract:pg']).toMatch(/postgres/);
  });

  it('includes host-contract-matrix in e2e:db', () => {
    expect(pkg.scripts['e2e:db']).toContain('host-contract-matrix.ts');
  });

  it('exposes unit coverage script for contract modules', () => {
    expect(pkg.scripts['test:unit:e2e-contract']).toContain('vitest.e2e-contract.config.ts');
    expect(pkg.scripts['test:unit:e2e-contract:coverage']).toContain('--coverage');
  });
});
