#!/usr/bin/env node
/**
 * Pack the checked-in community theme fixture into a zip for install tests.
 *
 * Source: fixtures/themes/community.fixture-dark/
 * Output: fixtures/themes/community.fixture-dark.zip
 *
 * Usage:
 *   node scripts/pack-theme-fixture.mjs
 */

import { execFileSync } from 'child_process';
import { existsSync, rmSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const SOURCE = resolve(ROOT, 'fixtures/themes/community.fixture-dark');
const OUT = resolve(ROOT, 'fixtures/themes/community.fixture-dark.zip');

if (!existsSync(SOURCE)) {
  console.error(`Missing fixture directory: ${SOURCE}`);
  process.exit(1);
}

if (existsSync(OUT)) {
  rmSync(OUT);
}

try {
  execFileSync('zip', ['-r', '-q', OUT, '.'], { cwd: SOURCE, stdio: 'inherit' });
} catch {
  console.error('Failed to create zip. Ensure the `zip` CLI is installed.');
  process.exit(1);
}

console.log(`Wrote ${OUT}`);
