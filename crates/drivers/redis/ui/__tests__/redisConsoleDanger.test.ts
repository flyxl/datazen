/**
 * Fail-closed Console danger classifier (PRD §4 I-7, table-driven).
 *
 * Every assertion is on the enum level or the `unknown` flag — never on English
 * copy (AGENTS.md: 测试禁止断言英文字面量文案). The counter-proof suite at the
 * bottom is the acceptance line: it turns red the moment the default branch
 * stops refusing unknown commands.
 */
import { describe, expect, it } from 'vitest';
import {
  assessCommand,
  classifyDangerLevel,
  dangerBadgeColor,
  dangerRank,
  isAtLeast,
  isBlockedLevel,
  isFlushCommand,
  requiresConfirmation,
  worstLevel,
  commandNameOf,
  type DangerLevel,
} from '../console/redisConsoleDanger';
import { REDIS_COMMAND_META } from '../console/consoleCompletion/commandMeta';

type Case = readonly [command: string, expected: DangerLevel];

const SAFE_CASES: readonly Case[] = [
  ['GET foo', 'safe'],
  ['INFO', 'safe'],
  ['PING', 'safe'],
  ['TTL key', 'safe'],
  ['TYPE key', 'safe'],
  ['EXISTS key', 'safe'],
  ['SCAN 0', 'safe'],
  ['DBSIZE', 'safe'],
  ['HGETALL user:1', 'safe'],
  ['LRANGE list 0 -1', 'safe'],
  ['XRANGE stream - +', 'safe'],
  ['ZSCORE z member', 'safe'],
];

const WRITE_CASES: readonly Case[] = [
  ['SET foo bar', 'write'],
  ['LPUSH list a', 'write'],
  ['HSET h f v', 'write'],
  ['ZADD z 1 m', 'write'],
  ['SADD s a', 'write'],
  ['XADD stream * k v', 'write'],
  ['INCR counter', 'write'],
  // Dataset mutations that used to fall into the fail-open default.
  ['LPOP list', 'write'],
  ['SPOP set', 'write'],
  ['TOUCH k1 k2', 'write'],
  ['GEOADD geo 1 2 n', 'write'],
];

const DANGER_CASES: readonly Case[] = [
  ['DEL key', 'danger'],
  ['EXPIRE key 100', 'danger'],
  ['RENAME a b', 'danger'],
  ['PERSIST key', 'danger'],
  ['SUBSCRIBE ch', 'danger'],
  ['CLIENT LIST', 'danger'],
];

const BLOCKED_CASES: readonly Case[] = [
  ['FLUSHDB', 'ultra-danger'],
  ['FLUSHALL', 'ultra-danger'],
  ['KEYS *', 'ultra-danger'],
  ['CONFIG SET maxmemory 1gb', 'ultra-danger'],
  ['SHUTDOWN NOSAVE', 'ultra-danger'],
  ['DEBUG JMAP', 'ultra-danger'],
  ['SCRIPT LOAD "return 1"', 'ultra-danger'],
  ['EVAL "return 1" 0', 'ultra-danger'],
  ['EVALSHA abc123 0', 'ultra-danger'],
  ['ACL LIST', 'ultra-danger'],
  ['MODULE LIST', 'ultra-danger'],
  ['CLUSTER INFO', 'ultra-danger'],
  ['REPLICAOF 127.0.0.1 6380', 'ultra-danger'],
  ['SLAVEOF no one', 'ultra-danger'],
];

const KNOWN_CASES: readonly Case[] = [
  ...SAFE_CASES,
  ...WRITE_CASES,
  ...DANGER_CASES,
  ...BLOCKED_CASES,
];

describe('redisConsoleDanger', () => {
  describe.each([
    ['safe', SAFE_CASES],
    ['write', WRITE_CASES],
    ['danger', DANGER_CASES],
    ['ultra-danger (blocked)', BLOCKED_CASES],
  ] as const)('classifies the %s tier', (_label, cases) => {
    it.each(cases)('%s', (command, expected) => {
      expect(classifyDangerLevel(command)).toBe(expected);
    });
  });

  it('is case-insensitive and whitespace tolerant', () => {
    expect(classifyDangerLevel('get foo')).toBe('safe');
    expect(classifyDangerLevel('Set foo bar')).toBe('write');
    expect(classifyDangerLevel('del key')).toBe('danger');
    expect(classifyDangerLevel('flushdb')).toBe('ultra-danger');
    expect(classifyDangerLevel('   \t\n  KEYS   *  ')).toBe('ultra-danger');
    expect(classifyDangerLevel('\n  ping  \n')).toBe('safe');
  });

  it('parses the command name without grading on arguments', () => {
    expect(commandNameOf('  get "a key"  ')).toBe('GET');
    expect(commandNameOf('"GET" foo')).toBe('GET');
    expect(commandNameOf(" 'set' k v")).toBe('SET');
    expect(commandNameOf('PING;')).toBe('PING');
    expect(commandNameOf('GET a\nSET b 1')).toBe('GET');
    expect(commandNameOf('   ')).toBe('');
    // quoted arg containing whitespace never becomes the name
    expect(commandNameOf('MSET "k 1" v')).toBe('MSET');
  });

  it('never classifies on script-body content (EVAL body is an argument)', () => {
    const script = 'EVAL "for k in KEYS do DEL(k) FLUSHALL() end" 0';
    expect(assessCommand(script)).toMatchObject({ name: 'EVAL', unknown: false });
    expect(classifyDangerLevel(script)).toBe('ultra-danger');
    // A read-only script body must not be promoted to a command list either.
    expect(commandNameOf(script)).toBe('EVAL');
  });

  describe('fail-closed default (PRD I-7)', () => {
    const unknowns = [
      'FLUSHD',
      'GETT foo',
      'JSON.GET doc',
      'FT.SEARCH idx query',
      'BF.EXISTS filter x',
      'DUMP_XYZ k',
      'KEY',
      'DEL2 k',
      'TOTALLY_MADE_UP',
    ];

    it.each(unknowns)('treats the unrecognised %s as blocked', (command) => {
      const assessment = assessCommand(command);
      expect(assessment.level).toBe('ultra-danger');
      expect(assessment.unknown).toBe(true);
      expect(isBlockedLevel(assessment.level)).toBe(true);
    });

    it('does not mark known commands as unknown', () => {
      for (const [command] of KNOWN_CASES) {
        expect(assessCommand(command).unknown).toBe(false);
      }
    });

    it('knows every command shipped in the completion catalog', () => {
      // A catalog entry with no classification would be silently blocked in the
      // UI; this pins the shipped vocabulary against the classification table.
      const unknownCatalog = REDIS_COMMAND_META.filter(
        (meta) => assessCommand(meta.name).unknown,
      ).map((meta) => meta.name);
      expect(unknownCatalog).toEqual([]);
    });
  });

  it('reports blank input as safe (the Console refuses blank input upstream)', () => {
    expect(classifyDangerLevel('')).toBe('safe');
    expect(assessCommand('')).toMatchObject({ name: '', level: 'safe', unknown: false });
  });

  describe('requiresConfirmation', () => {
    it('requires confirmation for danger and ultra-danger', () => {
      expect(requiresConfirmation('danger')).toBe(true);
      expect(requiresConfirmation('ultra-danger')).toBe(true);
    });

    it('does not require confirmation for safe and write', () => {
      expect(requiresConfirmation('safe')).toBe(false);
      expect(requiresConfirmation('write')).toBe(false);
    });
  });

  describe('level helpers', () => {
    it('folds and compares levels', () => {
      expect(worstLevel([])).toBe('safe');
      expect(worstLevel(['safe', 'write', 'danger'])).toBe('danger');
      expect(worstLevel(['safe', 'ultra-danger', 'write'])).toBe('ultra-danger');
      expect(isAtLeast('write', 'danger')).toBe(false);
      expect(isAtLeast('ultra-danger', 'danger')).toBe(true);
      expect(dangerRank('safe')).toBeLessThan(dangerRank('ultra-danger'));
    });

    it('exposes the flush opt-in names', () => {
      expect(isFlushCommand('FLUSHDB')).toBe(true);
      expect(isFlushCommand('FLUSHALL')).toBe(true);
      expect(isFlushCommand('KEYS')).toBe(false);
      expect(isFlushCommand('')).toBe(false);
    });
  });

  describe('dangerBadgeColor', () => {
    it('returns correct colors', () => {
      expect(dangerBadgeColor('ultra-danger')).toContain('bg-red');
      expect(dangerBadgeColor('danger')).toContain('bg-orange');
      expect(dangerBadgeColor('write')).toContain('bg-yellow');
      expect(dangerBadgeColor('safe')).toContain('bg-green');
    });

    it('gives unknown commands their own badge so they are visually distinct', () => {
      expect(dangerBadgeColor('ultra-danger', true)).not.toBe(dangerBadgeColor('ultra-danger'));
      expect(dangerBadgeColor('ultra-danger', true)).toContain('ring');
    });
  });
});
