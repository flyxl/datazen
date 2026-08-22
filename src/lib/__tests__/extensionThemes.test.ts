import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Guards the repo-bundled extension packages under `packages/extensions/`.
 *
 * Since the unified plugin system (PRD: ui-plugins.md), themes ship as
 * `contributes.themes` inside extension manifests (apiVersion 2) instead of
 * legacy v1 ThemePacks. The community theme pack was converted accordingly;
 * this suite keeps its token contract honest and validates the sample
 * extension structure that users install for testing.
 */

const EXTENSIONS_ROOT = resolve(__dirname, '../../../packages/extensions');

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
  '--c-titlebar-fg',
  '--c-titlebar-fg-muted',
  '--c-query-run',
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

/** Fonts stay part of the host token contract for full-look packs. */
const REQUIRED_FONT_TOKENS = ['--font-sans', '--font-mono', '--font-editor'] as const;

interface ExtensionManifest {
  id: string;
  apiVersion: number;
  entry?: string;
  contributes?: {
    pages?: Array<{ id: string; title: string; icon?: string; showIn?: string }>;
    themes?: Array<{
      id: string;
      name: string;
      tokensCss: string;
      modes: string[];
      editorJson?: string;
      chartsJson?: string;
      iconsDir?: string;
    }>;
  };
  permissions?: string[];
}

function listExtensionDirs(): string[] {
  return readdirSync(EXTENSIONS_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name.includes('.'))
    .map((e) => e.name)
    .filter((name) => existsSync(join(EXTENSIONS_ROOT, name, 'manifest.json')));
}

function loadManifest(dirName: string): ExtensionManifest {
  return JSON.parse(
    readFileSync(join(EXTENSIONS_ROOT, dirName, 'manifest.json'), 'utf8'),
  ) as ExtensionManifest;
}

describe('repo extension packages', () => {
  const dirNames = listExtensionDirs();

  it('includes the converted community theme and the sample extension', () => {
    expect(dirNames.sort()).toEqual(['community.slate-blue', 'datazen.playground'].sort());
  });

  it.each(dirNames)('%s keeps manifest id == directory name and apiVersion 2', (dirName) => {
    const manifest = loadManifest(dirName);
    expect(manifest.id).toBe(dirName);
    expect(manifest.apiVersion).toBe(2);
    expect(manifest.contributes).toBeDefined();
  });

  it.each(dirNames)(
    '%s declares only existing contribution paths with safe extensions',
    (dirName) => {
      const manifest = loadManifest(dirName);
      const allowed = /\.(html|js|mjs|css|json|svg|png|webp|woff2|woff)$/i;
      const declared: string[] = [];
      if (manifest.entry) declared.push(manifest.entry);
      for (const page of manifest.contributes?.pages ?? []) {
        if (page.icon) declared.push(page.icon);
      }
      for (const theme of manifest.contributes?.themes ?? []) {
        declared.push(theme.tokensCss, theme.editorJson, theme.chartsJson);
        // iconsDir is a directory — check existence separately below.
        if (theme.iconsDir) {
          const iconsPath = join(EXTENSIONS_ROOT, dirName, theme.iconsDir);
          expect(
            existsSync(iconsPath) && readdirSync(iconsPath).length > 0,
            `${dirName}: ${theme.iconsDir} must be a non-empty directory`,
          ).toBe(true);
        }
      }
      for (const rel of declared.filter(Boolean) as string[]) {
        expect(rel, `${dirName}: ${rel} must be relative`).not.toMatch(/^([A-Za-z]:)?\//);
        expect(rel, `${dirName}: ${rel} must not traverse`).not.toContain('..');
        expect(rel, `${dirName}: ${rel} must be whitelisted`).toMatch(allowed);
        expect(existsSync(join(EXTENSIONS_ROOT, dirName, rel)), `${dirName}: ${rel} missing`).toBe(
          true,
        );
      }
    },
  );
});

describe('community.slate-blue theme contract (converted extension)', () => {
  const packDir = join(EXTENSIONS_ROOT, 'community.slate-blue');
  const manifest = loadManifest('community.slate-blue');
  const theme = manifest.contributes?.themes?.[0];
  const tokensCss = readFileSync(join(packDir, theme?.tokensCss ?? ''), 'utf8');

  it('declares a light+dark pure-theme contribution without pages/permissions', () => {
    expect(theme?.id).toBe('slate-blue');
    expect(theme?.modes).toEqual(['light', 'dark']);
    expect(manifest.entry).toBeUndefined();
    expect(manifest.permissions ?? []).toEqual([]);
    expect(manifest.contributes?.pages ?? []).toEqual([]);
  });

  it('tokens.css defines host-aligned semantic variables', () => {
    for (const token of [...REQUIRED_SURFACE_TOKENS, ...REQUIRED_FONT_TOKENS]) {
      expect(tokensCss, `missing ${token}`).toContain(`${token}:`);
    }
  });

  it('tokens.css defines CodeMirror syntax colors', () => {
    for (const token of REQUIRED_CM_TOKENS) {
      expect(tokensCss, `missing ${token}`).toContain(`${token}:`);
    }
  });

  it('tokens.css defines DataTable cell color variables', () => {
    for (const token of RECOMMENDED_DT_TOKENS) {
      expect(tokensCss, `missing ${token}`).toContain(`${token}:`);
    }
  });

  it('targets :root for light and .dark for dark', () => {
    expect(tokensCss).toMatch(/:root\s*\{/);
    expect(tokensCss).toMatch(/\.dark\s*\{/);
  });

  it('preserves legacy ThemePack capabilities (charts + icons)', () => {
    expect(theme?.chartsJson).toBe('themes/slate-blue/charts.json');
    expect(theme?.iconsDir).toBe('themes/slate-blue/icons');
    const charts = JSON.parse(
      readFileSync(join(packDir, theme?.chartsJson ?? ''), 'utf8'),
    ) as Record<string, unknown>;
    expect(Object.keys(charts).length).toBeGreaterThan(0);
    expect(readdirSync(join(packDir, theme?.iconsDir ?? '')).some((f) => f.endsWith('.svg'))).toBe(
      true,
    );
  });
});

describe('datazen.playground sample extension', () => {
  const manifest = loadManifest('datazen.playground');

  it('contributes a workspace page plus a dark theme', () => {
    expect(manifest.entry).toBe('index.html');
    expect(manifest.contributes?.pages?.[0]?.id).toBe('playground');
    expect(manifest.contributes?.pages?.[0]?.showIn ?? 'workspace').toBe('workspace');
    expect(manifest.contributes?.themes?.[0]?.modes).toEqual(['dark']);
  });

  it('requests the v1 permissions its page exercises', () => {
    expect(manifest.permissions).toEqual([
      'context:connections',
      'command:invoke',
      'storage:local',
      'ui:notify',
    ]);
  });
});

describe('extensions README contract', () => {
  it('documents layout, install flow, and the legacy-parity theme fields', () => {
    const readme = readFileSync(join(EXTENSIONS_ROOT, 'README.md'), 'utf8');
    expect(readme).toContain('manifest.json');
    expect(readme).toContain('contributes');
    expect(readme).toContain('插件管理页'); // install entry point
    // Legacy ThemePack capabilities are first-class optional fields now.
    expect(readme).toContain('editorJson');
    expect(readme).toContain('chartsJson');
    expect(readme).toContain('iconsDir');
  });
});
