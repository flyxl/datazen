/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { checkLegacyConnectionRegistry } from '../check-legacy-connection-registry.mjs';

describe('checkLegacyConnectionRegistry', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'check-legacy-registry-'));
    mkdirSync(join(root, 'src'), { recursive: true });
    mkdirSync(join(root, 'src-tauri', 'src'), { recursive: true });
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  const run = () => {
    const errors: string[] = [];
    const code = checkLegacyConnectionRegistry({
      root,
      dirs: ['src', 'src-tauri'],
      log: () => {},
      error: (msg) => errors.push(String(msg)),
    });
    return { code, errors };
  };

  it('returns 0 for a clean tree', () => {
    writeFileSync(join(root, 'src', 'clean.ts'), 'const registry = new Map();\n');
    writeFileSync(
      join(root, 'src-tauri', 'src', 'clean.rs'),
      'use datazen_runtime::job::JobClock;\n',
    );
    expect(run().code).toBe(0);
  });

  it('returns 1 and reports file:line for a re-introduced manager', () => {
    writeFileSync(
      join(root, 'src', 'stale.ts'),
      'const a = 1;\nconst m = new ConnectionManager();\n',
    );
    const { code, errors } = run();
    expect(code).toBe(1);
    expect(errors.join('\n')).toContain('src/stale.ts:2');
    expect(errors.join('\n')).toContain('ConnectionManager');
  });

  it('flags the snake_case Rust twin', () => {
    writeFileSync(join(root, 'src-tauri', 'src', 'stale.rs'), 'mod connection_manager;\n');
    const { code, errors } = run();
    expect(code).toBe(1);
    expect(errors.join('\n')).toContain('connection_manager');
  });

  it('does not flag the runtime session path', () => {
    writeFileSync(
      join(root, 'src', 'runtime.ts'),
      'const registry = new SessionRegistry();\nconst sessions = registry.sessions;\n',
    );
    expect(run().code).toBe(0);
  });
});
