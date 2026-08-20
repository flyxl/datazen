import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { WINDOW_CAPABILITY_LABEL_SAMPLES } from './windowManager';

function globToRegExp(pattern: string): RegExp {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
  return new RegExp(`^${escaped}$`);
}

function loadCapabilityWindowPatterns(): string[] {
  const path = resolve(__dirname, '../../src-tauri/capabilities/default.json');
  const json = JSON.parse(readFileSync(path, 'utf-8')) as { windows: string[] };
  return json.windows;
}

function loadHostCapabilityWindowPatterns(): string[] {
  const path = resolve(__dirname, '../../src-tauri/capabilities/default.json.host');
  const json = JSON.parse(readFileSync(path, 'utf-8')) as { windows: string[] };
  return json.windows;
}

/** Window labels that must not appear in capabilities after P3/P6 cleanup. */
const OBSOLETE_CAPABILITY_PATTERNS = [
  'settings-singleton',
  'docs-singleton',
  'new-connection-*',
] as const;

describe('window capabilities coverage', () => {
  it('covers every windowManager label sample', () => {
    const patterns = loadCapabilityWindowPatterns();
    const missing = WINDOW_CAPABILITY_LABEL_SAMPLES.filter(
      (label) => !patterns.some((p) => globToRegExp(p).test(label)),
    );
    expect(missing, `add labels/globs to capabilities/default.json: ${missing.join(', ')}`).toEqual(
      [],
    );
  });

  it('does not declare obsolete singleton sub-window patterns', () => {
    const hostPatterns = loadHostCapabilityWindowPatterns();
    for (const obsolete of OBSOLETE_CAPABILITY_PATTERNS) {
      expect(
        hostPatterns,
        `remove obsolete pattern from default.json.host: ${obsolete}`,
      ).not.toContain(obsolete);
    }
  });
});
