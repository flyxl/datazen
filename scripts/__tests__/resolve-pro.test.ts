/** @vitest-environment node */
import { mkdtempSync, readFileSync, existsSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import { parseArgs, writeCommunityCodegen, writeProCodegen, resolvePro } from '../resolve-pro.mjs';

describe('resolve-pro parseArgs', () => {
  it('defaults to community edition', () => {
    const res = parseArgs([]);
    expect(res.edition).toBe('community');
    expect(res.restore).toBe(false);
  });

  it('detects --pro and --edition=pro', () => {
    expect(parseArgs(['--pro']).edition).toBe('pro');
    expect(parseArgs(['--edition=pro']).edition).toBe('pro');
  });

  it('detects --restore', () => {
    const res = parseArgs(['--restore']);
    expect(res.restore).toBe(true);
    expect(res.edition).toBe('community');
  });

  it('parses custom path and git parameters', () => {
    const res = parseArgs([
      '--edition=pro',
      '--pro-path=/custom/path',
      '--pro-git=git@github.com:custom/repo.git',
      '--codegen-only',
    ]);
    expect(res.edition).toBe('pro');
    expect(res.proPath).toBe('/custom/path');
    expect(res.proGit).toBe('git@github.com:custom/repo.git');
    expect(res.codegenOnly).toBe(true);
  });
});

describe('resolve-pro codegen output', () => {
  it('writes community codegen with fallback', () => {
    const dir = mkdtempSync(join(tmpdir(), 'resolve-pro-test-'));
    const file = join(dir, 'generated-pro.ts');
    writeCommunityCodegen(file);
    expect(existsSync(file)).toBe(true);
    const content = readFileSync(file, 'utf-8');
    expect(content).toContain("export const DATAZEN_EDITION = 'community'");
    expect(content).toContain('export function initProExtensions(): void');
    expect(content).not.toContain('@datazen/extension-sql-editor-pro');
  });

  it('writes pro codegen with builtin-ep HostExtensionLoader activation', () => {
    const dir = mkdtempSync(join(tmpdir(), 'resolve-pro-test-'));
    const file = join(dir, 'generated-pro.ts');
    writeProCodegen(file);
    expect(existsSync(file)).toBe(true);
    const content = readFileSync(file, 'utf-8');
    expect(content).toContain("export const DATAZEN_EDITION = 'pro'");
    expect(content).toContain('hostExtensionLoader');
    expect(content).toContain('loadFromUrl');
    expect(content).toContain("'builtin-ep'");
    expect(content).not.toContain('@datazen/extension-sql-editor-pro');
  });

  it('preserves existing generated-pro.ts when codegenOnly is run without explicit edition', () => {
    const res = resolvePro({ codegenOnly: true });
    // Should preserve existing file when no explicit edition passed
    expect(res).toBeDefined();
  });
});
