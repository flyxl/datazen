import { spawnSync } from 'node:child_process';
import { copyFileSync, cpSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { describe, expect, it } from 'vitest';
import { isDirectInvocation } from '../lib/isDirectInvocation.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const cases = [
  ['check-platform-crate-boundaries', ['--help'], 'usage:'],
  ['check-driver-protocol-compat', ['--unknown'], 'usage:'],
  ['check-driver-protocol-release', ['--unknown'], 'usage:'],
  ['run-platform-crate-tests', ['--unknown'], 'unknown argument'],
  ['check-managed-stubs', [], '[check-managed-stubs]'],
  ['check-module-layers', [], '[check-module-layers]'],
  ['check-release-variants', [], '[check-release-variants]'],
  ['platform-arch-selfcheck', [], 'REFUSING TO RUN'],
] as const;

describe('gate CLI entry paths', () => {
  for (const [script, args, marker] of cases) {
    it(`${script} executes via a symlink containing spaces and Unicode`, () => {
      const scratch = mkdtempSync(join(tmpdir(), 'datazen-cli-'));
      try {
        const link = join(scratch, '门禁 link.mjs');
        symlinkSync(join(root, 'scripts', `${script}.mjs`), link);
        // Node may preserve the main module's literal URL when explicitly requested.
        expect(isDirectInvocation(pathToFileURL(link).href, link)).toBe(true);
        // Keep the mutation runner on its refusal path even on a clean checkout.
        const preload = join(scratch, 'preload.mjs');
        writeFileSync(preload, `import cp from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
const original = cp.spawnSync;
cp.spawnSync = function(command, args, options) {
  if (command === 'git' && args.includes('status')) return { status: 0, stdout: ' M synthetic-dirty-probe\\n', stderr: '' };
  return original(command, args, options);
};
syncBuiltinESMExports();`);
        const result = spawnSync(process.execPath, ['--import', preload, link, ...args], {
          cwd: root, encoding: 'utf8', timeout: 30000,
        });
        expect(result.error).toBeUndefined();
        expect(result.signal).toBeNull();
        expect(result.status).not.toBeNull();
        expect(result.stdout + result.stderr).toContain(marker);
      } finally { rmSync(scratch, { recursive: true, force: true }); }
    });
    it(`${script} remains silent when imported`, () => {
      const url = pathToFileURL(join(root, 'scripts', `${script}.mjs`)).href;
      const result = spawnSync(process.execPath, ['--input-type=module', '-e', `await import(${JSON.stringify(url)})`], {
        cwd: root, encoding: 'utf8', timeout: 30000,
      });
      expect(result.status).toBe(0);
      expect(result.stdout + result.stderr).toBe('');
    });
  }
});

// Build and injection wrappers run against copied sources in a disposable root.
// External commands are blocked so entry detection cannot launch a real build.
const wrapperCases = [
  ['ci-union-typecheck', [], '[ci-union-typecheck]'],
  ['ensure-generated-drivers', ['--force'], '[ensure-generated]'],
  ['with-driver-inject', ['--plugins'], '[with-driver-inject]'],
  ['ci-tauri-build', [], '[ci-tauri-build]'],
  ['driver-file-stash', ['--unknown'], 'Usage:'],
  ['run-unit-driver-set', ['--drivers='], 'Empty driver set'],
  ['resolve-drivers', ['--plugins'], '[resolve-drivers]'],
  ['driver-stash-precommit', [], '[pre-commit] Deinjecting'],
] as const;

for (const [script, args, marker] of wrapperCases) {
  it(`${script} executes via a symlink inside an isolated fixture`, () => {
    const scratch = mkdtempSync(join(tmpdir(), 'datazen-wrapper-'));
    try {
      cpSync(join(root, 'scripts'), join(scratch, 'scripts'), {
        recursive: true, filter: (path) => !path.includes('__tests__'),
      });
      mkdirSync(join(scratch, 'node_modules'));
      cpSync(join(root, 'node_modules/fflate'), join(scratch, 'node_modules/fflate'), { recursive: true, dereference: true });
      copyFileSync(join(root, 'drivers-registry.json'), join(scratch, 'drivers-registry.json'));
      writeFileSync(join(scratch, 'Cargo.toml'), '# <<driver-patches>>\n[patch."https://example.invalid"]\nprobe = { path = "probe" }\n# <</driver-patches>>\n');
      const link = join(scratch, '入口 link.mjs');
      symlinkSync(join(scratch, 'scripts', `${script}.mjs`), link);
      const preload = join(scratch, 'preload.mjs');
      writeFileSync(preload, `import cp from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
cp.execSync = function(command) {
  if (command.startsWith('git ')) return '';
  throw new Error('External command blocked by CLI regression fixture');
};
cp.spawnSync = function() { throw new Error('External command blocked by CLI regression fixture'); };
syncBuiltinESMExports();`);
      const result = spawnSync(process.execPath, ['--import', preload, link, ...args], {
        cwd: scratch, encoding: 'utf8', timeout: 30000,
      });
      expect(result.error).toBeUndefined();
      expect(result.signal).toBeNull();
      expect(result.stdout + result.stderr).toContain(marker);
      const url = pathToFileURL(join(scratch, 'scripts', `${script}.mjs`)).href;
      const imported = spawnSync(process.execPath, ['--input-type=module', '-e', `await import(${JSON.stringify(url)})`], {
        cwd: scratch, encoding: 'utf8', timeout: 30000,
      });
      expect(imported.status).toBe(0);
      expect(imported.stdout + imported.stderr).toBe('');
    } finally { rmSync(scratch, { recursive: true, force: true }); }
  });
}
