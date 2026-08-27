/** @vitest-environment node */
import { describe, expect, it, vi } from 'vitest';
import {
  INJECT_ACTIVE_ENV,
  planDriverInjectLifecycle,
  runWithDriverInject,
} from '../with-driver-inject.mjs';

describe('runWithDriverInject rejected flags', () => {
  it('returns status 1 for --plugins', () => {
    const result = runWithDriverInject({
      argv: ['--plugins=kiwi', '--', 'true'],
      stashExistsFn: () => false,
      env: {},
      runResolve: () => {
        throw new Error('must not resolve');
      },
      log: () => {},
    });
    expect(result.status).toBe(1);
    expect(result.ownStash).toBe(false);
  });

  it('returns status 1 for DATAZEN_PLUGINS', () => {
    const result = runWithDriverInject({
      argv: ['--', 'true'],
      stashExistsFn: () => false,
      env: { DATAZEN_PLUGINS: 'kiwi' },
      runResolve: () => {
        throw new Error('must not resolve');
      },
      log: () => {},
    });
    expect(result.status).toBe(1);
  });
});

describe('planDriverInjectLifecycle', () => {
  it('owns stash when none exists', () => {
    expect(planDriverInjectLifecycle({ exists: () => false, env: {} })).toEqual({
      ownStash: true,
      nested: false,
      orphanStash: false,
    });
  });

  it('treats leftover stash without env as orphan (take ownership)', () => {
    expect(planDriverInjectLifecycle({ exists: () => true, env: {} })).toEqual({
      ownStash: true,
      nested: false,
      orphanStash: true,
    });
  });

  it('is nested only when inject-active env is set', () => {
    expect(
      planDriverInjectLifecycle({
        exists: () => true,
        env: { [INJECT_ACTIVE_ENV]: '1' },
      }),
    ).toEqual({
      ownStash: false,
      nested: true,
      orphanStash: false,
    });
  });

  it('accepts legacy function form for exists', () => {
    vi.stubEnv(INJECT_ACTIVE_ENV, '');
    try {
      expect(planDriverInjectLifecycle(() => false)).toEqual({
        ownStash: true,
        nested: false,
        orphanStash: false,
      });
    } finally {
      vi.unstubAllEnvs();
    }
  });
});

describe('runWithDriverInject nested ownership', () => {
  it('outer: resolves, runs command, restores', () => {
    const calls: string[] = [];
    const result = runWithDriverInject({
      argv: ['--drivers=basic', '--', 'echo', 'ok'],
      stashExistsFn: () => false,
      env: {},
      runResolve: (args) => {
        calls.push(`resolve:${args}`);
      },
      runRestore: () => {
        calls.push('restore');
      },
      runCommand: (cmd, args, env) => {
        calls.push(`cmd:${cmd} ${args.join(' ')}`);
        expect(env[INJECT_ACTIVE_ENV]).toBe('1');
        return { status: 0 };
      },
      log: () => {},
    });

    expect(result).toEqual({
      status: 0,
      ownStash: true,
      nested: false,
      orphanStash: false,
    });
    expect(calls).toEqual(['resolve:--drivers=basic', 'cmd:echo ok', 'restore']);
  });

  it('orphan stash: restore then resolve/command/restore', () => {
    const calls: string[] = [];
    const logs: string[] = [];
    const result = runWithDriverInject({
      argv: ['--', 'echo', 'ok'],
      stashExistsFn: () => true,
      env: {},
      runResolve: (args) => {
        calls.push(`resolve:${args}`);
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

    expect(result.ownStash).toBe(true);
    expect(result.orphanStash).toBe(true);
    expect(result.nested).toBe(false);
    expect(calls).toEqual(['restore', 'resolve:', 'cmd:echo', 'restore']);
    expect(logs.some((l) => /orphan/.test(l))).toBe(true);
  });

  it('nested via env: skips resolve and restore', () => {
    const calls: string[] = [];
    const logs: string[] = [];
    const result = runWithDriverInject({
      argv: ['--', 'tsc'],
      stashExistsFn: () => true,
      env: { [INJECT_ACTIVE_ENV]: '1' },
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

    expect(result).toEqual({
      status: 0,
      ownStash: false,
      nested: true,
      orphanStash: false,
    });
    expect(calls).toEqual(['cmd:tsc']);
    expect(logs.some((l) => /skipping resolve\/restore/.test(l))).toBe(true);
  });

  it('ownStash with no command still resolves and restores', () => {
    const calls: string[] = [];
    const result = runWithDriverInject({
      argv: ['--drivers=basic'],
      stashExistsFn: () => false,
      env: {},
      runResolve: () => {
        calls.push('resolve');
      },
      runRestore: () => {
        calls.push('restore');
      },
      log: () => {},
    });
    expect(result.status).toBe(0);
    expect(result.ownStash).toBe(true);
    expect(calls).toEqual(['resolve', 'restore']);
  });

  it('maps null command status to 1', () => {
    const result = runWithDriverInject({
      argv: ['--', 'false'],
      stashExistsFn: () => false,
      env: {},
      runResolve: () => {},
      runRestore: () => {},
      runCommand: () => ({ status: null }),
      log: () => {},
    });
    expect(result.status).toBe(1);
  });

  it('nested with no command still does not restore', () => {
    const calls: string[] = [];
    const result = runWithDriverInject({
      argv: ['--drivers=kiwi'],
      stashExistsFn: () => true,
      env: { [INJECT_ACTIVE_ENV]: '1' },
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
