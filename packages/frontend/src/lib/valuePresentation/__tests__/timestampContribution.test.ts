import { describe, expect, it } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  registerValuePresentation,
  type ColumnPresentationOverride,
  type ExtensionContext,
  type PresentationColumnContext,
  type PresentationConfigValue,
  type PresentationEnvironment,
  type PresentationPrepareInput,
  type PresentationValue,
  type PreparedPresentation,
} from '@datazen/extension-points';
import { timestampContribution, type TimestampOptions } from '../index';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SHANGHAI = { kind: 'iana', name: 'Asia/Shanghai' } as const;
const NEW_YORK = { kind: 'iana', name: 'America/New_York' } as const;
const UTC_SPEC = { kind: 'utc' } as const;

/** 环境快照：localTimeZone 固定为上海，与运行机器时区（UTC）不同。 */
const ENV_SHANGHAI: PresentationEnvironment = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' };

function column(semanticKind: PresentationColumnContext['semanticKind'] = 'instant'): {
  columnName: string;
  semanticKind: PresentationColumnContext['semanticKind'];
} {
  return { columnName: 'created_at', semanticKind };
}

interface PrepareOptions {
  configVersion?: number;
  columnOverride?: ColumnPresentationOverride;
  column?: PresentationColumnContext;
  environment?: PresentationEnvironment;
}

/**
 * 组装 prepare 输入：全局配置以 JSON 形状传入（宿主边界形态），
 * configVersion 缺省为 1。
 */
function makeInput(config: unknown, options: PrepareOptions = {}): PresentationPrepareInput {
  return {
    // 测试 helper：输入以 unknown 传入，由贡献在运行时验证；此处仅满足
    // VersionedPresentationConfig 的静态类型（不断言跳过任何验证）。
    config: {
      configVersion: options.configVersion ?? 1,
      config: config as PresentationConfigValue,
    },
    columnOverride: options.columnOverride,
    column: options.column ?? column(),
    environment: options.environment ?? ENV_SHANGHAI,
  };
}

/** 便捷入口：以 TimestampOptions 全局配置做 prepare。 */
function prepareWith(config: TimestampOptions, options: PrepareOptions = {}): PreparedPresentation {
  return timestampContribution.prepare(makeInput(config, options));
}

const NATIVE_SHANGHAI_ISO_TZ: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: SHANGHAI,
};

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: UTC_SPEC,
};

/** fake ExtensionContext（T01 lifecycle 形态：仅 subscriptions + extensionId）。 */
function fakeContext(extensionId = 'test-ext'): ExtensionContext {
  return { extensionId, subscriptions: [] };
}

// ---------------------------------------------------------------------------
// 贡献身份
// ---------------------------------------------------------------------------

describe('timestampContribution: identity', () => {
  it("declares id 'datazen.timestamp' and configVersion 1", () => {
    expect(timestampContribution.id).toBe('datazen.timestamp');
    expect(timestampContribution.configVersion).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// validateConfig：委托 T02，问题原样透传
// ---------------------------------------------------------------------------

describe('timestampContribution.validateConfig', () => {
  it('accepts a minimal native configuration and returns JSON config', () => {
    const result = timestampContribution.validateConfig({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
    });
    expect(result).toEqual({
      ok: true,
      config: {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
    expect(result.ok && 'sourceTimeZone' in (result.config as Record<string, unknown>)).toBe(false);
  });

  it('returns the validated canonical options (source time zone kept)', () => {
    const result = timestampContribution.validateConfig({
      interpretation: { kind: 'text' },
      format: 'iso-tz',
      displayTimeZone: SHANGHAI,
      sourceTimeZone: NEW_YORK,
    });
    expect(result).toEqual({
      ok: true,
      config: {
        interpretation: { kind: 'text' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        sourceTimeZone: { kind: 'iana', name: 'America/New_York' },
      },
    });
  });

  it('rejects unix + sourceTimeZone with the T02 problem passed through', () => {
    const result = timestampContribution.validateConfig({
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      sourceTimeZone: SHANGHAI,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'sourceTimeZone',
      message: 'unix interpretation must not carry a source time zone',
    });
    // 失败不返回部分配置。
    expect('config' in result).toBe(false);
  });

  it('rejects unknown fields (v1 has no rules field)', () => {
    const result = timestampContribution.validateConfig({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'utc' },
      rules: [],
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({ path: 'rules', message: 'unknown field' });
  });

  it('rejects non-object input with a path-less problem and never throws', () => {
    for (const bad of [null, undefined, 42, 'nope', true, [{}]]) {
      expect(timestampContribution.validateConfig(bad)).toEqual({
        ok: false,
        problems: [{ path: '', message: 'expected an object' }],
      });
    }
  });

  it('passes T02 problems through verbatim (bad format, bad zone kind)', () => {
    const result = timestampContribution.validateConfig({
      interpretation: { kind: 'native' },
      format: 'rfc2822',
      displayTimeZone: { kind: 'gps' },
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.problems).toContainEqual({
      path: 'format',
      message: 'expected "iso", "iso-tz" or "locale"',
    });
    expect(result.problems).toContainEqual({
      path: 'displayTimeZone.kind',
      message: 'expected "utc", "local" or "iana"',
    });
  });
});

// ---------------------------------------------------------------------------
// prepare：版本与全局配置解析
// ---------------------------------------------------------------------------

describe('timestampContribution.prepare: config version and global config', () => {
  it('returns unsupported-config-version for every value when version != 1', () => {
    const prepared = timestampContribution.prepare(
      makeInput(
        {
          interpretation: { kind: 'native' },
          format: 'iso',
          displayTimeZone: { kind: 'utc' },
        },
        { configVersion: 2 },
      ),
    );
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'unsupported-config-version',
    });
    expect(prepared.present(null)).toEqual({
      kind: 'unchanged',
      reason: 'unsupported-config-version',
    });
    // 版本 0 同样不匹配（不校验配置体）。
    const v0 = timestampContribution.prepare(makeInput(null, { configVersion: 0 }));
    expect(v0.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'unsupported-config-version',
    });
  });

  it('returns invalid-config for every value when the global config is invalid', () => {
    const prepared = timestampContribution.prepare(
      makeInput({ interpretation: { kind: 'native' }, format: 'nope' }),
    );
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
    expect(prepared.present(null)).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });

  it('global invalid-config takes precedence over a raw override (documented order)', () => {
    // 裁决：解析顺序为 版本 → 全局有效性 → 列覆盖；全局无效时一律 invalid-config。
    const prepared = prepareWith(
      { interpretation: { kind: 'native' }, format: 'nope' } as unknown as TimestampOptions,
      { columnOverride: { mode: 'raw' } },
    );
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });

  it('never throws on a malformed prepare snapshot; present falls back', () => {
    const prepared = timestampContribution.prepare({} as unknown as PresentationPrepareInput);
    expect(() => prepared.present('2026-01-15T14:30:00Z')).not.toThrow();
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'unexpected-error',
    });
  });
});

// ---------------------------------------------------------------------------
// prepare：列覆盖
// ---------------------------------------------------------------------------

describe('timestampContribution.prepare: column overrides', () => {
  it("raw override returns 'raw-override' for every value", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, {
      columnOverride: { mode: 'raw' },
    });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'raw-override',
    });
    expect(prepared.present(null)).toEqual({
      kind: 'unchanged',
      reason: 'raw-override',
    });
  });

  it('inherit (and absent) override uses the global configuration', () => {
    for (const columnOverride of [undefined, { mode: 'inherit' } as const]) {
      const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, { columnOverride });
      expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
        kind: 'text',
        text: '2026-01-15T22:30:00+08:00',
        interpretation: 'native',
      });
    }
  });

  it('custom override with a matching rendererId replaces the global options', () => {
    const prepared = prepareWith(NATIVE_UTC_ISO, {
      columnOverride: {
        mode: 'custom',
        rendererId: 'datazen.timestamp',
        options: {
          interpretation: { kind: 'native' },
          format: 'iso-tz',
          displayTimeZone: SHANGHAI,
        },
      },
    });
    // 全局是 iso/UTC，覆盖是 iso-tz/上海：覆盖生效。
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it("custom override with another rendererId returns 'override-target-mismatch'", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, {
      columnOverride: {
        mode: 'custom',
        rendererId: 'other.renderer',
        options: {
          interpretation: { kind: 'native' },
          format: 'iso',
          displayTimeZone: { kind: 'utc' },
        },
      },
    });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'override-target-mismatch',
    });
  });

  it("custom override with invalid options returns 'invalid-config'", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, {
      columnOverride: {
        mode: 'custom',
        rendererId: 'datazen.timestamp',
        options: {
          interpretation: { kind: 'unix', unit: 'seconds' },
          format: 'iso',
          displayTimeZone: { kind: 'utc' },
          sourceTimeZone: SHANGHAI,
        },
      },
    });
    expect(prepared.present(1705316400)).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });

  it("an unknown override mode at runtime returns 'invalid-config' (no guessing)", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, {
      columnOverride: { mode: 'bogus' } as unknown as ColumnPresentationOverride,
    });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });
});

// ---------------------------------------------------------------------------
// present：native 解释
// ---------------------------------------------------------------------------

describe('timestampContribution.present: native interpretation', () => {
  it('PRD anchor: zoned text + Asia/Shanghai iso-tz contains 22:30 and +08:00', () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('iso output is UTC regardless of the display zone', () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    const iso = prepareWith({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: SHANGHAI,
    });
    expect(iso.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
    expect(prepared.present('2026-01-15T14:30:00+08:00')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('locale output is text (explicit locale + zone, no machine default)', () => {
    const prepared = prepareWith({
      interpretation: { kind: 'native' },
      format: 'locale',
      displayTimeZone: SHANGHAI,
    });
    const result = prepared.present('2026-01-15T14:30:00Z');
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.interpretation).toBe('native');
    expect(result.text.length).toBeGreaterThan(0);
  });

  it('resolves displayTimeZone local from the environment snapshot, not the machine', () => {
    // 运行机器时区为 UTC；环境 localTimeZone 为上海 → 必须输出 +08:00。
    const prepared = prepareWith({
      interpretation: { kind: 'native' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'local' },
    });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('does not auto-interpret numbers or bigints as Unix', () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    expect(prepared.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    expect(prepared.present(1705316400000n)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
  });

  it('falls back to wall-clock text with the source time zone', () => {
    const prepared = prepareWith({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
      sourceTimeZone: SHANGHAI,
    });
    expect(prepared.present('2026-01-15 14:30:00')).toEqual({
      kind: 'text',
      text: '2026-01-15T06:30:00Z',
      interpretation: 'native',
    });
  });

  it('resolves sourceTimeZone local from the environment snapshot', () => {
    const prepared = prepareWith(
      {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: UTC_SPEC,
        sourceTimeZone: { kind: 'local' },
      },
      { environment: { locale: 'en-US', localTimeZone: 'America/New_York' } },
    );
    // 2026-01-15 纽约为 EST（-05:00）：14:30 墙上时间 = 19:30 UTC。
    expect(prepared.present('2026-01-15 14:30:00')).toEqual({
      kind: 'text',
      text: '2026-01-15T19:30:00Z',
      interpretation: 'native',
    });
  });

  it('without a source time zone, wall-clock text is unchanged with the zoned reason', () => {
    const prepared = prepareWith(NATIVE_UTC_ISO);
    expect(prepared.present('2026-01-15 14:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-format',
    });
  });

  it('passes the underlying parse reason code through for garbage text', () => {
    const prepared = prepareWith(NATIVE_UTC_ISO);
    expect(prepared.present('not-a-date')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-format',
    });
    expect(prepared.present('2026-13-01T00:00:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-month',
    });
    expect(prepared.present('')).toEqual({ kind: 'unchanged', reason: 'empty-input' });
  });

  it('wall-clock fallback reports dst-ambiguous / dst-invalid with stable codes', () => {
    const prepared = prepareWith({
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
      sourceTimeZone: NEW_YORK,
    });
    // 2026-11-01 美国夏令时结束：01:30 出现两次。
    expect(prepared.present('2026-11-01 01:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'dst-ambiguous',
    });
    // 2026-03-08 美国夏令时开始：02:30 不存在。
    expect(prepared.present('2026-03-08 02:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'dst-invalid',
    });
  });
});

// ---------------------------------------------------------------------------
// present：unix 解释
// ---------------------------------------------------------------------------

describe('timestampContribution.present: unix interpretation', () => {
  const UNIX_MS_SHANGHAI: TimestampOptions = {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: SHANGHAI,
  };

  it('PRD anchor: Unix 1705316400000 ms -> 2024-01-15T19:00:00+08:00', () => {
    const prepared = prepareWith(UNIX_MS_SHANGHAI);
    expect(prepared.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it('accepts seconds numbers, integer strings and bigints', () => {
    const seconds: TimestampOptions = {
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso-tz',
      displayTimeZone: SHANGHAI,
    };
    const prepared = prepareWith(seconds);
    const expected = {
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    } as const;
    expect(prepared.present(1705316400)).toEqual(expected);
    expect(prepared.present('1705316400')).toEqual(expected);
    const ms = prepareWith(UNIX_MS_SHANGHAI);
    expect(ms.present(1705316400000n)).toEqual(expected);
    expect(ms.present('1705316400000')).toEqual(expected);
  });

  it('handles 0 and negative Unix values as real instants', () => {
    const prepared = prepareWith({
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
    });
    expect(prepared.present(0)).toEqual({
      kind: 'text',
      text: '1970-01-01T00:00:00Z',
      interpretation: 'unix',
    });
    const seconds = prepareWith({
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
    });
    expect(seconds.present(-1)).toEqual({
      kind: 'text',
      text: '1969-12-31T23:59:59Z',
      interpretation: 'unix',
    });
  });

  it('passes Unix parse reason codes through', () => {
    const prepared = prepareWith(UNIX_MS_SHANGHAI);
    expect(prepared.present(2 ** 53)).toEqual({
      kind: 'unchanged',
      reason: 'not-safe-integer',
    });
    expect(prepared.present(Number.NaN)).toEqual({
      kind: 'unchanged',
      reason: 'not-safe-integer',
    });
    expect(prepared.present('12.5')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-integer-syntax',
    });
    expect(prepared.present(' 42')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-integer-syntax',
    });
    expect(prepared.present('')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-integer-syntax',
    });
    expect(prepared.present('99999999999999999')).toEqual({
      kind: 'unchanged',
      reason: 'out-of-range',
    });
  });
});

// ---------------------------------------------------------------------------
// present：text 解释
// ---------------------------------------------------------------------------

describe('timestampContribution.present: text interpretation', () => {
  const TEXT_SHANGHAI_UTC: TimestampOptions = {
    interpretation: { kind: 'text' },
    format: 'iso',
    displayTimeZone: UTC_SPEC,
    sourceTimeZone: SHANGHAI,
  };

  it('converts wall-clock datetime with the source time zone', () => {
    const prepared = prepareWith(TEXT_SHANGHAI_UTC);
    expect(prepared.present('2026-01-15 14:30:00')).toEqual({
      kind: 'text',
      text: '2026-01-15T06:30:00Z',
      interpretation: 'text',
    });
  });

  it('treats date-only text as midnight in the source zone', () => {
    const prepared = prepareWith(TEXT_SHANGHAI_UTC);
    // 2026-03-01 00:00 +08:00 = 2026-02-28 16:00 UTC（2026 不是闰年）。
    expect(prepared.present('2026-03-01')).toEqual({
      kind: 'text',
      text: '2026-02-28T16:00:00Z',
      interpretation: 'text',
    });
  });

  it('anchors time-only text to the 1970-01-01 epoch day (documented decision)', () => {
    const prepared = prepareWith(TEXT_SHANGHAI_UTC);
    expect(prepared.present('14:30:00')).toEqual({
      kind: 'text',
      text: '1970-01-01T06:30:00Z',
      interpretation: 'text',
    });
  });

  it("returns 'missing-source-time-zone' when no source zone is configured", () => {
    const prepared = prepareWith({
      interpretation: { kind: 'text' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
    });
    expect(prepared.present('2026-01-15 14:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'missing-source-time-zone',
    });
  });

  it('passes wall-clock parse and DST reason codes through', () => {
    const prepared = prepareWith(TEXT_SHANGHAI_UTC);
    expect(prepared.present('not-a-date')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-format',
    });
    const newYork = prepareWith({
      interpretation: { kind: 'text' },
      format: 'iso',
      displayTimeZone: UTC_SPEC,
      sourceTimeZone: NEW_YORK,
    });
    expect(newYork.present('2026-11-01 01:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'dst-ambiguous',
    });
    expect(newYork.present('2026-03-08 02:30:00')).toEqual({
      kind: 'unchanged',
      reason: 'dst-invalid',
    });
  });

  it('only accepts strings under text interpretation', () => {
    const prepared = prepareWith(TEXT_SHANGHAI_UTC);
    expect(prepared.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    expect(prepared.present(1705316400000n)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
  });
});

// ---------------------------------------------------------------------------
// present：null / 非法值 / semanticKind 门禁 / 格式化失败 / 永不抛异常
// ---------------------------------------------------------------------------

describe('timestampContribution.present: null, inapplicable values, kind gate', () => {
  it("returns reason 'null' for null and undefined", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    expect(prepared.present(null)).toEqual({ kind: 'unchanged', reason: 'null' });
    expect(prepared.present(undefined)).toEqual({ kind: 'unchanged', reason: 'null' });
  });

  it("returns 'not-applicable' for boolean, array and object values", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    expect(prepared.present(true)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    expect(prepared.present([2026])).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    expect(prepared.present({ year: 2026 })).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
  });

  it("semanticKind 'other' never applies, even for valid timestamps or null", () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, { column: column('other') });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    // 门禁在 null 检查之前：other 列的 null 得到 not-applicable 而非 null。
    expect(prepared.present(null)).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
  });

  it("semanticKind 'number' blocks native but allows unix", () => {
    const native = prepareWith(NATIVE_SHANGHAI_ISO_TZ, { column: column('number') });
    expect(native.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'not-applicable',
    });
    const unix = prepareWith(
      {
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'iso-tz',
        displayTimeZone: SHANGHAI,
      },
      { column: column('number') },
    );
    expect(unix.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it("semanticKind 'date' / 'time' / 'local-datetime' / 'text' follow the interpretation", () => {
    for (const semanticKind of ['date', 'time', 'local-datetime', 'text', 'instant'] as const) {
      const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ, { column: column(semanticKind) });
      expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
        kind: 'text',
        text: '2026-01-15T22:30:00+08:00',
        interpretation: 'native',
      });
    }
  });

  it('formatting failures surface the underlying format reason code', () => {
    // 'Mars/Olympus' 通过 IANA 格式校验，但 Intl 构造失败 → invalid-time-zone。
    const prepared = prepareWith({
      interpretation: { kind: 'native' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Mars/Olympus' },
    });
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-time-zone',
    });
    // 空 locale 字符串被格式层显式拒绝。
    const badLocale = prepareWith(NATIVE_SHANGHAI_ISO_TZ, {
      environment: { locale: '', localTimeZone: 'Asia/Shanghai' },
    });
    const localePrepared = timestampContribution.prepare(
      makeInput(
        {
          interpretation: { kind: 'native' },
          format: 'locale',
          displayTimeZone: SHANGHAI,
        },
        { environment: { locale: '', localTimeZone: 'Asia/Shanghai' } },
      ),
    );
    expect(badLocale.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    expect(localePrepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-locale',
    });
  });

  it('present never throws across a battery of odd values', () => {
    const prepared = prepareWith(NATIVE_SHANGHAI_ISO_TZ);
    const odd: PresentationValue[] = [
      null,
      undefined,
      true,
      false,
      0,
      -1,
      2 ** 53,
      Number.NaN,
      Number.POSITIVE_INFINITY,
      0n,
      '',
      ' ',
      '2026-02-30T00:00:00Z',
      [],
      [null],
      {},
      { toString: 1 },
      Symbol('ts') as unknown as PresentationValue,
      (() => 1) as unknown as PresentationValue,
    ];
    for (const value of odd) {
      let result: { kind: string } | undefined;
      expect(() => {
        result = prepared.present(value);
      }).not.toThrow();
      expect(result?.kind).toMatch(/^(text|unchanged)$/);
    }
  });
});

// ---------------------------------------------------------------------------
// 注册中心集成（只验贡献对象可注册且 prepare 可用，不重测注册中心本身）
// ---------------------------------------------------------------------------

describe('timestampContribution: registry integration', () => {
  it('registers via registerValuePresentation and prepare works from the registry', () => {
    const registry = new ValuePresentationRegistryImpl();
    const context = fakeContext();
    registerValuePresentation(context, timestampContribution, registry);

    expect(context.subscriptions).toHaveLength(1);
    expect(registry.get('datazen.timestamp')).toBe(timestampContribution);
    const registered = registry.get('datazen.timestamp');
    expect(registered).toBeDefined();
    if (registered === undefined) return;
    const prepared = registered.prepare(
      makeInput({
        interpretation: { kind: 'native' },
        format: 'iso-tz',
        displayTimeZone: SHANGHAI,
      }),
    );
    expect(prepared.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });

    // 释放句柄反注册（T01 语义：只移除本次注册）。
    const release = context.subscriptions[0];
    expect(typeof release).toBe('function');
    (release as () => void)();
    expect(registry.get('datazen.timestamp')).toBeUndefined();
  });

  it('duplicate id registration is rejected and keeps the original', () => {
    const registry = new ValuePresentationRegistryImpl();
    const first = fakeContext('ext-first');
    registerValuePresentation(first, timestampContribution, registry);
    const second = fakeContext('ext-second');
    expect(() => registerValuePresentation(second, timestampContribution, registry)).toThrow(
      /duplicate contribution id 'datazen\.timestamp'/,
    );
    expect(registry.get('datazen.timestamp')).toBe(timestampContribution);
    expect(second.subscriptions).toHaveLength(0);
  });
});
