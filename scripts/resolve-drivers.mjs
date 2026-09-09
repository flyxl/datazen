#!/usr/bin/env node
/**
 * resolve-drivers.mjs — thin loader that reconstructs the full script from
 * committed base64 parts (scripts/resolve-drivers.b64.*.txt).
 */
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { fileURLToPath, pathToFileURL } from 'url';
import { dirname, join } from 'path';

const dir = dirname(fileURLToPath(import.meta.url));
const impl = join(dir, 'resolve-drivers.impl.mjs');

function materialize() {
  const parts = readdirSync(dir)
    .filter((f) => /^resolve-drivers\.b64\.\d+\.txt$/.test(f))
    .sort((a, b) => {
      const na = Number(a.match(/(\d+)/)[1]);
      const nb = Number(b.match(/(\d+)/)[1]);
      return na - nb;
    });
  if (parts.length === 0) {
    console.error('[resolve-drivers] missing resolve-drivers.b64.*.txt parts');
    process.exit(1);
  }
  let b64 = '';
  for (const f of parts) {
    b64 += readFileSync(join(dir, f), 'utf8').trim();
  }
  writeFileSync(impl, Buffer.from(b64, 'base64'));
}

if (!existsSync(impl) || process.argv.includes('--rematerialize')) {
  materialize();
  console.log('[resolve-drivers] wrote', impl);
}
await import(pathToFileURL(impl).href);
