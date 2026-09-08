/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { parseBuildArgs, buildCommandString } from '../tauri-build.mjs';

describe('tauri-build parseBuildArgs', () => {
  it('defaults to pro edition and all drivers when no flags passed', () => {
    const res = parseBuildArgs([], {});
    expect(res.edition).toBe('pro');
    expect(res.drivers).toBe('all');
    expect(res.extraArgs).toEqual([]);
  });

  it('respects environment variables for default edition and drivers', () => {
    const res = parseBuildArgs([], {
      DATAZEN_EDITION: 'community',
      DATAZEN_DRIVERS: 'basic',
    });
    expect(res.edition).toBe('community');
    expect(res.drivers).toBe('basic');
  });

  it('parses --edition and --drivers simultaneously', () => {
    const res = parseBuildArgs([
      '--edition=pro',
      '--drivers=basic',
      '--target=x86_64-apple-darwin',
      '--debug',
    ]);
    expect(res.edition).toBe('pro');
    expect(res.drivers).toBe('basic');
    expect(res.extraArgs).toEqual(['--target=x86_64-apple-darwin', '--debug']);
  });

  it('supports alias flags --pro and --community', () => {
    expect(parseBuildArgs(['--pro']).edition).toBe('pro');
    expect(parseBuildArgs(['--community']).edition).toBe('community');
  });

  it('parses custom driver lists with custom pro path', () => {
    const res = parseBuildArgs(['--drivers=postgres,mysql,clickhouse', '--pro-path=/custom/pro']);
    expect(res.drivers).toBe('postgres,mysql,clickhouse');
    expect(res.proPath).toBe('/custom/pro');
  });
});

describe('tauri-build buildCommandString', () => {
  it('builds command with both inject and build arguments', () => {
    const cmd = buildCommandString({
      edition: 'pro',
      drivers: 'basic',
      proPath: null,
      proGit: null,
      extraArgs: ['--target=x86_64-pc-windows-msvc', '--debug'],
    });

    expect(cmd).toBe(
      'node scripts/with-driver-inject.mjs --drivers=basic --edition=pro -- node scripts/ci-tauri-build.mjs --edition=pro --target=x86_64-pc-windows-msvc --debug',
    );
  });

  it('builds command for community edition', () => {
    const cmd = buildCommandString({
      edition: 'community',
      drivers: 'all',
      proPath: null,
      proGit: null,
      extraArgs: [],
    });

    expect(cmd).toBe(
      'node scripts/with-driver-inject.mjs --drivers=all --edition=community -- node scripts/ci-tauri-build.mjs --edition=community',
    );
  });
});
