/** @vitest-environment node */
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  EXTENSION_POINTS_VERSION,
  verifyExtensionPackage,
} from '../../packages/extension-points/src/security.ts';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

describe('[tester] sign-ep.mjs cross-runtime parity', () => {
  it('produces signature.sig verifiable by host security gate', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'sign-ep-test-'));
    try {
      const manifest = {
        id: '@datazen/extension-test',
        version: '0.0.1',
        main: 'dist/index.esm.js',
        engines: { extensionPointsVersion: EXTENSION_POINTS_VERSION },
      };
      const manifestContent = JSON.stringify(manifest, null, 2);
      const bundleContent = 'export function activate() {}\n';

      mkdirSync(join(dir, 'dist'), { recursive: true });
      writeFileSync(join(dir, 'manifest.json'), manifestContent);
      writeFileSync(join(dir, 'dist/index.esm.js'), bundleContent);

      execSync(`node scripts/sign-ep.mjs --dir "${dir}"`, { cwd: repoRoot, stdio: 'pipe' });

      const signatureContent = readFileSync(join(dir, 'signature.sig'), 'utf8');
      const result = await verifyExtensionPackage({
        manifest,
        files: { manifestContent, bundleContent, signatureContent },
      });

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.trustSource).toBe('official');
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('rejects signing when required bundle file is missing', () => {
    const dir = mkdtempSync(join(tmpdir(), 'sign-ep-missing-'));
    try {
      writeFileSync(join(dir, 'manifest.json'), '{}');
      expect(() =>
        execSync(`node scripts/sign-ep.mjs --dir "${dir}"`, { cwd: repoRoot, stdio: 'pipe' }),
      ).toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
