import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { buildThemeSnapshot, THEME_TOKENS, THEME_SNAPSHOT_VERSION } from '../themeTokens';

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

describe('THEME_TOKENS contract', () => {
  const css = readFileSync(resolve(__dirname, '../../styles/themes.css'), 'utf8');

  it('covers every --c-*/--dt-* token defined in themes.css', () => {
    const defined = new Set<string>();
    for (const match of css.matchAll(/(--(?:c|dt)-[a-z][a-z0-9-]*)\s*:/g)) {
      defined.add(match[1]);
    }
    for (const name of defined) {
      expect(THEME_TOKENS).toContain(name);
    }
  });

  it('only contains --c-* and --dt-* names present in themes.css', () => {
    for (const name of THEME_TOKENS) {
      expect(css).toContain(`${name}:`);
      expect(name).toMatch(/^--(c|dt)-/);
    }
  });
});

describe('buildThemeSnapshot', () => {
  afterEach(() => {
    document.documentElement.classList.remove('dark');
  });

  it('reports the dark flag from the root class list and protocol version', () => {
    document.documentElement.classList.remove('dark');
    expect(buildThemeSnapshot().dark).toBe(false);

    document.documentElement.classList.add('dark');
    const snapshot = buildThemeSnapshot();
    expect(snapshot.dark).toBe(true);
    expect(snapshot.v).toBe(THEME_SNAPSHOT_VERSION);
    expect(snapshot.v).toBe(2);
  });

  it('returns exactly the THEME_TOKENS contract as the token map', () => {
    document.documentElement.classList.add('dark');
    const snapshot = buildThemeSnapshot();
    expect(Object.keys(snapshot.tokens).sort()).toEqual([...THEME_TOKENS].sort());
    // Values are live reads; jsdom yields '' but the keys must all exist.
    for (const value of Object.values(snapshot.tokens)) {
      expect(typeof value).toBe('string');
    }
  });
});
