#!/usr/bin/env node
/**
 * Guardrail: Host must not maintain a central structure caps registry.
 * See docs/superpowers/specs/2026-08-09-table-structure-editor-plugins-design.md
 *
 * Pattern notes:
 * - Registry identifiers (capabilityByType, structure_capabilities_by, …).
 * - Per-dialect inline caps maps: StructureCapabilities { … postgres … } catches
 *   DBX-style `{ postgres: { renameColumn: … } }` literals. Plain `interface
 *   StructureCapabilities { createTable: … }` in types.ts does not mention a
 *   dialect key, so it is not flagged.
 */
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCOPES = ['src', 'src-tauri'];

const CHECKS = [
  {
    name: 'registry identifiers',
    pattern: 'capabilityByType|structure_capabilities_by|structureCapabilitiesBy',
  },
  {
    name: 'dialect-keyed StructureCapabilities literal',
    pattern: String.raw`StructureCapabilities\s*\{[^}]*postgres`,
  },
];

function rg(pattern) {
  try {
    return execSync(`rg -n --pcre2 "${pattern}" ${SCOPES.join(' ')}`, {
      cwd: ROOT,
      encoding: 'utf-8',
    }).trim();
  } catch {
    return '';
  }
}

const hits = [];
for (const { name, pattern } of CHECKS) {
  const output = rg(pattern);
  if (output) {
    hits.push({ name, pattern, output });
  }
}

if (hits.length === 0) {
  console.log('[check-structure-editor-guardrails] ok — no Host caps registry patterns found');
  process.exit(0);
}

console.error('[check-structure-editor-guardrails] forbidden Host caps registry patterns found:');
for (const { name, pattern, output } of hits) {
  console.error(`\n== ${name} (${pattern}) ==\n${output}`);
}
process.exit(1);
