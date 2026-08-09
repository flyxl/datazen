#!/usr/bin/env node
/**
 * Guardrail: Host must not maintain a central structure caps registry.
 * See docs/superpowers/specs/2026-08-09-table-structure-editor-plugins-design.md
 */
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PATTERN = 'capabilityByType|structure_capabilities_by';

let output = '';
try {
  output = execSync(`rg -n "${PATTERN}" src src-tauri`, {
    cwd: ROOT,
    encoding: 'utf-8',
  });
} catch {
  console.log('[check-structure-editor-guardrails] ok — no Host caps registry patterns found');
  process.exit(0);
}

const lines = output.trim().split('\n').filter(Boolean);
if (lines.length === 0) {
  console.log('[check-structure-editor-guardrails] ok — no Host caps registry patterns found');
  process.exit(0);
}

console.error('[check-structure-editor-guardrails] forbidden Host caps registry patterns found:');
console.error(output);
process.exit(1);
