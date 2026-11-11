import { describe, expect, it } from 'vitest';
import {
  type InstantParts,
  type TimeZoneSpec,
  type TimestampFormatOptions,
  type TimestampFormatResult,
  type TimestampOutputFormat,
  formatTimestampOutput,
  isValidIanaTimeZoneName,
  parseUnixValue,
  parseZonedTimestamp,
  resolveTimeZoneName,
} from '../index';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function instantOf(text: string): InstantParts {
  const result = parseZonedTimestamp(text);
  if (result.kind !== 'ok') {
    throw new Error(`parseZonedTimestamp(${text}) failed: ${JSON.stringify(result.reason)}`);
  }
  return result.instant;
}

function unixOf(value: number | string, unit: 'seconds' | 'milliseconds'): InstantParts {
  const result = parseUnixValue(value, unit);
  if (result.kind !== 'ok') {
    throw new Error(`parseUnixValue(${value}, ${unit}) failed: ${JSON.stringify(result.reason)}`);
  }
  return result.instant;
}

function fmt(
  instant: InstantParts,
  format: TimestampOutputFormat,
  displayTimeZone: TimeZoneSpec,
  locale = 'en-US',
): TimestampFormatResult {
  return formatTimestampOutput(instant, { format, displayTimeZone, locale });
}

function textOf(result: TimestampFormatResult): string {
  if (!result.ok) throw new Error(`formatting failed: ${JSON.stringify(result.reason)}`);
  return result.text;
}

/**
 * Machine-default-timezone control.
 *
 * `process.env.TZ` was read at module load (ORIGINAL_TZ; in this sandbox it is
 * unset, so Node falls back to the system zone — UTC here — but the tests must
 * not depend on that). The `local` spec resolves against the runner's default
 * zone, so every `local` test below PINS process.env.TZ to a known IANA zone,
 * derives the expectation from Date's local accessors (a different code path
 * than the Intl-based implementation under test), and restores the original
 * value afterwards.
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

/** Expected iso-tz text for the pinned local zone, computed via Date local accessors. */
function expectedLocalIsoTz(epochMs: number): string {
  const date = new Date(epochMs);
  const offsetMinutes = -date.getTimezoneOffset();
  const sign = offsetMinutes < 0 ? '-' : '+';
  const abs = Math.abs(offsetMinutes);
  const pad2 = (value: number) => String(value).padStart(2, '0');
  return (
    `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}` +
    `T${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}` +
    `${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`
  );
}

const SHANGHAI: TimeZoneSpec = { kind: 'iana', name: 'Asia/Shanghai' };
const NEW_YORK: TimeZoneSpec = { kind: 'iana', name: 'America/New_York' };
const KATHMANDU: TimeZoneSpec = { kind: 'iana', name: 'Asia/Kathmandu' };
const UTC_SPEC: TimeZoneSpec = { kind: 'utc' };

// ---------------------------------------------------------------------------
// iso — UTC output with source precision
// ---------------------------------------------------------------------------

describe('formatTimestampOutput: iso', () => {
  it('emits UTC ISO with Z and no invented fraction', () => {
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00Z'), 'iso', SHANGHAI))).toBe(
      '2026-01-15T14:30:00Z',
    );
    // The display time zone must not influence iso output.
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00Z'), 'iso', NEW_YORK))).toBe(
      '2026-01-15T14:30:00Z',
    );
    expect(textOf(fmt(unixOf(0, 'milliseconds'), 'iso', UTC_SPEC))).toBe('1970-01-01T00:00:00Z');
  });

  it('preserves the source fraction digit count (no padding, no truncation)', () => {
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.123Z'), 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.123Z',
    );
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.100Z'), 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.100Z',
    );
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.123456Z'), 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.123456Z',
    );
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.123456789Z'), 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.123456789Z',
    );
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.000000000Z'), 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.000000000Z',
    );
  });

  it('derives minimal exact digits when fractionDigits is absent', () => {
    // 123000 ns of a second → six digits are needed (…000123), three are not.
    expect(textOf(fmt({ epochMs: 1768487400000, subSecondNanos: 123000 }, 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.000123Z',
    );
    expect(textOf(fmt({ epochMs: 1768487400000, subSecondNanos: 1 }, 'iso', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00.000000001Z',
    );
  });

  it('round-trips negative and extended-year instants', () => {
    expect(textOf(fmt(instantOf('1969-12-31T23:59:59.123456Z'), 'iso', UTC_SPEC))).toBe(
      '1969-12-31T23:59:59.123456Z',
    );
    expect(textOf(fmt(instantOf('0050-01-01T00:00:00Z'), 'iso', UTC_SPEC))).toBe(
      '0050-01-01T00:00:00Z',
    );
    expect(textOf(fmt(instantOf('0000-01-01T00:00:00Z'), 'iso', UTC_SPEC))).toBe(
      '0000-01-01T00:00:00Z',
    );
    expect(textOf(fmt(instantOf('-0050-01-01T00:00:00Z'), 'iso', UTC_SPEC))).toBe(
      '-0050-01-01T00:00:00Z',
    );
    expect(textOf(fmt(instantOf('+10000-01-01T00:00:00Z'), 'iso', UTC_SPEC))).toBe(
      '+10000-01-01T00:00:00Z',
    );
  });

  it('formats Unix seconds and milliseconds as UTC instants', () => {
    expect(textOf(fmt(unixOf(1705316400, 'seconds'), 'iso', UTC_SPEC))).toBe(
      '2024-01-15T11:00:00Z',
    );
    expect(textOf(fmt(unixOf(-1, 'seconds'), 'iso', UTC_SPEC))).toBe('1969-12-31T23:59:59Z');
  });
});

// ---------------------------------------------------------------------------
// iso-tz — target-zone wall time with the actual offset
// ---------------------------------------------------------------------------

describe('formatTimestampOutput: iso-tz', () => {
  it('converts the PRD anchor to UTC+8 (2026-01-15T22:30:00+08:00)', () => {
    // issue-32 PRD §4 correction example.
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00Z'), 'iso-tz', SHANGHAI))).toBe(
      '2026-01-15T22:30:00+08:00',
    );
  });

  it('renders the PRD Unix-millisecond anchor in Asia/Shanghai (2024-01-15T19:00:00+08:00)', () => {
    // issue-32 PRD §3.2 preview example; string and number inputs agree.
    expect(textOf(fmt(unixOf('1705316400000', 'milliseconds'), 'iso-tz', SHANGHAI))).toBe(
      '2024-01-15T19:00:00+08:00',
    );
    expect(textOf(fmt(unixOf(1705316400000, 'milliseconds'), 'iso-tz', SHANGHAI))).toBe(
      '2024-01-15T19:00:00+08:00',
    );
  });

  it('spells the offset explicitly for the utc spec (+00:00, not Z)', () => {
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00Z'), 'iso-tz', UTC_SPEC))).toBe(
      '2026-01-15T14:30:00+00:00',
    );
  });

  it('crosses the date line when the target day rolls over', () => {
    expect(textOf(fmt(instantOf('2026-01-15T23:30:00Z'), 'iso-tz', SHANGHAI))).toBe(
      '2026-01-16T07:30:00+08:00',
    );
  });

  it('emits negative offsets', () => {
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00Z'), 'iso-tz', NEW_YORK))).toBe(
      '2026-01-15T09:30:00-05:00',
    );
  });

  it('preserves full nanosecond precision through the conversion', () => {
    expect(textOf(fmt(instantOf('2026-01-15T14:30:00.123456789Z'), 'iso-tz', KATHMANDU))).toBe(
      '2026-01-15T20:15:00.123456789+05:45',
    );
  });

  it('uses the offset actually in effect on a DST switch day', () => {
    // 2026-03-08: New York springs forward at 07:00Z (02:00→03:00 local).
    expect(textOf(fmt(instantOf('2026-03-08T06:30:00Z'), 'iso-tz', NEW_YORK))).toBe(
      '2026-03-08T01:30:00-05:00',
    );
    expect(textOf(fmt(instantOf('2026-03-08T07:30:00Z'), 'iso-tz', NEW_YORK))).toBe(
      '2026-03-08T03:30:00-04:00',
    );
  });

  it('emits ±HH:MM:SS for historical sub-minute offsets', () => {
    // 1850 Asia/Shanghai LMT +08:05:43.
    expect(textOf(fmt(instantOf('1850-01-15T12:00:00Z'), 'iso-tz', SHANGHAI))).toBe(
      '1850-01-15T20:05:43+08:05:43',
    );
    // -0050 America/New_York LMT -04:56:02.
    expect(textOf(fmt(instantOf('-0050-01-15T12:00:00Z'), 'iso-tz', NEW_YORK))).toBe(
      '-0050-01-15T07:03:58-04:56:02',
    );
  });

  it('resolves the local spec against the pinned default zone', () => {
    // See the withPinnedTz comment: expectations are computed from Date's
    // local accessors so the test holds on any machine.
    const anchor = instantOf('2026-01-15T14:30:00Z');
    withPinnedTz('America/New_York', () => {
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('America/New_York');
      expect(textOf(fmt(anchor, 'iso-tz', { kind: 'local' }))).toBe(
        expectedLocalIsoTz(anchor.epochMs),
      );
      expect(textOf(fmt(anchor, 'iso-tz', { kind: 'local' }))).toBe('2026-01-15T09:30:00-05:00');
    });
    withPinnedTz('Asia/Shanghai', () => {
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('Asia/Shanghai');
      expect(textOf(fmt(anchor, 'iso-tz', { kind: 'local' }))).toBe(
        expectedLocalIsoTz(anchor.epochMs),
      );
      expect(textOf(fmt(anchor, 'iso-tz', { kind: 'local' }))).toBe('2026-01-15T22:30:00+08:00');
    });
  });
});

// ---------------------------------------------------------------------------
// locale — explicit locale + time zone
// ---------------------------------------------------------------------------

describe('formatTimestampOutput: locale', () => {
  // Locale assertions below check structural components (wall time digits and
  // the GMT offset marker) rather than full strings: exact punctuation is
  // CLDR data and can differ between ICU versions (verified against Node 24
  // full-ICU: en-US → "01/15/2026, 10:30:00 PM GMT+8").

  it('honors an explicit locale and time zone', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    const enUs = textOf(fmt(anchor, 'locale', SHANGHAI, 'en-US'));
    expect(enUs).toContain('10:30:00 PM');
    expect(enUs).toContain('GMT+8');

    const deDe = textOf(fmt(anchor, 'locale', SHANGHAI, 'de-DE'));
    expect(deDe).toContain('22:30:00');
    expect(deDe).not.toContain('PM');
    expect(deDe).toContain('GMT+8');

    const zhCn = textOf(fmt(anchor, 'locale', SHANGHAI, 'zh-CN'));
    expect(zhCn).toContain('22:30:00');
  });

  it('reflects the target time zone in the wall time and offset marker', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    const newYork = textOf(fmt(anchor, 'locale', NEW_YORK, 'en-US'));
    expect(newYork).toContain('09:30:00 AM');
    expect(newYork).toContain('GMT-5');
  });

  it('truncates sub-millisecond precision to 3 digits (documented decision)', () => {
    // timestampFormat header decision: Intl caps fractionalSecondDigits at 3,
    // so micro/nano sources are truncated toward the past instead of failing.
    const nano = instantOf('2026-01-15T14:30:00.123456789Z');
    const enUs = textOf(fmt(nano, 'locale', SHANGHAI, 'en-US'));
    expect(enUs).toMatch(/10:30:00\.123\b/);
    expect(enUs).not.toContain('.1234');

    const deDe = textOf(fmt(nano, 'locale', SHANGHAI, 'de-DE'));
    expect(deDe).toMatch(/22:30:00,123\b/);
  });

  it('appends no fraction when the source has none', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    expect(textOf(fmt(anchor, 'locale', SHANGHAI, 'en-US'))).not.toMatch(/30:00[.,]\d/);
  });

  it('renders a zero millisecond part when only sub-millisecond nanos exist', () => {
    // epochMs on a whole second + 500000 ns: wall nanos > 0 → the fraction
    // option is on, but Date only carries the (zero) millisecond part, so the
    // output shows .000 — truncation toward the past of 0.5 ms.
    const instant: InstantParts = { epochMs: 1768487400000, subSecondNanos: 500000 };
    expect(textOf(fmt(instant, 'locale', SHANGHAI, 'en-US'))).toMatch(/10:30:00\.000\b/);
  });

  it('resolves the local spec for locale output against the pinned zone', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    withPinnedTz('Asia/Shanghai', () => {
      expect(new Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('Asia/Shanghai');
      expect(textOf(fmt(anchor, 'locale', { kind: 'local' }, 'en-US'))).toContain('10:30:00 PM');
    });
  });

  it('rejects empty and non-string locales', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    expect(fmt(anchor, 'locale', SHANGHAI, '')).toEqual({
      ok: false,
      reason: { reason: 'invalid-locale' },
    });
    expect(fmt(anchor, 'locale', SHANGHAI, null as unknown as string)).toEqual({
      ok: false,
      reason: { reason: 'invalid-locale' },
    });
  });

  it('falls back to the default language for data-less but well-formed tags', () => {
    // Documented boundary: 'zzz' is structurally valid, so Intl falls back to
    // its default language rather than failing (no invented error).
    const anchor = instantOf('2026-01-15T14:30:00Z');
    const result = fmt(anchor, 'locale', SHANGHAI, 'zzz');
    expect(result.ok).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

describe('formatTimestampOutput: error paths', () => {
  it('fails every format for an invalid IANA display zone (validated up front)', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    const bogus: TimeZoneSpec = { kind: 'iana', name: 'Mars/Olympus' };
    for (const format of ['iso', 'iso-tz', 'locale'] as const) {
      expect(fmt(anchor, format, bogus)).toEqual({
        ok: false,
        reason: { reason: 'invalid-time-zone' },
      });
    }
  });

  it('fails for a zone name with whitespace (format-level rejection)', () => {
    const anchor = instantOf('2026-01-15T14:30:00Z');
    const padded: TimeZoneSpec = { kind: 'iana', name: 'Asia/Shanghai ' };
    expect(fmt(anchor, 'iso-tz', padded)).toEqual({
      ok: false,
      reason: { reason: 'invalid-time-zone' },
    });
  });

  it('validates the instant before formatting', () => {
    expect(fmt({ epochMs: 0, subSecondNanos: 1_000_000 }, 'iso', UTC_SPEC)).toEqual({
      ok: false,
      reason: { reason: 'invalid-instant' },
    });
    expect(fmt({ epochMs: 3093559603200000, subSecondNanos: 0 }, 'iso', UTC_SPEC)).toEqual({
      ok: false,
      reason: { reason: 'out-of-range' },
    });
  });

  it('returns invalid-locale for an unrecognized format value (locked quirk)', () => {
    // Runtime-garbage defense: the default branch of the format switch reports
    // 'invalid-locale'. Semantically odd for a bad *format*, but it is the
    // actual source behavior and unreachable through validated options.
    const garbage = {
      format: 'bogus',
      displayTimeZone: { kind: 'utc' },
      locale: 'en-US',
    } as unknown as TimestampFormatOptions;
    expect(formatTimestampOutput(instantOf('2026-01-15T14:30:00Z'), garbage)).toEqual({
      ok: false,
      reason: { reason: 'invalid-locale' },
    });
  });
});

// ---------------------------------------------------------------------------
// resolveTimeZoneName
// ---------------------------------------------------------------------------

describe('resolveTimeZoneName', () => {
  it('maps utc to the fixed name UTC', () => {
    expect(resolveTimeZoneName({ kind: 'utc' })).toEqual({ ok: true, name: 'UTC' });
  });

  it('maps local to the runner default zone', () => {
    withPinnedTz('America/New_York', () => {
      expect(resolveTimeZoneName({ kind: 'local' })).toEqual({
        ok: true,
        name: 'America/New_York',
      });
    });
    withPinnedTz('Asia/Shanghai', () => {
      expect(resolveTimeZoneName({ kind: 'local' })).toEqual({ ok: true, name: 'Asia/Shanghai' });
    });
  });

  it('passes syntactically valid iana names through', () => {
    expect(resolveTimeZoneName({ kind: 'iana', name: 'Asia/Shanghai' })).toEqual({
      ok: true,
      name: 'Asia/Shanghai',
    });
    // Format-only stage: a syntactically valid but non-existent zone still
    // resolves here; full validation happens at format time.
    expect(resolveTimeZoneName({ kind: 'iana', name: 'Mars/Olympus' })).toEqual({
      ok: true,
      name: 'Mars/Olympus',
    });
  });

  it('rejects malformed iana names and non-spec inputs', () => {
    expect(resolveTimeZoneName({ kind: 'iana', name: '' })).toEqual({
      ok: false,
      reason: { reason: 'invalid-time-zone' },
    });
    expect(resolveTimeZoneName({ kind: 'iana', name: 'Foo Bar' })).toEqual({
      ok: false,
      reason: { reason: 'invalid-time-zone' },
    });
    expect(resolveTimeZoneName(null as unknown as TimeZoneSpec)).toEqual({
      ok: false,
      reason: { reason: 'invalid-time-zone' },
    });
    expect(resolveTimeZoneName({ kind: 'mars' } as unknown as TimeZoneSpec)).toEqual({
      ok: false,
      reason: { reason: 'invalid-time-zone' },
    });
  });
});

// ---------------------------------------------------------------------------
// IANA name format validation
// ---------------------------------------------------------------------------

describe('isValidIanaTimeZoneName', () => {
  it('accepts names within the documented character set and length', () => {
    for (const name of [
      'UTC',
      'GMT',
      'Asia/Shanghai',
      'America/New_York',
      'Etc/GMT+8',
      'Etc/GMT-14',
      'Asia/Kolkata',
    ]) {
      expect(isValidIanaTimeZoneName(name)).toBe(true);
    }
  });

  it('rejects empty, whitespace, non-ASCII and oversized names', () => {
    expect(isValidIanaTimeZoneName('')).toBe(false);
    expect(isValidIanaTimeZoneName('Asia/Shanghai ')).toBe(false);
    expect(isValidIanaTimeZoneName('Asia/上海')).toBe(false);
    expect(isValidIanaTimeZoneName('Asia\nShanghai')).toBe(false);
    expect(isValidIanaTimeZoneName('A'.repeat(101))).toBe(false);
    // Lowercase passes the format layer (full validity is delegated to Intl).
    expect(isValidIanaTimeZoneName('asia/shanghai')).toBe(true);
  });
});
