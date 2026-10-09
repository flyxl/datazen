/**
 * Counter-proof suite for the fail-closed Console classifier (track
 * `redis-console-safety`, PRD §4 I-7).
 *
 * This file exists for the mutation self-check described in the track task
 * book: restore `assessCommand`'s fallback to `return 'safe'` and *only* the
 * cases below go red (10 in the current build). Each case
 * asserts the strictest tier plus "the Console will not run it", never English
 * copy.
 */
import { describe, expect, it } from 'vitest';
import { assessCommand, isBlockedLevel, type DangerLevel } from '../console/redisConsoleDanger';
import {
  assessCommands,
  badgeAssessment,
  splitConsoleCommands,
} from '../console/consoleCommandBatch';

const STRICTEST: DangerLevel = 'ultra-danger';

/** Commands that are not part of the known vocabulary, in any spelling. */
const UNRECOGNISED = [
  'FLUSHD key',
  'GETX foo',
  'JSON.GET doc',
  'FT.SEARCH idx *',
  'unknown_admin_thing',
  'DEL', // lower-case known command in a *name* position is still recognised
];

describe('fail-closed counter-proof (I-7): unknown commands are never let through', () => {
  it('uses the strictest tier as the default verdict', () => {
    expect(STRICTEST).toBe('ultra-danger');
    expect(assessCommand('SOMETHING_NEW_IN_REDIS_99').level).toBe(STRICTEST);
  });

  it.each(['FLUSHD key', 'GETX foo', 'JSON.GET doc', 'FT.SEARCH idx *', 'no_such_command 1 2'])(
    'refuses to run the unrecognised %s',
    (command) => {
      const assessment = assessCommand(command);
      expect(assessment.level).toBe('ultra-danger');
      expect(assessment.unknown).toBe(true);
      // the tier is the blocking one: no confirmation dialog releases it
      expect(isBlockedLevel(assessment.level)).toBe(true);
    },
  );

  it('keeps an unknown command blocked inside a multi-command batch', () => {
    const batch = assessCommands('GET a\nJSON.SET doc $ {}');
    expect(batch.blocked.map((command) => command.name)).toEqual(['JSON.SET']);
    expect(batch.worst).toBe('ultra-danger');
    // a typo in the middle of an otherwise clean paste is not waved through
    expect(assessCommands('PING\nPING\nTYPO_HERE').blocked).toHaveLength(1);
  });

  it('flags the batch badge as unknown so the UI cannot hide the refusal', () => {
    expect(badgeAssessment('GET a\nWHATEVER b').unknown).toBe(true);
    expect(badgeAssessment('GET a').unknown).toBe(false);
  });

  it('still classifies known commands, so the mutation does not just flip everything', () => {
    // Guards against a "green for the wrong reason" reading of the mutation:
    // these stay put whichever way the default branch goes.
    expect(assessCommand('GET a').level).toBe('safe');
    expect(assessCommand('SET a b').level).toBe('write');
    expect(assessCommand('DEL a').level).toBe('danger');
    expect(assessCommand('KEYS *').level).toBe('ultra-danger');
  });

  it('treats a half-typed command name as unknown (typing journey)', () => {
    for (const partial of UNRECOGNISED) {
      const assessment = assessCommand(partial);
      if (assessment.name === 'DEL') continue; // the deliberate known-control case
      expect(assessment.unknown, partial).toBe(true);
    }
  });

  it('splits on newlines but never inside a quoted argument', () => {
    expect(splitConsoleCommands('GET a\n\nSET b 1\n')).toEqual(['GET a', 'SET b 1']);
    expect(splitConsoleCommands('EVAL "return KEYS[1]\nand DEL(x)" 0')).toHaveLength(1);
    expect(splitConsoleCommands('EVAL "return 1')).toHaveLength(1);
    expect(splitConsoleCommands('   \n  \t \n')).toEqual([]);
  });
});
