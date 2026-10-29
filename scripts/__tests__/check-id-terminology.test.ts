/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { checkIdTerminology } from '../check-id-terminology.mjs';

describe('checkIdTerminology', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'check-ids-'));
    mkdirSync(join(root, 'src'), { recursive: true });
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  const run = () => {
    const errors: string[] = [];
    const code = checkIdTerminology({ root, dirs: ['src'], log: () => {}, error: (msg) => errors.push(String(msg)) });
    return { code, errors };
  };

  it('returns 0 for a clean file', () => {
    writeFileSync(
      join(root, 'src', 'clean.ts'),
      'const entry = { connectionId, dbSessionId };\n',
    );
    expect(run().code).toBe(0);
  });

  it('returns 1 and reports file:line for a forbidden token', () => {
    writeFileSync(join(root, 'src', 'stale.ts'), 'const a = 1;\nconst activeConfigId = x;\n');
    const { code, errors } = run();
    expect(code).toBe(1);
    expect(errors.join('\n')).toContain('src/stale.ts:2');
    expect(errors.join('\n')).toContain('activeConfigId');
  });

  it('flags the reversed form of stuffing a config id into a session key', () => {
    writeFileSync(join(root, 'src', 'reversed.ts'), 'invoke("q", { dbSessionId: config.id });\n');
    expect(run().code).toBe(1);
  });

  it('suppresses allow-listed lines only when file and line both match', () => {
    // The allow-list keys the historical comment by its F6 path under
    // packages/frontend/src, so the fixture must mirror that layout exactly.
    mkdirSync(join(root, 'packages/frontend/src', 'commands'), { recursive: true });
    const file = join(root, 'packages/frontend/src', 'commands', 'schemaDiff.ts');
    const runAtLayout = () => {
      const errors: string[] = [];
      const code = checkIdTerminology({
        root,
        dirs: ['packages/frontend/src'],
        log: () => {},
        error: (msg) => errors.push(String(msg)),
      });
      return { code, errors };
    };
    // Mirrors the whitelisted historical-format comment in schemaDiff.ts.
    writeFileSync(
      file,
      "// v1 configs with configId are rejected.\nconst legacyPayload = { configId: 'old' };\n",
    );
    const { code, errors } = runAtLayout();
    expect(code).toBe(1); // line 2 carries the same token outside the allow-list
    expect(errors.join('\n')).toContain('schemaDiff.ts:2');
    // Same file, allow-listed line only → clean.
    writeFileSync(file, '// v1 configs with configId are rejected.\n');
    expect(runAtLayout().code).toBe(0);
  });
});
