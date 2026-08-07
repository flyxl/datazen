/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  planPluginInjectLifecycle,
  runWithPluginInject,
} from '../with-plugin-inject.mjs';

describe('planPluginInjectLifecycle', () => {
  it('owns stash when none exists', () => {
    expect(planPluginInjectLifecycle(() => false)).toEqual({
      ownStash: true,
      nested: false,
    });
  });

  it('is nested when stash already exists', () => {
    expect(planPluginInjectLifecycle(() => true)).toEqual({
      ownStash: false,
      nested: true,
    });
  });
});

describe('runWithPluginInject nested ownership', () => {
  it('outer: resolves, runs command, restores', () => {
    const calls: string[] = [];
    const result = runWithPluginInject({
      argv: ['--plugins=none', '--', 'echo', 'ok'],
      stashExistsFn: () => false,
      runResolve: (args) => {
        calls.push(`resolve:${args}`);
      },
      runRestore: () => {
        calls.push('restore');
      },
      runCommand: (cmd, args) => {
        calls.push(`cmd:${cmd} ${args.join(' ')}`);
        return { status: 0 };
      },
      log: () => {},
    });

    expect(result).toEqual({ status: 0, ownStash: true, nested: false });
    expect(calls).toEqual([
      'resolve:--plugins=none',
      'cmd:echo ok',
      'restore',
    ]);
  });

  it('nested: skips resolve and restore so outer stash stays for rust build', () => {
    const calls: string[] = [];
    const logs: string[] = [];
    const result = runWithPluginInject({
      argv: ['--', 'tsc'],
      stashExistsFn: () => true,
      runResolve: () => {
        calls.push('resolve');
      },
      runRestore: () => {
        calls.push('restore');
      },
      runCommand: (cmd) => {
        calls.push(`cmd:${cmd}`);
        return { status: 0 };
      },
      log: (msg) => logs.push(msg),
    });

    expect(result).toEqual({ status: 0, ownStash: false, nested: true });
    expect(calls).toEqual(['cmd:tsc']);
    expect(logs.some((l) => /skipping resolve\/restore/.test(l))).toBe(true);
  });

  it('nested with no command still does not restore', () => {
    const calls: string[] = [];
    const result = runWithPluginInject({
      argv: ['--plugins=kiwi'],
      stashExistsFn: () => true,
      runResolve: () => {
        calls.push('resolve');
      },
      runRestore: () => {
        calls.push('restore');
      },
      log: () => {},
    });

    expect(result.status).toBe(0);
    expect(result.nested).toBe(true);
    expect(calls).toEqual([]);
  });
});
