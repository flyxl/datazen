/**
 * Console batch helpers (task book §6.1 R-3 + §6.4 P0-3 refusal copy).
 * Assertions are on enum levels, partition membership and i18n keys — never on
 * English sentences.
 */
import { describe, expect, it } from 'vitest';
import {
  assessCommands,
  badgeAssessment,
  composeBlockedMessage,
  describeCommands,
  splitConsoleCommands,
} from '../console/consoleCommandBatch';

/**
 * `t` stand-in that keeps the key *and* its interpolation values observable, so
 * assertions pin the i18n key instead of the English sentence.
 */
const t = (key: string, params?: Record<string, string>) =>
  params ? `${key}|${Object.values(params).join('|')}` : key;

describe('splitConsoleCommands', () => {
  it('mirrors the server line split and drops blanks', () => {
    expect(splitConsoleCommands('GET a\n\n  \nSET b 1\n')).toEqual(['GET a', 'SET b 1']);
    expect(splitConsoleCommands('  PING  ')).toEqual(['PING']);
    expect(splitConsoleCommands('')).toEqual([]);
  });

  it('keeps a quoted argument that spans lines inside one command', () => {
    const commands = splitConsoleCommands('EVAL "for k in KEYS do end\nDEL(k)" 0\nGET a');
    expect(commands).toHaveLength(2);
    expect(commands[0]).toContain('DEL(k)');
    expect(commands[1]).toBe('GET a');
  });

  it('honours escaped quotes and a dangling quote at end of input', () => {
    expect(splitConsoleCommands('SET a "b\\"c"\nGET d')).toEqual(['SET a "b\\"c"', 'GET d']);
    expect(splitConsoleCommands('SET a "unclosed')).toEqual(['SET a "unclosed']);
  });
});

describe('assessCommands', () => {
  it('grades the whole batch, not just its first line', () => {
    const batch = assessCommands('GET a\nDEL b');
    expect(batch.worst).toBe('danger');
    expect(batch.confirmations.map((command) => command.raw)).toEqual(['DEL b']);
    expect(batch.blocked).toHaveLength(0);
  });

  it('partitions blocked commands into destructive and unknown', () => {
    const batch = assessCommands('KEYS *\nMYSTERY x');
    expect(batch.blocked.map((command) => command.name)).toEqual(['KEYS', 'MYSTERY']);
    expect(batch.destructive.map((command) => command.name)).toEqual(['KEYS']);
    expect(batch.unknown.map((command) => command.name)).toEqual(['MYSTERY']);
  });

  it('keeps FLUSHDB/FLUSHALL blocked unless allowFlush is on', () => {
    expect(assessCommands('FLUSHDB').blocked.map((c) => c.name)).toEqual(['FLUSHDB']);
    const allowed = assessCommands('FLUSHALL\nSET a 1', true);
    expect(allowed.blocked).toHaveLength(0);
    expect(allowed.doubleConfirmations.map((c) => c.name)).toEqual(['FLUSHALL']);
    expect(allowed.worst).toBe('ultra-danger');
  });

  it('does not let allowFlush release anything but the flush commands', () => {
    const batch = assessCommands('KEYS *\nFLUSHDB', true);
    expect(batch.blocked.map((c) => c.name)).toEqual(['KEYS']);
    expect(batch.doubleConfirmations.map((c) => c.name)).toEqual(['FLUSHDB']);
  });

  it('treats blank input as an empty batch', () => {
    const batch = assessCommands('  \n \n');
    expect(batch.commands).toHaveLength(0);
    expect(batch.worst).toBe('safe');
    expect(batch.blocked).toEqual([]);
    expect(batch.confirmations).toEqual([]);
    expect(batch.doubleConfirmations).toEqual([]);
  });

  it('reports a safe-only batch without any confirmation targets', () => {
    const batch = assessCommands('PING\nGET a');
    expect(batch.worst).toBe('safe');
    expect(batch.confirmations).toEqual([]);
  });
});

describe('badgeAssessment', () => {
  it('shows the strictest command of the batch', () => {
    expect(badgeAssessment('SET a 1\nGET b').level).toBe('write');
    expect(badgeAssessment('GET a\nKEYS *').level).toBe('ultra-danger');
  });

  it('marks the badge unknown when any command was unrecognised', () => {
    expect(badgeAssessment('GET a\nNOTA_COMMAND').unknown).toBe(true);
    expect(badgeAssessment('GET a').unknown).toBe(false);
    expect(badgeAssessment('').level).toBe('safe');
    expect(badgeAssessment('').unknown).toBe(false);
  });
});

describe('describeCommands', () => {
  it('joins the listed commands', () => {
    const batch = assessCommands('DEL a\nEXPIRE b 1');
    expect(describeCommands(batch.confirmations)).toBe('DEL a, EXPIRE b 1');
  });

  it('truncates a long script body instead of flooding the dialog', () => {
    const batch = assessCommands('SET supercalifragilisticexpialidocious averyveryverylongvalue');
    const listing = describeCommands(batch.commands, 20);
    expect(listing.length).toBeLessThanOrEqual(20);
    expect(listing.endsWith('…')).toBe(true);
  });
});

describe('composeBlockedMessage', () => {
  it('names destructive and unknown commands with their own copy keys', () => {
    const batch = assessCommands('KEYS *\nMYSTERY 1');
    const message = composeBlockedMessage(batch, t);
    expect(message).toContain('redis.consoleSafety.blockedDestructive');
    expect(message).toContain('redis.consoleSafety.blockedUnknown');
    expect(message).toContain('redis.consoleSafety.blockedHint');
    expect(message).toContain('KEYS *');
    expect(message).toContain('MYSTERY 1');
  });

  it('omits the destructive line when nothing known was blocked', () => {
    const message = composeBlockedMessage(assessCommands('MYSTERY 1'), t);
    expect(message).not.toContain('redis.consoleSafety.blockedDestructive');
    expect(message).toContain('redis.consoleSafety.blockedUnknown');
  });
});
