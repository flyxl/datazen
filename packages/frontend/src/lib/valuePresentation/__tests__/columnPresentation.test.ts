/**
 * columnPresentation（issue-32 T09）：列覆盖校验、快照读取、样本与预览。
 *
 * 覆盖：inherit / raw / custom 校验（非法 options、未知渲染器、未知
 * 字段、名内空白均拒绝）、无来源列恒为 inherit、样本锚点、预览与
 * 运行时同一条路（inherit 走计划、raw 阻断规则、custom Unix 秒与
 * 毫秒显式区分且不以位数自动决定、非法 custom 预览 invalid、贡献
 * 缺失预览 unavailable）。
 */
import { describe, expect, it } from 'vitest';
import { ValuePresentationRegistryImpl, type ColumnSource } from '@datazen/extension-points';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  columnOverrideKey,
  computeColumnPresentationPreview,
  columnOverrideFromSnapshot,
  fallbackSampleForColumn,
  formatColumnPresentationSampleValue,
  timestampContribution,
  validateColumnPresentationOverride,
  type ValuePresentationSnapshot,
} from '../index';

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' } as const;

const UNIX_MS_OPTIONS = {
  interpretation: { kind: 'unix', unit: 'milliseconds' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const UNIX_SECONDS_OPTIONS = {
  interpretation: { kind: 'unix', unit: 'seconds' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function enabledSnapshot(
  overrides: ValuePresentationSnapshot['overrides'] = {},
  rules: ValuePresentationSnapshot['rules'] = [],
): ValuePresentationSnapshot {
  return {
    revision: 1,
    environment: ENVIRONMENT,
    configs: {
      [TIMESTAMP_CONTRIBUTION_ID]: {
        configVersion: 1,
        config: {
          interpretation: { kind: 'native' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
    enabledRendererIds: [TIMESTAMP_CONTRIBUTION_ID],
    overrides,
    rules,
  };
}

describe('validateColumnPresentationOverride', () => {
  it('accepts inherit and raw with no extra fields', () => {
    expect(validateColumnPresentationOverride({ mode: 'inherit' })).toEqual({
      ok: true,
      override: { mode: 'inherit' },
    });
    expect(validateColumnPresentationOverride({ mode: 'raw' })).toEqual({
      ok: true,
      override: { mode: 'raw' },
    });
  });

  it('accepts a timestamp custom override and normalises its options', () => {
    const validated = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: UNIX_MS_OPTIONS,
    });
    expect(validated).toEqual({
      ok: true,
      override: { mode: 'custom', rendererId: 'datazen.timestamp', options: UNIX_MS_OPTIONS },
    });
  });

  it('rejects an unknown mode and unknown fields', () => {
    expect(validateColumnPresentationOverride({ mode: 'other' }).ok).toBe(false);
    expect(validateColumnPresentationOverride({ mode: 'raw', extra: 1 }).ok).toBe(false);
    expect(validateColumnPresentationOverride(null).ok).toBe(false);
  });

  it('rejects an unknown renderer instead of storing an unverifiable config', () => {
    const validated = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: 'someone.else',
      options: UNIX_MS_OPTIONS,
    });
    expect(validated.ok).toBe(false);
    if (!validated.ok) {
      expect(validated.problems.map((p) => p.path)).toContain('rendererId');
    }
  });

  it('rejects a renderer id with inner whitespace (T07 spacing ruling)', () => {
    const validated = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: ' datazen.timestamp ',
      options: UNIX_MS_OPTIONS,
    });
    // Trimmed, the id is the known one: leading/trailing space is accepted.
    expect(validated.ok).toBe(true);
    const withInnerSpace = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: 'datazen.time stamp',
      options: UNIX_MS_OPTIONS,
    });
    expect(withInnerSpace.ok).toBe(false);
  });

  it('rejects illegal timestamp options instead of storing them', () => {
    // Unix interpretation must not carry a source time zone (design §5).
    const validated = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        ...UNIX_MS_OPTIONS,
        sourceTimeZone: { kind: 'utc' },
      },
    });
    expect(validated.ok).toBe(false);
    if (!validated.ok) {
      expect(validated.problems.map((p) => p.path)).toContain('options.sourceTimeZone');
    }
    // Unix without an explicit unit is illegal: no digit-count inference.
    const noUnit = validateColumnPresentationOverride({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        interpretation: { kind: 'unix' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
    });
    expect(noUnit.ok).toBe(false);
  });
});

describe('columnOverrideFromSnapshot', () => {
  it('returns the stored override for a sourced column', () => {
    const snapshot = enabledSnapshot({
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
    });
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'epoch_ms')).toEqual({ mode: 'raw' });
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'other')).toEqual({ mode: 'inherit' });
  });

  it('returns inherit for a sourceless (query result) column', () => {
    const snapshot = enabledSnapshot({
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
    });
    expect(columnOverrideFromSnapshot(snapshot, undefined, 'epoch_ms')).toEqual({
      mode: 'inherit',
    });
  });
});

describe('fallbackSampleForColumn / formatColumnPresentationSampleValue', () => {
  it('uses the PRD anchor samples by semantic kind', () => {
    expect(fallbackSampleForColumn({ name: 'epoch_ms', dataType: 'bigint' })).toBe(1705316400000);
    expect(
      fallbackSampleForColumn({ name: 'created_at', dataType: 'timestamp with time zone' }),
    ).toBe('2026-01-15T14:30:00Z');
    expect(fallbackSampleForColumn({ name: 'ts', dataType: 'timestamp without time zone' })).toBe(
      '2026-01-15 14:30:00',
    );
    expect(fallbackSampleForColumn({ name: 'name', dataType: 'varchar' })).toBeUndefined();
    expect(fallbackSampleForColumn({ name: 'd', dataType: 'date' })).toBe('2026-01-15');
    expect(fallbackSampleForColumn({ name: 't', dataType: 'time' })).toBe('14:30:00');
  });

  it('formats null as NULL and objects as JSON', () => {
    expect(formatColumnPresentationSampleValue(null)).toBe('NULL');
    expect(formatColumnPresentationSampleValue(1705316400000)).toBe('1705316400000');
    expect(formatColumnPresentationSampleValue({ a: 1 })).toBe('{"a":1}');
  });
});

describe('computeColumnPresentationPreview', () => {
  const epochColumn = { name: 'epoch_ms', dataType: 'bigint' };
  const registry = registryWithTimestamp();

  it('previews Unix milliseconds exactly (PRD §3.2 anchor)', () => {
    const preview = computeColumnPresentationPreview({
      candidate: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: UNIX_MS_OPTIONS,
      },
      column: epochColumn,
      source: SOURCE,
      snapshot: enabledSnapshot(),
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    expect(preview).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
      sample: 1705316400000,
    });
  });

  it('treats Unix seconds and milliseconds as explicit, different choices', () => {
    const previewMs = computeColumnPresentationPreview({
      candidate: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: UNIX_MS_OPTIONS,
      },
      column: epochColumn,
      source: SOURCE,
      snapshot: enabledSnapshot(),
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    const previewSeconds = computeColumnPresentationPreview({
      candidate: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: UNIX_SECONDS_OPTIONS,
      },
      column: epochColumn,
      source: SOURCE,
      snapshot: enabledSnapshot(),
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    expect(previewMs.kind).toBe('text');
    expect(previewSeconds.kind).toBe('text');
    if (previewMs.kind === 'text' && previewSeconds.kind === 'text') {
      // Same digits, explicit unit only: milliseconds anchor vs seconds far-future.
      expect(previewMs.text).toBe('2024-01-15T19:00:00+08:00');
      expect(previewSeconds.text).not.toBe(previewMs.text);
      expect(previewSeconds.text).toBe('+56009-04-29T16:00:00+08:00');
    }
  });

  it('raw blocks a matching rule: the column still shows its raw value', () => {
    const ruleSnapshot = enabledSnapshot({}, [
      {
        name: 'unix-ms',
        enabled: true,
        match: { column: { kind: 'exact', value: 'epoch_ms' } },
        options: {
          interpretation: { kind: 'unix', unit: 'milliseconds' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    ]);
    // Sanity: inherit follows the rule.
    const inherited = computeColumnPresentationPreview({
      candidate: { mode: 'inherit' },
      column: epochColumn,
      source: SOURCE,
      snapshot: ruleSnapshot,
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    expect(inherited).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
      sample: 1705316400000,
    });
    // Raw blocks it.
    const raw = computeColumnPresentationPreview({
      candidate: { mode: 'raw' },
      column: epochColumn,
      source: SOURCE,
      snapshot: ruleSnapshot,
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    expect(raw).toEqual({ kind: 'unchanged', reason: 'raw-override', sample: 1705316400000 });
  });

  it('reports invalid for an illegal custom override instead of previewing it', () => {
    const preview = computeColumnPresentationPreview({
      candidate: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'utc' },
        },
      },
      column: epochColumn,
      source: SOURCE,
      snapshot: enabledSnapshot(),
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry,
    });
    expect(preview.kind).toBe('invalid');
  });

  it('reports unavailable when the timestamp contribution is missing', () => {
    const emptyRegistry = new ValuePresentationRegistryImpl();
    const preview = computeColumnPresentationPreview({
      candidate: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: UNIX_MS_OPTIONS,
      },
      column: epochColumn,
      source: SOURCE,
      snapshot: enabledSnapshot(),
      environment: ENVIRONMENT,
      sampleValue: 1705316400000,
      registry: emptyRegistry,
    });
    expect(preview).toEqual({ kind: 'unavailable', sample: 1705316400000 });
  });
});
