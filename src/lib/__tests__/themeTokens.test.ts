import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('themes.css tokens', () => {
  const css = readFileSync(resolve(__dirname, '../../styles/themes.css'), 'utf8');
  for (const token of [
    '--c-surface',
    '--c-success',
    '--c-warning',
    '--c-danger',
    '--font-sans',
    '--font-mono',
    '--font-editor',
  ]) {
    it(`defines ${token}`, () => {
      expect(css).toContain(`${token}:`);
    });
  }
});
