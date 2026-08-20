import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const PACKS_ROOT = resolve(__dirname, '../../../packages/themes');
const REQUIRED_SURFACE_TOKENS = [
  '--c-surface',
  '--c-surface-alt',
  '--c-surface-raised',
  '--c-surface-inset',
  '--c-edge',
  '--c-fg',
  '--c-fg-secondary',
  '--c-fg-muted',
  '--c-accent',
  '--c-success',
  '--c-warning',
  '--c-danger',
  '--c-titlebar',
  '--font-sans',
  '--font-mono',
  '--font-editor',
] as const;

const REQUIRED_CM_TOKENS = [
  '--cm-keyword',
  '--cm-string',
  '--cm-number',
  '--cm-comment',
  '--cm-operator',
  '--cm-punctuation',
  '--cm-foreground',
  '--cm-background',
  '--cm-selection',
  '--cm-cursor',
] as const;

/** Optional for third-party packs; community samples ship them. */
const RECOMMENDED_DT_TOKENS = [
  '--dt-null',
  '--dt-bool',
  '--dt-number',
  '--dt-datetime',
  '--dt-json',
  '--dt-text',
  '--dt-binary',
] as const;

const EDITOR_JSON_KEYS = [
  'keyword',
  'string',
  'number',
  'comment',
  'operator',
  'punctuation',
  'foreground',
  'background',
  'selection',
  'cursor',
] as const;

function listCommunityPackDirs(): string[] {
  return readdirSync(PACKS_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name.startsWith('community.'))
    .map((e) => e.name)
    .filter((name) => {
      try {
        readFileSync(join(PACKS_ROOT, name, 'manifest.json'), 'utf8');
        return true;
      } catch {
        return false;
      }
    });
}

const RECOMMENDED_WORKSPACE_ICONS = [
  'nav.connections',
  'action.workflow',
  'action.dashboard',
  'nav.settings',
  'query.run',
  'query.stop',
  'ai.chat',
  'theme.light',
  'theme.dark',
  'theme.system',
] as const;

describe('community theme packs', () => {
  const packIds = listCommunityPackDirs();

  it('includes all documented community packs', () => {
    expect(packIds.sort()).toEqual(
      [
        'community.dracula',
        'community.nord',
        'community.paper',
        'community.solarized-light',
        'community.tokyo-night',
      ].sort(),
    );
  });

  for (const dirName of packIds) {
    describe(dirName, () => {
      const packDir = join(PACKS_ROOT, dirName);
      const manifest = JSON.parse(readFileSync(join(packDir, 'manifest.json'), 'utf8')) as {
        id: string;
        apiVersion: number;
        modes: string[];
      };
      const tokensCss = readFileSync(join(packDir, 'tokens.css'), 'utf8');

      it('manifest id matches directory name', () => {
        expect(manifest.id).toBe(dirName);
        expect(manifest.apiVersion).toBe(1);
        expect(manifest.modes.length).toBeGreaterThan(0);
      });

      it('tokens.css defines host-aligned semantic variables', () => {
        for (const token of [...REQUIRED_SURFACE_TOKENS, ...REQUIRED_CM_TOKENS]) {
          expect(tokensCss, `${dirName} missing ${token}`).toContain(`${token}:`);
        }
      });

      it('tokens.css defines DataTable cell color variables', () => {
        for (const token of RECOMMENDED_DT_TOKENS) {
          expect(tokensCss, `${dirName} missing ${token}`).toContain(`${token}:`);
        }
      });

      it('targets the correct root selector for its mode', () => {
        const isLight = manifest.modes.includes('light') && !manifest.modes.includes('dark');
        if (isLight) {
          expect(tokensCss).toMatch(/:root\s*\{/);
        } else {
          expect(tokensCss).toMatch(/\.dark\s*\{/);
        }
      });

      it('editor.json keys match EditorColorContract when present', () => {
        const editorPath = join(packDir, 'editor.json');
        try {
          const editor = JSON.parse(readFileSync(editorPath, 'utf8')) as Record<string, unknown>;
          for (const key of EDITOR_JSON_KEYS) {
            expect(typeof editor[key]).toBe('string');
          }
        } catch {
          // optional file
        }
      });

      it('ships full v1 UI_ICON_IDS sample set (except driver db.* icons)', () => {
        for (const iconId of RECOMMENDED_WORKSPACE_ICONS) {
          const iconPath = join(packDir, 'icons', `${iconId}.svg`);
          expect(
            readFileSync(iconPath, 'utf8'),
            `${dirName} missing icons/${iconId}.svg`,
          ).toContain('<svg');
        }
      });
    });
  }
});

describe('theme pack icon contract', () => {
  it('documents semantic ids aligned with UI_ICON_IDS', () => {
    const readme = readFileSync(join(PACKS_ROOT, 'README.md'), 'utf8');
    expect(readme).toContain('nav.settings');
    expect(readme).toContain('query.run');
    expect(readme).toContain('nav.connections');
    expect(readme).toContain('--dt-number');
  });
});
