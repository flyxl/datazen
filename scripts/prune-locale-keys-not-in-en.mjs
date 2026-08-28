#!/usr/bin/env node
/**
 * Remove locale entries whose keys are absent from en.ts (orphan keys after en prune).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const localesDir = resolve(root, 'src/locales');

const TARGETS = ['de.ts', 'es.ts', 'fr.ts', 'ja.ts', 'ko.ts', 'pt-BR.ts', 'ru.ts', 'zh-TW.ts'];

function extractKeySet(filePath) {
  const src = readFileSync(filePath, 'utf-8');
  const keys = new Set();
  const re = /^\s*'([^']+)':/gm;
  let m;
  while ((m = re.exec(src)) !== null) keys.add(m[1]);
  return keys;
}

/** Remove one key entry (single- or multi-line string value). */
function removeKeyEntry(src, key) {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const patterns = [
    // multiline: 'key':\n    '...',
    new RegExp(`\\n  '${escaped}':\\n    '[\\s\\S]*?',`, 'g'),
    // single line: 'key': '...',
    new RegExp(`\\n  '${escaped}': '(?:[^'\\\\]|\\\\.)*',`, 'g'),
    // single line double quotes
    new RegExp(`\\n  '${escaped}': "(?:[^"\\\\]|\\\\.)*",`, 'g'),
  ];
  let next = src;
  for (const re of patterns) {
    next = next.replace(re, '');
  }
  return next;
}

const enKeys = extractKeySet(resolve(localesDir, 'en.ts'));

for (const file of TARGETS) {
  const path = resolve(localesDir, file);
  let src = readFileSync(path, 'utf-8');
  const localeKeys = extractKeySet(path);
  const orphans = [...localeKeys].filter((k) => !enKeys.has(k)).sort();
  if (orphans.length === 0) {
    console.log(`${file}: no orphans`);
    continue;
  }
  for (const key of orphans) {
    src = removeKeyEntry(src, key);
  }
  writeFileSync(path, src, 'utf-8');
  console.log(`${file}: removed ${orphans.length} orphan key(s)`);
}
