#!/usr/bin/env node
/**
 * Run Kiwi plugin E2E from Host. Specs live in datazen-driver-kiwi (not e2e/specs).
 *
 *   DATAZEN_DRIVERS=basic,kiwi pnpm e2e:kiwi
 *   DATAZEN_DRIVERS=basic,kiwi pnpm e2e:kiwi -- --skip-build
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');

function findKiwiSpec() {
  const extra = process.env.DATAZEN_KIWI_DIR
    ? [path.join(process.env.DATAZEN_KIWI_DIR, 'e2e/kiwi.ts')]
    : [];
  const candidates = [
    path.join(ROOT, '.plugins/kiwi/e2e/kiwi.ts'),
    path.join(ROOT, '../datazen-driver-kiwi/e2e/kiwi.ts'),
    ...extra,
  ];
  return candidates.find((p) => fs.existsSync(p));
}

const spec = findKiwiSpec();
if (!spec) {
  console.error(`Kiwi E2E lives in the datazen-driver-kiwi repo (e2e/kiwi.ts).
Host no longer ships e2e/specs/kiwi.ts.

Clone/inject kiwi, then re-run from the DataZen checkout:

  DATAZEN_DRIVERS=basic,kiwi pnpm e2e:kiwi

Or set DATAZEN_KIWI_DIR to a checkout that contains e2e/kiwi.ts.
Looked in:
  .plugins/kiwi/e2e/kiwi.ts
  ../datazen-driver-kiwi/e2e/kiwi.ts`);
  process.exit(1);
}

const env = { ...process.env, DATAZEN_ROOT: ROOT };
const drivers = env.DATAZEN_DRIVERS || '';
if (!drivers || drivers === 'basic') {
  env.DATAZEN_DRIVERS = 'basic,kiwi';
} else if (drivers !== 'all' && !drivers.split(',').map((s) => s.trim()).includes('kiwi')) {
  env.DATAZEN_DRIVERS = `${drivers},kiwi`;
}

console.log(`[e2e-kiwi] spec=${path.relative(ROOT, spec)} DATAZEN_DRIVERS=${env.DATAZEN_DRIVERS}`);

const extraArgs = process.argv.slice(2).filter((a) => a !== '--');
const child = spawn(
  process.execPath,
  [path.join(ROOT, 'e2e/run.mjs'), ...extraArgs, '--spec', spec],
  { stdio: 'inherit', cwd: ROOT, env },
);
child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
