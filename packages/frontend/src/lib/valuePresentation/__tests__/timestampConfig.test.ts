import { describe, expect, it } from 'vitest';
import { type TimestampOptions, validateTimestampOptions } from '../timestampConfig';

// ---------------------------------------------------------------------------
// Valid shapes
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: valid shapes', () => {
  it('accepts a minimal native configuration', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
    });
    expect(result).toEqual({
      ok: true,
      options: {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
    // sourceTimeZone must not appear as a key when absent (stable JSON).
    expect(result.ok && 'sourceTimeZone' in result.options).toBe(false);
  });

  it('accepts unix seconds and milliseconds', () => {
    expect(
      validateTimestampOptions({
        interpretation: { kind: 'unix', unit: 'seconds' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      }),
    ).toEqual({
      ok: true,
      options: {
        interpretation: { kind: 'unix', unit: 'seconds' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
    });
    expect(
      validateTimestampOptions({
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'locale',
        displayTimeZone: { kind: 'local' },
      }).ok,
    ).toBe(true);
  });

  it('accepts text interpretation with a source time zone', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'text' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      sourceTimeZone: { kind: 'iana', name: 'America/New_York' },
    });
    expect(result).toEqual({
      ok: true,
      options: {
        interpretation: { kind: 'text' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        sourceTimeZone: { kind: 'iana', name: 'America/New_York' },
      },
    });
  });

  it('accepts a source time zone for native interpretation', () => {
    // native local-datetime values may carry a source zone (design §5).
    expect(
      validateTimestampOptions({
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
        sourceTimeZone: { kind: 'local' },
      }).ok,
    ).toBe(true);
  });

  it('treats an explicitly undefined sourceTimeZone as absent', () => {
    // JSON round-trips drop the key; an explicit undefined must behave the
    // same way instead of tripping the unknown-field or shape checks.
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      sourceTimeZone: undefined,
    });
    expect(result.ok).toBe(true);
    expect(result.ok && 'sourceTimeZone' in result.options).toBe(false);
  });

  it('returns deeply frozen canonical options', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'text' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      sourceTimeZone: { kind: 'utc' },
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(Object.isFrozen(result.options)).toBe(true);
    expect(Object.isFrozen(result.options.interpretation)).toBe(true);
    expect(Object.isFrozen(result.options.displayTimeZone)).toBe(true);
    expect(Object.isFrozen(result.options.sourceTimeZone)).toBe(true);
  });

  it('round-trips canonical options through JSON', () => {
    // text (not unix) so the sourceTimeZone below is semantically legal.
    const legal = {
      interpretation: { kind: 'text' },
      format: 'locale',
      displayTimeZone: { kind: 'iana', name: 'Asia/Kolkata' },
      sourceTimeZone: { kind: 'iana', name: 'Etc/GMT+8' },
    };
    const first = validateTimestampOptions(legal);
    expect(first.ok).toBe(true);
    if (!first.ok) return;
    const second = validateTimestampOptions(JSON.parse(JSON.stringify(first.options)));
    expect(second).toEqual(first);
  });
});

// ---------------------------------------------------------------------------
// Root shape errors
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: root shape errors', () => {
  it('rejects non-object roots with a path-less problem', () => {
    for (const bad of [null, undefined, 42, 'nope', true, [{}]]) {
      expect(validateTimestampOptions(bad)).toEqual({
        ok: false,
        problems: [{ path: '', message: 'expected an object' }],
      });
    }
  });

  it('rejects a missing interpretation', () => {
    const result = validateTimestampOptions({ format: 'iso', displayTimeZone: { kind: 'utc' } });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'interpretation',
      message: 'expected an object',
    });
  });

  it('rejects an unknown interpretation kind', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'julian' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'interpretation.kind',
      message: 'expected "native", "unix" or "text"',
    });
  });

  it('rejects unix without a unit and with a bogus unit', () => {
    for (const unit of [undefined, 'nanoseconds', 5]) {
      const result = validateTimestampOptions({
        interpretation: { kind: 'unix', unit },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.problems).toContainEqual({
        path: 'interpretation.unit',
        message: 'expected "seconds" or "milliseconds"',
      });
    }
  });

  it('rejects a unit on native and text interpretations', () => {
    for (const kind of ['native', 'text']) {
      const result = validateTimestampOptions({
        interpretation: { kind, unit: 'seconds' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.problems).toContainEqual({
        path: 'interpretation.unit',
        message: `unknown field ("${kind}" interpretation has no unit)`,
      });
    }
  });

  it('rejects an invalid format', () => {
    for (const format of [undefined, 'rfc2822', 7]) {
      const result = validateTimestampOptions({
        interpretation: { kind: 'native' },
        format,
        displayTimeZone: { kind: 'utc' },
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.problems).toContainEqual({
        path: 'format',
        message: 'expected "iso", "iso-tz" or "locale"',
      });
    }
  });
});

// ---------------------------------------------------------------------------
// TimeZoneSpec errors
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: time-zone spec errors', () => {
  it('rejects a missing or malformed displayTimeZone', () => {
    for (const bad of [undefined, null, 'UTC', 42]) {
      const result = validateTimestampOptions({
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: bad,
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.problems).toContainEqual({
        path: 'displayTimeZone',
        message: 'expected an object',
      });
    }
  });

  it('rejects an unknown time-zone spec kind', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'gps' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'displayTimeZone.kind',
      message: 'expected "utc", "local" or "iana"',
    });
  });

  it('applies the IANA format checks to iana names', () => {
    for (const name of ['', 'Foo Bar', 'Asia/上海', 42, null, 'A'.repeat(101)]) {
      const result = validateTimestampOptions({
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'iana', name },
      });
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.problems).toContainEqual({
        path: 'displayTimeZone.name',
        message: 'expected a non-empty IANA time zone name without whitespace',
      });
    }
  });

  it('accepts well-formed iana names (existence is validated at use time)', () => {
    // 'Mars/Olympus' passes the format check by design: full validation is
    // delegated to Intl at formatting time (documented two-stage approach).
    for (const name of ['UTC', 'Asia/Shanghai', 'Etc/GMT+8', 'Mars/Olympus', 'asia/shanghai']) {
      expect(
        validateTimestampOptions({
          interpretation: { kind: 'native' },
          format: 'iso',
          displayTimeZone: { kind: 'iana', name },
        }).ok,
      ).toBe(true);
    }
  });

  it('rejects a name field on utc/local specs', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc', name: 'UTC' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'displayTimeZone.name',
      message: 'unknown field (only "iana" carries a name)',
    });
  });

  it('rejects unknown fields inside a time-zone spec', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc', autoDst: true },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'displayTimeZone.autoDst',
      message: 'unknown field',
    });
  });
});

// ---------------------------------------------------------------------------
// sourceTimeZone semantics
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: sourceTimeZone semantics', () => {
  it('rejects sourceTimeZone on unix interpretation with a pathed error', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      sourceTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'sourceTimeZone',
      message: 'unix interpretation must not carry a source time zone',
    });
  });

  it('reports shape problems for a malformed sourceTimeZone', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'text' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      sourceTimeZone: null,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'sourceTimeZone',
      message: 'expected an object',
    });
  });

  it('accumulates shape and semantic problems for unix + malformed sourceTimeZone', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      sourceTimeZone: { kind: 'gps' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    const paths = result.problems.map((problem) => problem.path);
    expect(paths).toContain('sourceTimeZone.kind');
    expect(paths.filter((path) => path === 'sourceTimeZone').length).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Unknown-field rejection and problem accumulation
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: unknown fields and accumulation', () => {
  it('rejects unknown root fields', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      timezone: { kind: 'utc' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({ path: 'timezone', message: 'unknown field' });
  });

  it('rejects unknown fields inside the interpretation', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'unix', unit: 'seconds', calendar: 'gregorian' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'interpretation.calendar',
      message: 'unknown field',
    });
  });

  it('collects every problem instead of stopping at the first', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'julian' },
      format: 'rfc2822',
      displayTimeZone: { kind: 'gps' },
      bogus: 1,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    const paths = result.problems.map((problem) => problem.path);
    expect(paths).toEqual(
      expect.arrayContaining(['interpretation.kind', 'format', 'displayTimeZone.kind', 'bogus']),
    );
    expect(paths).toHaveLength(4);
  });
});

// ---------------------------------------------------------------------------
// Type-level sanity: the canonical result is usable as TimestampOptions
// ---------------------------------------------------------------------------

describe('validateTimestampOptions: canonical result shape', () => {
  it('produces options that satisfy the TimestampOptions interface', () => {
    const result = validateTimestampOptions({
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const options: TimestampOptions = result.options;
    expect(options.interpretation.kind).toBe('unix');
    if (options.interpretation.kind !== 'unix') return;
    expect(options.interpretation.unit).toBe('milliseconds');
    expect(options.format).toBe('iso-tz');
    expect(options.displayTimeZone).toEqual({ kind: 'iana', name: 'Asia/Shanghai' });
  });
});
