/**
 * Temporal value handling in the condition editor:
 *   - per-column-type picker selection (date / time / datetime-local)
 *   - format validation of user-typed literals
 *   - normalisation of the datetime-local "T" separator for SQL portability
 */
import { describe, expect, it } from 'vitest';
import {
  isTemporalColumnType,
  normalizeTemporalValue,
  temporalInputType,
  validateTemporalValue,
} from '../temporalValue';

describe('isTemporalColumnType', () => {
  it('recognises postgres timestamp spellings', () => {
    expect(isTemporalColumnType('timestamp without time zone')).toBe(true);
    expect(isTemporalColumnType('timestamp with time zone')).toBe(true);
    expect(isTemporalColumnType('TIMESTAMP')).toBe(true);
  });
  it('rejects non-temporal types', () => {
    expect(isTemporalColumnType('integer')).toBe(false);
    expect(isTemporalColumnType('varchar(255)')).toBe(false);
    expect(isTemporalColumnType('')).toBe(false);
  });
});

describe('temporalInputType', () => {
  it('uses the plain date picker for date columns', () => {
    expect(temporalInputType('date')).toBe('date');
  });
  it('uses the time picker for time-of-day columns', () => {
    expect(temporalInputType('time without time zone')).toBe('time');
    expect(temporalInputType('timetz')).toBe('time');
  });
  it('uses datetime-local for timestamp columns', () => {
    expect(temporalInputType('timestamp without time zone')).toBe('datetime-local');
    expect(temporalInputType('datetime')).toBe('datetime-local');
  });
  it('returns text for non-temporal columns', () => {
    expect(temporalInputType('integer')).toBe('text');
  });
});

describe('validateTemporalValue', () => {
  const at = { table: 't', column: 'created_at' };

  it('accepts date, datetime and datetime-with-seconds literals', () => {
    expect(validateTemporalValue('2026-09-20', 'date')).toBeNull();
    expect(validateTemporalValue('2026-09-20 01:13', 'timestamp without time zone')).toBeNull();
    expect(validateTemporalValue('2026-09-20 01:13:45', 'timestamp')).toBeNull();
  });
  it('accepts the datetime-local T separator while typing', () => {
    expect(validateTemporalValue('2026-09-20T01:13', 'timestamp')).toBeNull();
  });
  it('accepts time literals for time columns', () => {
    expect(validateTemporalValue('01:13', 'time')).toBeNull();
    expect(validateTemporalValue('01:13:45', 'time without time zone')).toBeNull();
  });
  it('rejects bare integers (the `timestamp > integer` bug class)', () => {
    expect(validateTemporalValue('10', 'timestamp without time zone')).toBe('invalid');
    expect(validateTemporalValue('2026', 'date')).toBe('invalid');
  });
  it('rejects wrong separators and partial input', () => {
    expect(validateTemporalValue('2026/09/20', 'date')).toBe('invalid');
    expect(validateTemporalValue('2026-09-20 01', 'timestamp')).toBe('invalid');
    expect(validateTemporalValue('', 'date')).toBe('invalid');
  });
  it('rejects impossible calendar values', () => {
    expect(validateTemporalValue('2026-13-01', 'date')).toBe('invalid');
    expect(validateTemporalValue('2026-02-30', 'date')).toBe('invalid');
    expect(validateTemporalValue('25:00', 'time')).toBe('invalid');
  });
  it('validates every entry of an IN list', () => {
    expect(validateTemporalValue('2026-01-01, 2026-02-01', 'date')).toBeNull();
    expect(validateTemporalValue('2026-01-01, x', 'date')).toBe('invalid');
  });
  it('is a no-op for non-temporal columns', () => {
    void at;
    expect(validateTemporalValue('anything', 'varchar')).toBeNull();
    expect(validateTemporalValue('10', undefined)).toBeNull();
  });
});

describe('normalizeTemporalValue', () => {
  it('replaces the T separator with a space (portable across dialects)', () => {
    expect(normalizeTemporalValue('2026-09-20T01:13', 'timestamp')).toBe('2026-09-20 01:13');
  });
  it('leaves already-normal or non-temporal values untouched', () => {
    expect(normalizeTemporalValue('2026-09-20 01:13:45', 'timestamp')).toBe('2026-09-20 01:13:45');
    expect(normalizeTemporalValue('abc', 'varchar')).toBe('abc');
    expect(normalizeTemporalValue('2026-09-20', 'date')).toBe('2026-09-20');
  });
});
