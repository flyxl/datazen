import { describe, expect, it } from 'vitest';
import {
  type InstantParts,
  type WallComponents,
  compareInstants,
  formatOffsetMinutes,
  formatOffsetSeconds,
  instantToWallParts,
  isValidTimeZoneName,
  MAX_SUPPORTED_EPOCH_MS,
  MIN_SUPPORTED_EPOCH_MS,
  parseUnixValue,
  parseWallClockText,
  parseZonedTimestamp,
  validateInstantParts,
  wallClockToInstant,
} from '../temporalMath';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Parse a zoned timestamp that is expected to succeed (fail loud on error). */
function instantOf(text: string): InstantParts {
  const result = parseZonedTimestamp(text);
  if (result.kind !== 'ok') {
    throw new Error(`parseZonedTimestamp(${text}) failed: ${JSON.stringify(result.reason)}`);
  }
  return result.instant;
}

/** Parse a Unix value that is expected to succeed (fail loud on error). */
function unixOf(value: number | string, unit: 'seconds' | 'milliseconds'): InstantParts {
  const result = parseUnixValue(value, unit);
  if (result.kind !== 'ok') {
    throw new Error(`parseUnixValue(${value}, ${unit}) failed: ${JSON.stringify(result.reason)}`);
  }
  return result.instant;
}

/** Wall components with a neutral default date (2026-01-15 00:00:00). */
function wall(overrides: Partial<WallComponents>): WallComponents {
  return { year: 2026, month: 1, day: 15, hour: 0, minute: 0, second: 0, nanos: 0, ...overrides };
}

/**
 * Machine-default-timezone control.
 *
 * Every temporalMath API takes an explicit zone except the wall-clock parsers,
 * which must be zone-free by contract. To prove that contract against the
 * runner's default zone we PIN `process.env.TZ` to known IANA zones for the
 * duration of an assertion and restore the original value afterwards
 * (including the "unset" state). Node ≥ 13 re-reads TZ for both Date local
 * accessors and the Intl default zone, so the pin is observable; each test
 * asserts the pin took effect before asserting zone-independence, so a future
 * runtime that stops honoring runtime TZ changes fails loudly instead of
 * silently proving nothing.
 */
const ORIGINAL_TZ = process.env.TZ;

function withPinnedTz<T>(tz: string, run: () => T): T {
  process.env.TZ = tz;
  try {
    return run();
  } finally {
    if (ORIGINAL_TZ === undefined) delete process.env.TZ;
    else process.env.TZ = ORIGINAL_TZ;
  }
}

/** Exact total ordering reference for InstantParts (BigInt, no float loss). */
function instantKey(parts: InstantParts): bigint {
  return BigInt(parts.epochMs) * 1_000_000n + BigInt(parts.subSecondNanos);
}

// ---------------------------------------------------------------------------
// parseZonedTimestamp — valid forms
// ---------------------------------------------------------------------------

describe('parseZonedTimestamp: valid forms', () => {
  it('parses the PRD anchor instant', () => {
    // 2026-01-15T14:30:00Z (issue-32 PRD §4 correction example).
    expect(parseZonedTimestamp('2026-01-15T14:30:00Z')).toEqual({
      kind: 'ok',
      instant: { epochMs: 1768487400000, subSecondNanos: 0, fractionDigits: 0 },
    });
  });

  it('accepts lowercase t/z separators and single-space database style', () => {
    expect(instantOf('2026-01-15t14:30:00z').epochMs).toBe(1768487400000);
    expect(instantOf('2026-01-15 14:30:00Z').epochMs).toBe(1768487400000);
  });

  it('applies +HH:MM offsets', () => {
    expect(instantOf('2026-01-15T22:30:00+08:00')).toEqual({
      epochMs: 1768487400000,
      subSecondNanos: 0,
      fractionDigits: 0,
    });
  });

  it('applies compact -HHMM offsets', () => {
    expect(instantOf('2026-01-15T09:30:00-0500').epochMs).toBe(1768487400000);
  });

  it('applies non-whole-hour +05:45 offsets', () => {
    expect(instantOf('2026-01-15T20:15:00+05:45').epochMs).toBe(1768487400000);
    expect(instantOf('2026-01-15T20:15:00+0545').epochMs).toBe(1768487400000);
  });

  it('accepts Postgres-style short ±HH offsets', () => {
    expect(instantOf('2026-01-15T22:30:00+08').epochMs).toBe(1768487400000);
    expect(instantOf('2026-01-15T09:30:00-05').epochMs).toBe(1768487400000);
  });

  it('treats -00:00 as a zero offset (RFC 3339 unknown-offset form)', () => {
    expect(instantOf('2026-01-15T14:30:00-00:00').epochMs).toBe(1768487400000);
  });

  it('accepts the seconds-omitted shape (minute precision)', () => {
    // 14:30 wall at +08:00 → 06:30Z same day.
    expect(instantOf('2026-01-15T14:30+08:00')).toEqual({
      epochMs: 1768458600000,
      subSecondNanos: 0,
      fractionDigits: 0,
    });
  });

  it('keeps 3-digit fractional seconds in epochMs', () => {
    expect(instantOf('2026-01-15T14:30:00.123Z')).toEqual({
      epochMs: 1768487400123,
      subSecondNanos: 0,
      fractionDigits: 3,
    });
  });

  it('keeps 6-digit fractional seconds split across epochMs and nanos', () => {
    expect(instantOf('2026-01-15T14:30:00.123456Z')).toEqual({
      epochMs: 1768487400123,
      subSecondNanos: 456000,
      fractionDigits: 6,
    });
  });

  it('keeps 9-digit fractional seconds losslessly', () => {
    expect(instantOf('2026-01-15T14:30:00.123456789Z')).toEqual({
      epochMs: 1768487400123,
      subSecondNanos: 456789,
      fractionDigits: 9,
    });
  });

  it('keeps sub-millisecond-only fractions out of epochMs', () => {
    expect(instantOf('2026-01-15T14:30:00.000456Z')).toEqual({
      epochMs: 1768487400000,
      subSecondNanos: 456000,
      fractionDigits: 6,
    });
  });

  it('preserves explicit all-zero 9-digit source precision', () => {
    expect(instantOf('2026-01-15T14:30:00.000000000Z')).toEqual({
      epochMs: 1768487400000,
      subSecondNanos: 0,
      fractionDigits: 9,
    });
  });

  it('satisfies the floor invariant for negative instants', () => {
    // -876.544 ms → epochMs floor = -877, sub-millisecond remainder 456000 ns
    // (InstantParts invariant: epochMs is floored, subSecondNanos ≥ 0).
    expect(instantOf('1969-12-31T23:59:59.123456Z')).toEqual({
      epochMs: -877,
      subSecondNanos: 456000,
      fractionDigits: 6,
    });
  });

  it('accepts leap days for actual leap years only', () => {
    expect(instantOf('2024-02-29T00:00:00Z').epochMs).toBe(1709164800000);
    expect(instantOf('2000-02-29T00:00:00Z').epochMs).toBe(951782400000);
  });

  it('maps 0–99 years to the first century, not 1900–1999', () => {
    // Date.UTC(50, …) is 1950; the module's +400-year correction must land
    // on astronomical year 50 (epochMs -60589296000000).
    expect(instantOf('0050-01-01T00:00:00Z').epochMs).toBe(-60589296000000);
    expect(instantOf('0000-01-01T00:00:00Z').epochMs).toBe(-62167219200000);
  });

  it('accepts signed extended years within the supported range', () => {
    expect(instantOf('+10000-01-01T00:00:00Z').epochMs).toBe(253402300800000);
    expect(instantOf('-0050-01-15T12:00:00Z').epochMs).toBe(-63743803200000);
  });
});

// ---------------------------------------------------------------------------
// parseZonedTimestamp — invalid inputs
// ---------------------------------------------------------------------------

describe('parseZonedTimestamp: invalid inputs', () => {
  it('rejects non-timestamp text', () => {
    expect(parseZonedTimestamp('not-a-date')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
  });

  it('rejects out-of-range months', () => {
    expect(parseZonedTimestamp('2026-13-01T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-month' },
    });
    expect(parseZonedTimestamp('2026-00-10T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-month' },
    });
  });

  it('rejects impossible days including non-leap Feb 29', () => {
    expect(parseZonedTimestamp('2026-02-30T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-day' },
    });
    expect(parseZonedTimestamp('2026-02-29T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-day' },
    });
    // Century rule: 1900 is not a leap year (400-year exception does not hit).
    expect(parseZonedTimestamp('1900-02-29T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-day' },
    });
  });

  it('rejects 24:00 end-of-day, :60 minutes and leap seconds', () => {
    expect(parseZonedTimestamp('2026-01-15T24:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-hour' },
    });
    expect(parseZonedTimestamp('2026-01-15T23:60:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-minute' },
    });
    expect(parseZonedTimestamp('2026-01-15T23:59:60Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-second' },
    });
  });

  it('rejects an empty string', () => {
    expect(parseZonedTimestamp('')).toEqual({ kind: 'error', reason: { reason: 'empty-input' } });
  });

  it('rejects fractional seconds beyond 9 digits', () => {
    expect(parseZonedTimestamp('2026-01-15T14:30:00.1234567890Z')).toEqual({
      kind: 'error',
      reason: { reason: 'fraction-digits-out-of-range' },
    });
  });

  it('rejects surrounding whitespace', () => {
    expect(parseZonedTimestamp(' 2026-01-15T14:30:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
    expect(parseZonedTimestamp('2026-01-15T14:30:00Z ')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
  });

  it('rejects zone-less, date-only and non-padded forms', () => {
    // Zone-less values belong to parseWallClockText, not to the zoned parser.
    expect(parseZonedTimestamp('2026-01-15T14:30:00')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
    expect(parseZonedTimestamp('2026-01-15')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
    expect(parseZonedTimestamp('2026-1-15T14:30:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
  });

  it('rejects out-of-range zone offsets', () => {
    expect(parseZonedTimestamp('2026-01-15T14:30:00+24:00')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-offset' },
    });
    expect(parseZonedTimestamp('2026-01-15T14:30:00+05:60')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-offset' },
    });
  });

  it('rejects non-string input defensively', () => {
    // Runtime garbage guard; the signature is string, hence the double cast
    // (no `any` in tests).
    expect(parseZonedTimestamp(42 as unknown as string)).toEqual({
      kind: 'error',
      reason: { reason: 'not-a-string' },
    });
  });

  it('rejects years beyond the ±100000 support range', () => {
    expect(parseZonedTimestamp('+100001-01-01T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
    expect(parseZonedTimestamp('-100001-01-01T00:00:00Z')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
    // 6-digit garbage year: Date.UTC overflows to NaN → out-of-range.
    expect(parseZonedTimestamp('999999-12-31T23:59:59.999Z')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
  });

  it('accepts the exact support-range boundaries', () => {
    expect(instantOf('+100000-12-31T23:59:59.999Z').epochMs).toBe(MAX_SUPPORTED_EPOCH_MS);
    expect(instantOf('-100000-01-01T00:00:00Z').epochMs).toBe(MIN_SUPPORTED_EPOCH_MS);
  });
});

// ---------------------------------------------------------------------------
// parseUnixValue
// ---------------------------------------------------------------------------

describe('parseUnixValue: number input', () => {
  it('parses 0 as the epoch (1970-01-01T00:00:00Z)', () => {
    expect(parseUnixValue(0, 'milliseconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: 0, subSecondNanos: 0, fractionDigits: 0 },
    });
    expect(parseUnixValue(0, 'seconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: 0, subSecondNanos: 0, fractionDigits: 0 },
    });
  });

  it('parses negative values as pre-1970 instants', () => {
    expect(parseUnixValue(-1, 'seconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: -1000, subSecondNanos: 0, fractionDigits: 0 },
    });
    expect(unixOf(-1, 'seconds')).toEqual(unixOf(-1000, 'milliseconds'));
  });

  it('converts seconds to milliseconds exactly (BigInt domain)', () => {
    // PRD anchor: 1705316400 s = 2024-01-15T11:00:00Z.
    expect(parseUnixValue(1705316400, 'seconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: 1705316400000, subSecondNanos: 0, fractionDigits: 0 },
    });
  });

  it('rejects numbers that are not safe integers', () => {
    // 2^53 is exactly one past Number.MAX_SAFE_INTEGER.
    for (const bad of [2 ** 53, -(2 ** 53), 1.5, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(parseUnixValue(bad, 'milliseconds')).toEqual({
        kind: 'error',
        reason: { reason: 'not-safe-integer' },
      });
    }
  });
});

describe('parseUnixValue: string input', () => {
  it('accepts integer syntax with explicit sign', () => {
    expect(parseUnixValue('+12', 'milliseconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: 12, subSecondNanos: 0, fractionDigits: 0 },
    });
    expect(parseUnixValue('-0', 'seconds')).toEqual({
      kind: 'ok',
      instant: { epochMs: 0, subSecondNanos: 0, fractionDigits: 0 },
    });
  });

  it('parses the PRD millisecond anchor exactly', () => {
    // 1705316400000 ms = 2024-01-15T11:00:00Z (issue-32 PRD §3.2 preview).
    expect(unixOf('1705316400000', 'milliseconds').epochMs).toBe(1705316400000);
    expect(unixOf(1705316400000, 'milliseconds').epochMs).toBe(1705316400000);
  });

  it('keeps full precision for in-range strings via BigInt', () => {
    // Every in-range epochMs is a safe integer, so the BigInt path is
    // defense-in-depth; the boundary value still round-trips exactly.
    expect(unixOf('3093559603199999', 'milliseconds').epochMs).toBe(MAX_SUPPORTED_EPOCH_MS);
    expect(unixOf('-3217862419200000', 'milliseconds').epochMs).toBe(MIN_SUPPORTED_EPOCH_MS);
  });

  it('rejects non-integer string syntax', () => {
    for (const bad of ['12.5', ' 12', '12 ', '', '1e3', '0x1f', '+ 12', '12,000']) {
      expect(parseUnixValue(bad, 'seconds')).toEqual({
        kind: 'error',
        reason: { reason: 'invalid-integer-syntax' },
      });
    }
  });

  it('rejects values beyond the supported date range', () => {
    expect(parseUnixValue('3162240000000', 'seconds')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
    expect(parseUnixValue('3093559603200000', 'milliseconds')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
    expect(parseUnixValue('-3217862419200001', 'milliseconds')).toEqual({
      kind: 'error',
      reason: { reason: 'out-of-range' },
    });
  });
});

// ---------------------------------------------------------------------------
// parseWallClockText
// ---------------------------------------------------------------------------

describe('parseWallClockText: shapes', () => {
  it('parses date-only text', () => {
    expect(parseWallClockText('2026-03-01')).toEqual({
      kind: 'ok',
      shape: 'date',
      date: { year: 2026, month: 3, day: 1 },
    });
  });

  it('parses datetime text with space, T and t separators', () => {
    const expected = {
      kind: 'ok',
      shape: 'datetime',
      wall: {
        year: 2026,
        month: 3,
        day: 1,
        hour: 0,
        minute: 15,
        second: 30,
        nanos: 123000000,
        fractionDigits: 3,
      },
    };
    expect(parseWallClockText('2026-03-01 00:15:30.123')).toEqual(expected);
    expect(parseWallClockText('2026-03-01T00:15:30.123')).toEqual(expected);
    expect(parseWallClockText('2026-03-01t00:15:30.123')).toEqual(expected);
  });

  it('defaults omitted seconds and fraction to zero', () => {
    expect(parseWallClockText('2026-03-01T00:15')).toEqual({
      kind: 'ok',
      shape: 'datetime',
      wall: {
        year: 2026,
        month: 3,
        day: 1,
        hour: 0,
        minute: 15,
        second: 0,
        nanos: 0,
        fractionDigits: 0,
      },
    });
  });

  it('parses time-only text including minute precision and nanos', () => {
    expect(parseWallClockText('14:30')).toEqual({
      kind: 'ok',
      shape: 'time',
      time: { hour: 14, minute: 30, second: 0, nanos: 0, fractionDigits: 0 },
    });
    expect(parseWallClockText('00:15:30.123456789')).toEqual({
      kind: 'ok',
      shape: 'time',
      time: { hour: 0, minute: 15, second: 30, nanos: 123456789, fractionDigits: 9 },
    });
  });
});

describe('parseWallClockText: no time-zone interpretation', () => {
  it('returns identical components regardless of the machine default zone', () => {
    // The parser must be a pure string→components function: no Date, no Intl,
    // therefore no dependence on process.env.TZ. We pin two very different
    // default zones and require byte-identical results.
    const samples = ['2026-03-01', '2026-03-01 00:15:30.123', '14:30'];

    const inNewYork = withPinnedTz('America/New_York', () => {
      // Premise guard: the pin must actually move the default zone, otherwise
      // this test would prove nothing on a runtime that caches TZ.
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('America/New_York');
      return samples.map((text) => parseWallClockText(text));
    });
    const inShanghai = withPinnedTz('Asia/Shanghai', () => {
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('Asia/Shanghai');
      return samples.map((text) => parseWallClockText(text));
    });

    expect(inNewYork).toEqual(inShanghai);
  });
});

describe('parseWallClockText: invalid inputs', () => {
  it('rejects text carrying a zone designator', () => {
    // Zone-bearing values are parseZonedTimestamp's responsibility; the
    // wall-clock parser must not silently reinterpret them.
    expect(parseWallClockText('2026-03-01T00:15:30Z')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
    expect(parseWallClockText('2026-03-01 00:15:30+08:00')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
    expect(parseWallClockText('14:30+08:00')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
  });

  it('rejects empty and non-timestamp text', () => {
    expect(parseWallClockText('')).toEqual({ kind: 'error', reason: { reason: 'empty-input' } });
    expect(parseWallClockText('not-a-date')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-format' },
    });
  });

  it('rejects invalid calendar components', () => {
    expect(parseWallClockText('2026-13-01')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-month' },
    });
    expect(parseWallClockText('2026-02-30')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-day' },
    });
    expect(parseWallClockText('24:00')).toEqual({
      kind: 'error',
      reason: { reason: 'invalid-hour' },
    });
  });

  it('rejects fractional seconds beyond 9 digits', () => {
    expect(parseWallClockText('2026-03-01T00:15:30.1234567890')).toEqual({
      kind: 'error',
      reason: { reason: 'fraction-digits-out-of-range' },
    });
  });

  it('rejects non-string input defensively', () => {
    expect(parseWallClockText(null as unknown as string)).toEqual({
      kind: 'error',
      reason: { reason: 'not-a-string' },
    });
  });
});

// ---------------------------------------------------------------------------
// wallClockToInstant
// ---------------------------------------------------------------------------

describe('wallClockToInstant: normal conversion', () => {
  it('converts UTC wall time with nanosecond precision', () => {
    // 1969-07-20 20:17:40.123456 UTC (negative epochMs).
    expect(
      wallClockToInstant(
        wall({
          year: 1969,
          month: 7,
          day: 20,
          hour: 20,
          minute: 17,
          second: 40,
          nanos: 123456000,
          fractionDigits: 6,
        }),
        'UTC',
      ),
    ).toEqual({
      ok: true,
      instant: { epochMs: -14182939877, subSecondNanos: 456000, fractionDigits: 6 },
    });
  });

  it('applies the floor/carry semantics for negative sub-second instants', () => {
    // 1969-12-31 23:59:59.765999999 UTC = -234.000001 ms → epochMs floor
    // -235 with a 999999 ns remainder (the documented InstantParts invariant).
    expect(
      wallClockToInstant(
        wall({
          year: 1969,
          month: 12,
          day: 31,
          hour: 23,
          minute: 59,
          second: 59,
          nanos: 765999999,
          fractionDigits: 9,
        }),
        'UTC',
      ),
    ).toEqual({
      ok: true,
      instant: { epochMs: -235, subSecondNanos: 999999, fractionDigits: 9 },
    });
  });

  it('converts Asia/Shanghai wall time across the date line', () => {
    // 2026-01-16 07:30 at +08:00 → 2026-01-15T23:30Z (UTC date rolls back a
    // day). 1768519800000 = Date.UTC(2026, 0, 15, 23, 30) — the exact inverse
    // of the instantToWallParts case below (UTC 23:30 → 07:30 +08:00) and of
    // the iso-tz date-line case in timestampFormat.test.ts. The previous
    // expectation 1768487400000 was the 14:30Z PRD anchor pasted by mistake
    // (no +08:00 relation: 23:30Z − 14:30Z = 9h). fractionDigits stays
    // absent when the wall input carries none: source precision is unknown,
    // so iso output derives minimal digits (0 for zero nanos) at format time.
    expect(
      wallClockToInstant(wall({ month: 1, day: 16, hour: 7, minute: 30 }), 'Asia/Shanghai'),
    ).toEqual({
      ok: true,
      instant: { epochMs: 1768519800000, subSecondNanos: 0, fractionDigits: undefined },
    });
  });

  it('converts a non-whole-minute offset zone (+05:45 Kathmandu)', () => {
    // 22:30 at +05:45 → 16:45Z. fractionDigits stays absent for the same
    // reason as the Shanghai case above (unknown source precision is passed
    // through, not materialized as 0 — see the InstantParts.fractionDigits
    // contract and timestampFormat's resolveFractionDigits).
    expect(wallClockToInstant(wall({ hour: 22, minute: 30 }), 'Asia/Kathmandu')).toEqual({
      ok: true,
      instant: { epochMs: 1768495500000, subSecondNanos: 0, fractionDigits: undefined },
    });
  });

  it('round-trips through instantToWallParts for a negative instant', () => {
    const wallIn: WallComponents = wall({
      year: 1969,
      month: 12,
      day: 31,
      hour: 23,
      minute: 59,
      second: 59,
      nanos: 765999999,
      fractionDigits: 9,
    });
    const toInstant = wallClockToInstant(wallIn, 'UTC');
    expect(toInstant.ok).toBe(true);
    if (!toInstant.ok) return;
    const back = instantToWallParts(toInstant.instant, 'UTC');
    expect(back).toEqual({ ok: true, wall: wallIn, offsetSeconds: 0, offsetMinutes: 0 });
  });
});

describe('wallClockToInstant: DST transitions', () => {
  it('reports spring-forward non-existent wall time as dst-invalid', () => {
    // America/New_York jumps 02:00 → 03:00 on 2026-03-08 (2nd Sunday).
    expect(
      wallClockToInstant(wall({ month: 3, day: 8, hour: 2, minute: 30 }), 'America/New_York'),
    ).toEqual({
      ok: false,
      reason: 'dst-invalid',
    });
  });

  it('accepts wall times just around the spring-forward gap', () => {
    expect(
      wallClockToInstant(wall({ month: 3, day: 8, hour: 1, minute: 30 }), 'America/New_York').ok,
    ).toBe(true);
    expect(
      wallClockToInstant(wall({ month: 3, day: 8, hour: 3, minute: 30 }), 'America/New_York').ok,
    ).toBe(true);
  });

  it('reports fall-back repeated wall time as dst-ambiguous', () => {
    // America/New_York repeats 01:00–02:00 on 2026-11-01 (1st Sunday).
    const result = wallClockToInstant(
      wall({ month: 11, day: 1, hour: 1, minute: 30, second: 0, nanos: 500000000 }),
      'America/New_York',
    );
    expect(result).toEqual({ ok: false, reason: 'dst-ambiguous' });
  });

  it('accepts wall times just around the fall-back overlap', () => {
    expect(
      wallClockToInstant(wall({ month: 11, day: 1, hour: 0, minute: 30 }), 'America/New_York').ok,
    ).toBe(true);
    expect(
      wallClockToInstant(wall({ month: 11, day: 1, hour: 2, minute: 30 }), 'America/New_York').ok,
    ).toBe(true);
  });

  it('does not flag DST in zones without transitions', () => {
    expect(
      wallClockToInstant(wall({ month: 3, day: 8, hour: 2, minute: 30 }), 'Asia/Shanghai').ok,
    ).toBe(true);
  });
});

describe('wallClockToInstant: error paths', () => {
  it('rejects invalid IANA time-zone names', () => {
    expect(wallClockToInstant(wall({}), 'Mars/Olympus')).toEqual({
      ok: false,
      reason: 'invalid-time-zone',
    });
    expect(wallClockToInstant(wall({}), '')).toEqual({ ok: false, reason: 'invalid-time-zone' });
  });

  it('rejects invalid wall components', () => {
    const bad: ReadonlyArray<Partial<WallComponents>> = [
      { month: 13 },
      { day: 32 },
      { hour: 24 },
      { minute: 60 },
      { second: 60 },
      { nanos: -1 },
      { nanos: 1_000_000_000 },
      { nanos: 0.5 },
      { fractionDigits: 10 },
    ];
    for (const overrides of bad) {
      expect(wallClockToInstant(wall(overrides), 'UTC')).toEqual({
        ok: false,
        reason: 'invalid-components',
      });
    }
  });

  it('rejects non-object wall input defensively', () => {
    expect(wallClockToInstant(null as unknown as WallComponents, 'UTC')).toEqual({
      ok: false,
      reason: 'invalid-components',
    });
    expect(wallClockToInstant('2026-01-15' as unknown as WallComponents, 'UTC')).toEqual({
      ok: false,
      reason: 'invalid-components',
    });
  });

  it('rejects years beyond the supported range', () => {
    expect(wallClockToInstant(wall({ year: 100001 }), 'UTC')).toEqual({
      ok: false,
      reason: 'out-of-range',
    });
    expect(wallClockToInstant(wall({ year: -100001 }), 'UTC')).toEqual({
      ok: false,
      reason: 'out-of-range',
    });
  });
});

// ---------------------------------------------------------------------------
// instantToWallParts
// ---------------------------------------------------------------------------

describe('instantToWallParts', () => {
  it('converts UTC 23:30 to next-day 07:30 in Asia/Shanghai', () => {
    const result = instantToWallParts(
      { epochMs: 1768519800000, subSecondNanos: 0 },
      'Asia/Shanghai',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ month: 1, day: 16, hour: 7, minute: 30 }),
      offsetSeconds: 28800,
      offsetMinutes: 480,
    });
  });

  it('handles the non-whole-minute offset of Asia/Kathmandu (+05:45)', () => {
    const result = instantToWallParts(
      { epochMs: 1768487400000, subSecondNanos: 0 },
      'Asia/Kathmandu',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ hour: 20, minute: 15 }),
      offsetSeconds: 20700,
      offsetMinutes: 345,
    });
  });

  it('handles negative whole-hour offsets (-05:00 New York in January)', () => {
    const result = instantToWallParts(
      { epochMs: 1768487400000, subSecondNanos: 0 },
      'America/New_York',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ hour: 9, minute: 30 }),
      offsetSeconds: -18000,
      offsetMinutes: -300,
    });
  });

  it('exposes historical sub-minute LMT offsets precisely', () => {
    // Asia/Shanghai before 1901 used LMT +08:05:43 (29143 s); the
    // offsetMinutes convenience field truncates to 485.
    const result = instantToWallParts(
      { epochMs: -3785572800000, subSecondNanos: 0 },
      'Asia/Shanghai',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ year: 1850, month: 1, day: 15, hour: 20, minute: 5, second: 43 }),
      offsetSeconds: 29143,
      offsetMinutes: 485,
    });
  });

  it('maps BC-era readback to negative astronomical years', () => {
    // -0050-01-15T12:00:00Z in New York (LMT -04:56:02): Intl reports
    // "51 BC 07:03:58", which the module converts to ISO year -50.
    const result = instantToWallParts(
      { epochMs: -63743803200000, subSecondNanos: 0 },
      'America/New_York',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ year: -50, month: 1, day: 15, hour: 7, minute: 3, second: 58 }),
      offsetSeconds: -17762,
      offsetMinutes: -296,
    });
  });

  it('maps the year-0 boundary to 2 BC (= astronomical year -1)', () => {
    const result = instantToWallParts(
      { epochMs: -62167219200000, subSecondNanos: 0 },
      'America/New_York',
    );
    expect(result).toEqual({
      ok: true,
      wall: wall({ year: -1, month: 12, day: 31, hour: 19, minute: 3, second: 58 }),
      offsetSeconds: -17762,
      offsetMinutes: -296,
    });
  });

  it('carries subSecondNanos into wall nanos (invariant: no borrow)', () => {
    const result = instantToWallParts(
      { epochMs: 1768487400123, subSecondNanos: 456789, fractionDigits: 9 },
      'Asia/Shanghai',
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.wall.nanos).toBe(123456789);
    expect(result.wall.fractionDigits).toBe(9);
  });

  it('keeps civil-millisecond carry for negative epochMs (floor invariant)', () => {
    // epochMs -235 (civil 765 ms) + 999999 ns → 23:59:59.765999999.
    const result = instantToWallParts({ epochMs: -235, subSecondNanos: 999999 }, 'UTC');
    expect(result).toEqual({
      ok: true,
      wall: wall({
        year: 1969,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 59,
        nanos: 765999999,
      }),
      offsetSeconds: 0,
      offsetMinutes: 0,
    });
  });

  it('keeps offsetMinutes consistent with offsetSeconds across zones', () => {
    const probes: ReadonlyArray<{ epochMs: number; zone: string }> = [
      { epochMs: 1768487400000, zone: 'Asia/Shanghai' },
      { epochMs: 1768487400000, zone: 'Asia/Kathmandu' },
      { epochMs: 1768487400000, zone: 'America/New_York' },
      { epochMs: 1772951400000, zone: 'America/New_York' }, // 2026-03-08T06:30Z, EST side
      { epochMs: 1772955000000, zone: 'America/New_York' }, // 2026-03-08T07:30Z, EDT side
      { epochMs: -3785572800000, zone: 'Asia/Shanghai' }, // 1850 LMT
    ];
    for (const { epochMs, zone } of probes) {
      const result = instantToWallParts({ epochMs, subSecondNanos: 0 }, zone);
      expect(result.ok).toBe(true);
      if (!result.ok) continue;
      expect(result.offsetMinutes).toBe(Math.trunc(result.offsetSeconds / 60));
    }
  });

  it('rejects invalid zones, instants and out-of-range epoch values', () => {
    expect(instantToWallParts({ epochMs: 0, subSecondNanos: 0 }, 'Mars/Olympus')).toEqual({
      ok: false,
      reason: 'invalid-time-zone',
    });
    expect(
      instantToWallParts({ epochMs: MAX_SUPPORTED_EPOCH_MS + 1, subSecondNanos: 0 }, 'UTC'),
    ).toEqual({
      ok: false,
      reason: 'out-of-range',
    });
    expect(instantToWallParts({ epochMs: 1.5, subSecondNanos: 0 }, 'UTC')).toEqual({
      ok: false,
      reason: 'invalid-instant',
    });
    expect(instantToWallParts({ epochMs: 0, subSecondNanos: 1_000_000 }, 'UTC')).toEqual({
      ok: false,
      reason: 'invalid-instant',
    });
    expect(instantToWallParts(null as unknown as InstantParts, 'UTC')).toEqual({
      ok: false,
      reason: 'invalid-instant',
    });
  });
});

// ---------------------------------------------------------------------------
// InstantParts composition math and total order
// ---------------------------------------------------------------------------

describe('InstantParts composition math', () => {
  it('represents microsecond and nanosecond instants losslessly', () => {
    // Parse → format-free structural round-trip: the same text parses to the
    // same parts and reconstructs the same wall nanos.
    const micro = instantOf('2026-01-15T14:30:00.123456Z');
    expect(micro).toEqual({ epochMs: 1768487400123, subSecondNanos: 456000, fractionDigits: 6 });
    const nano = instantOf('2026-01-15T14:30:00.123456789Z');
    expect(nano).toEqual({ epochMs: 1768487400123, subSecondNanos: 456789, fractionDigits: 9 });
  });

  it('satisfies epochMs = floor(instant) with a non-negative remainder', () => {
    // -1234.000001 ms must decompose as epochMs -1235 + 999999 ns, never as
    // -1234 with a negative remainder or -1234.000001 rounded to -1234.
    const parts = instantOf('1969-12-31T23:59:59.999999999Z');
    expect(parts).toEqual({ epochMs: -1, subSecondNanos: 999999, fractionDigits: 9 });
    // Manual equivalent of the documented example (-1235 ms + 999999 ns):
    const manual: InstantParts = { epochMs: -1235, subSecondNanos: 999999 };
    expect(validateInstantParts(manual)).toBeNull();
    // Instant = -1234.000001 ms, which is strictly before -1234 ms:
    expect(compareInstants(manual, { epochMs: -1234, subSecondNanos: 0 })).toBe(-1);
  });
});

describe('compareInstants', () => {
  it('returns 0 for identical instants regardless of fraction digits', () => {
    expect(
      compareInstants(
        { epochMs: 1768487400000, subSecondNanos: 0 },
        { epochMs: 1768487400000, subSecondNanos: 0 },
      ),
    ).toBe(0);
    // .123 and .123000 denote the same instant with different source precision.
    expect(
      compareInstants(
        instantOf('2026-01-15T14:30:00.123Z'),
        instantOf('2026-01-15T14:30:00.123000Z'),
      ),
    ).toBe(0);
  });

  it('orders by epochMs first, then by subSecondNanos', () => {
    const a = { epochMs: 1000, subSecondNanos: 0 };
    const b = { epochMs: 1000, subSecondNanos: 1 };
    const c = { epochMs: 1001, subSecondNanos: 0 };
    expect(compareInstants(a, b)).toBe(-1);
    expect(compareInstants(b, a)).toBe(1);
    expect(compareInstants(b, c)).toBe(-1);
    expect(compareInstants(c, b)).toBe(1);
  });

  it('orders correctly across the negative floor boundary', () => {
    // -1234.000001 < -1234 < -1233.999999 … and -1.999999 ms < -1 ms.
    expect(
      compareInstants(
        { epochMs: -1235, subSecondNanos: 999999 },
        { epochMs: -1234, subSecondNanos: 0 },
      ),
    ).toBe(-1);
    expect(
      compareInstants({ epochMs: -1234, subSecondNanos: 0 }, { epochMs: -1234, subSecondNanos: 1 }),
    ).toBe(-1);
    expect(
      compareInstants({ epochMs: -2, subSecondNanos: 999999 }, { epochMs: -1, subSecondNanos: 0 }),
    ).toBe(-1);
  });

  it('is a strict total order over a scrambled set', () => {
    const instants: InstantParts[] = [
      { epochMs: 1768487400123, subSecondNanos: 456789 },
      { epochMs: -235, subSecondNanos: 999999 },
      { epochMs: 0, subSecondNanos: 0 },
      { epochMs: -1, subSecondNanos: 999999 },
      { epochMs: 1768487400123, subSecondNanos: 456788 },
      { epochMs: MAX_SUPPORTED_EPOCH_MS, subSecondNanos: 0 },
      { epochMs: MIN_SUPPORTED_EPOCH_MS, subSecondNanos: 0 },
      { epochMs: 1768487400122, subSecondNanos: 999999 },
    ];
    const sorted = [...instants].sort(compareInstants);
    const reference = [...instants].sort((x, y) => {
      const kx = instantKey(x);
      const ky = instantKey(y);
      return kx < ky ? -1 : kx > ky ? 1 : 0;
    });
    expect(sorted).toEqual(reference);
    // Antisymmetry spot-check on neighbours.
    for (let i = 1; i < sorted.length; i += 1) {
      expect(compareInstants(sorted[i - 1], sorted[i])).toBeLessThan(0);
      expect(compareInstants(sorted[i], sorted[i - 1])).toBeGreaterThan(0);
    }
  });
});

// ---------------------------------------------------------------------------
// validateInstantParts
// ---------------------------------------------------------------------------

describe('validateInstantParts', () => {
  it('accepts boundary and normal values', () => {
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 0 })).toBeNull();
    expect(validateInstantParts({ epochMs: MIN_SUPPORTED_EPOCH_MS, subSecondNanos: 0 })).toBeNull();
    expect(
      validateInstantParts({ epochMs: MAX_SUPPORTED_EPOCH_MS, subSecondNanos: 999999 }),
    ).toBeNull();
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 0, fractionDigits: 9 })).toBeNull();
  });

  it('rejects non-integer or out-of-domain components', () => {
    expect(validateInstantParts({ epochMs: 1.5, subSecondNanos: 0 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: Number.NaN, subSecondNanos: 0 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: -1 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 1_000_000 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 0.5 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 0, fractionDigits: 10 })).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts({ epochMs: 0, subSecondNanos: 0, fractionDigits: -1 })).toEqual({
      reason: 'invalid-instant',
    });
  });

  it('rejects non-object instants and out-of-range epochs', () => {
    expect(validateInstantParts(null as unknown as InstantParts)).toEqual({
      reason: 'invalid-instant',
    });
    expect(validateInstantParts('x' as unknown as InstantParts)).toEqual({
      reason: 'invalid-instant',
    });
    expect(
      validateInstantParts({ epochMs: MAX_SUPPORTED_EPOCH_MS + 1, subSecondNanos: 0 }),
    ).toEqual({
      reason: 'out-of-range',
    });
    expect(
      validateInstantParts({ epochMs: MIN_SUPPORTED_EPOCH_MS - 1, subSecondNanos: 0 }),
    ).toEqual({
      reason: 'out-of-range',
    });
  });
});

// ---------------------------------------------------------------------------
// Offset text helpers
// ---------------------------------------------------------------------------

describe('formatOffsetMinutes', () => {
  it('formats whole-minute offsets', () => {
    expect(formatOffsetMinutes(480)).toBe('+08:00');
    expect(formatOffsetMinutes(-300)).toBe('-05:00');
    expect(formatOffsetMinutes(345)).toBe('+05:45');
  });

  it('formats zero as +00:00, not Z (locked source behavior)', () => {
    // The helper has no Z special case; only the `iso` format emits Z.
    expect(formatOffsetMinutes(0)).toBe('+00:00');
  });

  it('truncates non-integer minutes toward zero (documented, no rounding)', () => {
    expect(formatOffsetMinutes(345.9)).toBe('+05:45');
    // Math.trunc(-0.5) === -0 and -0 < 0 is false, so -0.5 renders as +00:00.
    expect(formatOffsetMinutes(-0.5)).toBe('+00:00');
  });
});

describe('formatOffsetSeconds', () => {
  it('formats whole-minute offsets as ±HH:MM', () => {
    expect(formatOffsetSeconds(0)).toBe('+00:00');
    expect(formatOffsetSeconds(28800)).toBe('+08:00');
    expect(formatOffsetSeconds(-18000)).toBe('-05:00');
    expect(formatOffsetSeconds(20700)).toBe('+05:45');
  });

  it('formats sub-minute historical offsets as ±HH:MM:SS', () => {
    expect(formatOffsetSeconds(29143)).toBe('+08:05:43');
    expect(formatOffsetSeconds(-17762)).toBe('-04:56:02');
  });
});

// ---------------------------------------------------------------------------
// Time-zone name validation
// ---------------------------------------------------------------------------

describe('isValidTimeZoneName', () => {
  it('accepts real IANA names', () => {
    for (const zone of [
      'UTC',
      'GMT',
      'Asia/Shanghai',
      'America/New_York',
      'Asia/Kolkata',
      'Etc/GMT+8',
      'Etc/GMT-14',
    ]) {
      expect(isValidTimeZoneName(zone)).toBe(true);
    }
  });

  it('is case-insensitive because Intl canonicalizes (locked Node behavior)', () => {
    // Node/ICU accepts and canonicalizes lowercase zone IDs. Comment: this is
    // ICU behavior; a stricter engine would flip this expectation.
    expect(isValidTimeZoneName('asia/shanghai')).toBe(true);
  });

  it('rejects empty, unknown and whitespace-padded names', () => {
    expect(isValidTimeZoneName('')).toBe(false);
    expect(isValidTimeZoneName('Mars/Olympus')).toBe(false);
    expect(isValidTimeZoneName('Asia/Shanghai ')).toBe(false);
  });

  it('rejects non-IANA character sets', () => {
    expect(isValidTimeZoneName('Asia/上海')).toBe(false);
    expect(isValidTimeZoneName(42 as unknown as string)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Support-range constants
// ---------------------------------------------------------------------------

describe('support-range constants', () => {
  it('covers exactly [-100000-01-01, 100000-12-31] UTC', () => {
    expect(MIN_SUPPORTED_EPOCH_MS).toBe(Date.UTC(-100000, 0, 1));
    expect(MAX_SUPPORTED_EPOCH_MS).toBe(Date.UTC(100001, 0, 1) - 1);
  });
});
