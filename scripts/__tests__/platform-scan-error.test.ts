import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

it('CLI attributes a mid-scan read error to F-07 and recovers to 0 violations / 0 errors', () => {
  const root = mkdtempSync(join(tmpdir(), 'datazen-scan-'));
  try {
    const source = join(root, 'packages/backend-client');
    mkdirSync(source, { recursive: true });
    writeFileSync(join(source, 'a.ts'), 'export const valid = true;');
    // A dangling source-file link fails readFileSync on every OS/user, including root.
    symlinkSync(join(root, 'missing.ts'), join(source, 'z.ts'));
    const specDir = join(root, 'docs/architecture/platform');
    mkdirSync(specDir, { recursive: true });
    copyFileSync(join(repo, 'docs/architecture/platform/shared-boundaries-and-ports.md'), join(specDir, 'shared-boundaries-and-ports.md'));
    const bin = join(root, 'bin');
    mkdirSync(bin);
    const cargo = join(bin, 'cargo');
    const metadata = { workspace_root: root, workspace_members: [], packages: [], resolve: { nodes: [] } };
    writeFileSync(cargo, `#!${process.execPath}\nconsole.log(${JSON.stringify(JSON.stringify(metadata))});\n`);
    chmodSync(cargo, 0o755);
    const run = () => spawnSync(process.execPath, [join(repo, 'scripts/check-platform-crate-boundaries.mjs'), `--root=${root}`, '--require-layers='], {
      cwd: root, env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}` }, encoding: 'utf8',
    });
    const failed = run();
    expect(failed.status).toBe(1);
    expect(failed.stderr).toContain('ERROR    F-07: could not scan packages/backend-client');
    expect(failed.stdout).toContain('fail F-07:');
    expect(failed.stdout).not.toContain('ok F-07:');
    expect(failed.stdout).toContain('0 violation(s), 1 error(s)');
    unlinkSync(join(source, 'z.ts'));
    const clean = run();
    expect(clean.status).toBe(0);
    expect(clean.stdout).toContain('ok F-07:');
    expect(clean.stdout).toContain('0 violation(s), 0 error(s)');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
