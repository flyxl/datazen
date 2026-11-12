/**
 * valuePresentation 存储逻辑（issue-32 T06）：宽容解析与快照构建。
 *
 * 覆盖：parse 的宽容规则（非对象 / version 非数字 / renderers 非对象 /
 * 非对象条目丢弃 / 未知版本原样保留）；buildSnapshot 矩阵（有效→ready、
 * 无效 options→排除、enabled=false→排除、缺失贡献→排除、未知版本→空
 * 快照且原文保留、overrides 恒 {})。全部用全新
 * ValuePresentationRegistryImpl，不动全局单例。
 */
import { describe, expect, it } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  type ColumnPresentationOverride,
} from '@datazen/extension-points';
import { TIMESTAMP_CONTRIBUTION_ID, timestampContribution, type TimestampOptions } from '../index';
import {
  DEFAULT_VALUE_PRESENTATION_SETTINGS,
  buildValuePresentationSnapshot,
  parseStoredValuePresentationSettings,
  type StoredValuePresentationSettings,
} from '../settings';

const ENV = { locale: 'en-US', localTimeZone: 'UTC' } as const;

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

const UNIX_MS_SHANGHAI: TimestampOptions = {
  interpretation: { kind: 'unix', unit: 'milliseconds' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function storedWith(
  renderers: StoredValuePresentationSettings['renderers'],
  version = 1,
): StoredValuePresentationSettings {
  return { version, renderers };
}

describe('parseStoredValuePresentationSettings', () => {
  it('returns the default for non-object input', () => {
    for (const input of [undefined, null, 'x', 42, true, [], ['a']]) {
      expect(parseStoredValuePresentationSettings(input)).toEqual(
        DEFAULT_VALUE_PRESENTATION_SETTINGS,
      );
    }
  });

  it('returns the default for an empty object', () => {
    expect(parseStoredValuePresentationSettings({})).toEqual({
      version: 1,
      renderers: {},
    });
  });

  it('falls back to version 1 when version is not a number', () => {
    expect(parseStoredValuePresentationSettings({ version: '1', renderers: {} })).toEqual({
      version: 1,
      renderers: {},
    });
    expect(parseStoredValuePresentationSettings({ version: null })).toEqual({
      version: 1,
      renderers: {},
    });
  });

  it('keeps an unknown top-level version verbatim instead of rewriting it', () => {
    expect(parseStoredValuePresentationSettings({ version: 99, renderers: {} })).toEqual({
      version: 99,
      renderers: {},
    });
  });

  it('falls back to empty renderers when renderers is not an object', () => {
    for (const renderers of [null, 'x', 42, []]) {
      expect(parseStoredValuePresentationSettings({ version: 1, renderers })).toEqual({
        version: 1,
        renderers: {},
      });
    }
  });

  it('drops non-object renderer entries and keeps object entries verbatim', () => {
    const entry = { enabled: true, options: NATIVE_UTC_ISO };
    const parsed = parseStoredValuePresentationSettings({
      version: 1,
      renderers: {
        'datazen.timestamp': entry,
        broken: 'not-an-object',
        alsoBroken: 42,
        nullEntry: null,
      },
    });
    expect(parsed).toEqual({
      version: 1,
      renderers: { 'datazen.timestamp': entry },
    });
    // 条目内容原样保留：不做 options 深验证（深验证在快照构建时按贡献做）。
    expect(parsed.renderers['datazen.timestamp'].options).toBe(NATIVE_UTC_ISO);
  });

  it('returns fresh objects instead of aliasing the shared default', () => {
    const a = parseStoredValuePresentationSettings(null);
    const b = parseStoredValuePresentationSettings(undefined);
    expect(a).not.toBe(DEFAULT_VALUE_PRESENTATION_SETTINGS);
    expect(a).not.toBe(b);
    expect(a.renderers).not.toBe(b.renderers);
  });
});

describe('buildValuePresentationSnapshot', () => {
  it('builds a ready config for a valid enabled renderer', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({ [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } }),
      ENV,
      registry,
      7,
    );
    expect(snapshot.revision).toBe(7);
    expect(snapshot.environment).toEqual(ENV);
    expect(snapshot.enabledRendererIds).toEqual([TIMESTAMP_CONTRIBUTION_ID]);
    expect(snapshot.configs[TIMESTAMP_CONTRIBUTION_ID]).toEqual({
      configVersion: 1,
      config: {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
    expect(snapshot.overrides).toEqual({});
  });

  it('validates options through the contribution (unix ms keeps its shape)', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({ [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: UNIX_MS_SHANGHAI } }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.enabledRendererIds).toEqual([TIMESTAMP_CONTRIBUTION_ID]);
    expect(snapshot.configs[TIMESTAMP_CONTRIBUTION_ID]?.config).toEqual({
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
  });

  it('excludes a renderer whose options fail contribution validation', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({
        [TIMESTAMP_CONTRIBUTION_ID]: {
          enabled: true,
          // format 不在 iso / iso-tz / locale 中 → validateConfig 失败。
          options: { ...NATIVE_UTC_ISO, format: 'bogus' } as unknown as TimestampOptions,
        },
      }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.configs).toEqual({});
    expect(snapshot.enabledRendererIds).toEqual([]);
  });

  it('excludes a renderer with enabled === false', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({ [TIMESTAMP_CONTRIBUTION_ID]: { enabled: false, options: NATIVE_UTC_ISO } }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.configs).toEqual({});
    expect(snapshot.enabledRendererIds).toEqual([]);
  });

  it('excludes a renderer whose contribution is missing', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({ 'acme.future': { enabled: true, options: NATIVE_UTC_ISO } }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.configs).toEqual({});
    expect(snapshot.enabledRendererIds).toEqual([]);
  });

  it('returns an empty snapshot for an unknown top-level version and keeps the original', () => {
    const registry = registryWithTimestamp();
    const stored = storedWith(
      { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
      99,
    );
    const snapshot = buildValuePresentationSnapshot(stored, ENV, registry, 3);
    expect(snapshot).toEqual({
      revision: 3,
      environment: ENV,
      configs: {},
      enabledRendererIds: [],
      overrides: {},
      rules: [],
    });
    // 原文保留在 store：构建不改写 stored，也不返回默认值。
    expect(stored.version).toBe(99);
    expect(stored.renderers[TIMESTAMP_CONTRIBUTION_ID]).toBeDefined();
  });

  it('returns an empty snapshot for the default (all-off) stored value', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      DEFAULT_VALUE_PRESENTATION_SETTINGS,
      ENV,
      registry,
      0,
    );
    expect(snapshot.configs).toEqual({});
    expect(snapshot.enabledRendererIds).toEqual([]);
    expect(snapshot.overrides).toEqual({});
  });

  it('overrides is empty when nothing is stored', () => {
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({ [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.overrides).toEqual({});
  });

  it('keeps an enabled-but-invalid renderer out of configs so the plan reports missing-config', () => {
    // T04 的 missing-config 区分配合：enabled 但无效的不进 configs，计划
    // 构建时因 configs 缺失报 missing-config，而不是 disabled。
    const registry = registryWithTimestamp();
    const snapshot = buildValuePresentationSnapshot(
      storedWith({
        [TIMESTAMP_CONTRIBUTION_ID]: {
          enabled: true,
          options: {
            interpretation: { kind: 'unix' },
            format: 'iso',
            displayTimeZone: { kind: 'utc' },
          } as unknown as TimestampOptions,
        },
      }),
      ENV,
      registry,
      1,
    );
    expect(snapshot.enabledRendererIds).toEqual([]);
    expect(snapshot.configs[TIMESTAMP_CONTRIBUTION_ID]).toBeUndefined();
  });
});

describe('parseStoredValuePresentationSettings: rules (issue-32 T08)', () => {
  const RULE = {
    name: 'unix-ms',
    enabled: true,
    match: {
      column: { kind: 'regex', source: '^(created_at|updated_at)$' },
      databaseTypeName: { kind: 'exact', value: 'BIGINT' },
    },
    options: UNIX_MS_SHANGHAI,
  };

  it('defaults to no rules key for an old payload without rules', () => {
    const parsed = parseStoredValuePresentationSettings({ version: 1, renderers: {} });
    expect(parsed).toEqual({ version: 1, renderers: {} });
    expect(parsed.rules).toBeUndefined();
  });

  it('keeps valid rules and drops invalid entries', () => {
    const parsed = parseStoredValuePresentationSettings({
      version: 1,
      renderers: {},
      rules: [RULE, { ...RULE, name: '' }, 'not-a-rule', { ...RULE, name: 'second' }],
    });
    expect(parsed.rules?.map((r) => r.name)).toEqual(['unix-ms', 'second']);
  });

  it('ignores a non-array rules field', () => {
    expect(parseStoredValuePresentationSettings({ version: 1, renderers: {}, rules: 'x' })).toEqual(
      { version: 1, renderers: {} },
    );
  });

  it('does not parse rules for an unknown top-level version', () => {
    const parsed = parseStoredValuePresentationSettings({
      version: 99,
      renderers: {},
      rules: [RULE],
    });
    expect(parsed).toEqual({ version: 99, renderers: {} });
    expect(parsed.rules).toBeUndefined();
  });
});

describe('parseStoredValuePresentationSettings: columnOverrides (issue-32 T10)', () => {
  const KEY = JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'epoch_ms']);
  const RAW: ColumnPresentationOverride = { mode: 'raw' };
  const CUSTOM: ColumnPresentationOverride = {
    mode: 'custom',
    rendererId: TIMESTAMP_CONTRIBUTION_ID,
    options: UNIX_MS_SHANGHAI,
  };

  it('defaults to no columnOverrides key for an old payload without the field', () => {
    const parsed = parseStoredValuePresentationSettings({ version: 1, renderers: {} });
    expect(parsed).toEqual({ version: 1, renderers: {} });
    expect(parsed.columnOverrides).toBeUndefined();
  });

  it('keeps valid overrides and drops invalid keys, invalid overrides and inherit entries', () => {
    const parsed = parseStoredValuePresentationSettings({
      version: 1,
      renderers: {},
      columnOverrides: {
        [KEY]: RAW,
        [JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'created_at'])]: CUSTOM,
        'not-a-key': RAW,
        [JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'bad'])]: { mode: 'inherit' },
        [JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'also-bad'])]: { mode: 'bogus' },
      },
    });
    expect(parsed.columnOverrides).toEqual({
      [KEY]: RAW,
      [JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'created_at'])]: CUSTOM,
    });
  });

  it('ignores a non-object columnOverrides field', () => {
    expect(
      parseStoredValuePresentationSettings({ version: 1, renderers: {}, columnOverrides: 'x' }),
    ).toEqual({ version: 1, renderers: {} });
  });

  it('does not parse columnOverrides for an unknown top-level version', () => {
    const parsed = parseStoredValuePresentationSettings({
      version: 99,
      renderers: {},
      columnOverrides: { [KEY]: RAW },
    });
    expect(parsed).toEqual({ version: 99, renderers: {} });
    expect(parsed.columnOverrides).toBeUndefined();
  });
});

describe('buildValuePresentationSnapshot: columnOverrides (issue-32 T10)', () => {
  const KEY = JSON.stringify(['conn-1', 'shop', 'public', 'orders', 'epoch_ms']);
  const RAW: ColumnPresentationOverride = { mode: 'raw' };

  it('fills snapshot overrides from stored columnOverrides', () => {
    const snapshot = buildValuePresentationSnapshot(
      {
        version: 1,
        renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
        columnOverrides: { [KEY]: RAW },
      },
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.overrides).toEqual({ [KEY]: RAW });
  });

  it('carries no overrides for an unknown top-level version', () => {
    const snapshot = buildValuePresentationSnapshot(
      {
        version: 99,
        renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
        columnOverrides: { [KEY]: RAW },
      } as unknown as StoredValuePresentationSettings,
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.overrides).toEqual({});
  });

  it('carries overrides even when the timestamp renderer is disabled (raw needs no switch)', () => {
    const snapshot = buildValuePresentationSnapshot(
      {
        version: 1,
        renderers: {},
        columnOverrides: { [KEY]: RAW },
      },
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.overrides).toEqual({ [KEY]: RAW });
    expect(snapshot.enabledRendererIds).toEqual([]);
  });
});

describe('buildValuePresentationSnapshot: rules (issue-32 T08)', () => {
  const RULE_INPUT = {
    name: 'unix-ms',
    enabled: true,
    match: { column: { kind: 'exact', value: 'created_at' } },
    options: UNIX_MS_SHANGHAI,
  };

  function storedWithRules(enabled: boolean): StoredValuePresentationSettings {
    return {
      version: 1,
      renderers: {
        [TIMESTAMP_CONTRIBUTION_ID]: { enabled, options: NATIVE_UTC_ISO },
      },
      rules: [RULE_INPUT] as unknown as StoredValuePresentationSettings['rules'],
    };
  }

  it('carries validated rules when the timestamp renderer is enabled', () => {
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules(true),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.rules.map((r) => r.name)).toEqual(['unix-ms']);
  });

  it('carries no rules when the timestamp renderer is disabled', () => {
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules(false),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.rules).toEqual([]);
  });

  it('carries no rules for an unknown top-level version', () => {
    const snapshot = buildValuePresentationSnapshot(
      { ...storedWithRules(true), version: 99 },
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.rules).toEqual([]);
  });
});
