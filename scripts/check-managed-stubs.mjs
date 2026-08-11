#!/usr/bin/env node
/**
 * Fail if git-tracked fully-generated managed files look injected.
 * Run on a clean checkout (before resolve-drivers) so CI catches accidental commits.
 */
import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import {
  cleanGeneratedTsContent,
  cleanGeneratedLocalesContent,
  cleanPluginInitContent,
} from './plugin-deinject.mjs';
import {
  fileHasInjection,
  hasInjectedGeneratedTs,
  hasInjectedGeneratedLocales,
  hasInjectedPluginInit,
} from './plugin-stash-precommit.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const CHECKS = [
  {
    rel: 'src/plugins/generated.ts',
    isInjected: hasInjectedGeneratedTs,
    expected: cleanGeneratedTsContent,
  },
  {
    rel: 'src/plugins/generated-locales.ts',
    isInjected: hasInjectedGeneratedLocales,
    expected: cleanGeneratedLocalesContent,
  },
  {
    rel: 'src-tauri/src/plugin_init.rs',
    isInjected: hasInjectedPluginInit,
    expected: cleanPluginInitContent,
  },
];

let failed = false;
for (const { rel, isInjected, expected } of CHECKS) {
  const path = resolve(ROOT, rel);
  if (!existsSync(path)) {
    console.error(`[check-managed-stubs] missing ${rel}`);
    failed = true;
    continue;
  }
  const content = readFileSync(path, 'utf-8');
  if (isInjected(content) || fileHasInjection(rel, content)) {
    console.error(
      `[check-managed-stubs] ${rel} looks injected (DatabaseType/plugin crates present).`,
    );
    console.error(
      '  Commit the git-safe stub instead: node scripts/resolve-drivers.mjs --drivers=stub',
    );
    console.error('  then restore/deinject before committing.');
    failed = true;
    continue;
  }
  if (content !== expected()) {
    console.error(
      `[check-managed-stubs] ${rel} is not injected but differs from the canonical stub.`,
    );
    console.error(
      '  Refresh with: node -e "import { writeFileSync } from \'fs\'; import { cleanGeneratedTsContent, cleanPluginInitContent } from \'./scripts/plugin-deinject.mjs\'; ..."',
    );
    failed = true;
  } else {
    console.log(`[check-managed-stubs] ok ${rel}`);
  }
}

process.exit(failed ? 1 : 0);
