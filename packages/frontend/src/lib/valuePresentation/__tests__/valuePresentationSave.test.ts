/**
 * valuePresentationSave（issue-32 T11）：路径化保存更新。
 *
 * 覆盖：整份对象原样返回、updater 作用于 fresh、withTimestampRenderer
 * 只换时间渲染器一项（其余字段取 fresh 原文、未知版本字段不丢）、
 * withValuePresentationRules 只在 version 1 写规则、两路并发合并
 * （A 改格式 + B 改规则/覆盖后最终一致且无字段丢失）。
 */
import { describe, expect, it } from 'vitest';
import {
  applyValuePresentationUpdate,
  columnOverrideKey,
  withColumnOverride,
  withTimestampRenderer,
  withValuePresentationRules,
  type StoredValuePresentationSettings,
  type TimestampOptions,
  type ValuePresentationRule,
} from '../index';

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

const NATIVE_ISO_TZ_SHANGHAI: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const RULE: ValuePresentationRule = {
  name: 'unix-ms',
  enabled: true,
  match: { column: { kind: 'exact', value: 'epoch_ms' } },
  options: {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  },
};

const SOURCE = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
} as const;

function storedWithRuleAndOverride(): StoredValuePresentationSettings {
  return {
    version: 1,
    renderers: {
      'datazen.timestamp': { enabled: true, options: NATIVE_UTC_ISO },
    },
    rules: [RULE],
    columnOverrides: {
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
    },
  };
}

describe('applyValuePresentationUpdate', () => {
  it('returns a whole object as-is', () => {
    const fresh = storedWithRuleAndOverride();
    const next: StoredValuePresentationSettings = {
      version: 1,
      renderers: { 'datazen.timestamp': { enabled: false, options: NATIVE_UTC_ISO } },
    };
    expect(applyValuePresentationUpdate(fresh, next)).toBe(next);
  });

  it('applies an updater to the fresh value', () => {
    const fresh = storedWithRuleAndOverride();
    const next = applyValuePresentationUpdate(fresh, (value) => ({
      ...value,
      renderers: {},
    }));
    expect(next.renderers).toEqual({});
    expect(next.rules).toEqual([RULE]);
    expect(fresh.renderers['datazen.timestamp']).toBeDefined();
  });

  it('an updater returning fresh unchanged is a no-op save input', () => {
    const fresh = storedWithRuleAndOverride();
    expect(applyValuePresentationUpdate(fresh, (value) => value)).toBe(fresh);
  });
});

describe('withTimestampRenderer', () => {
  it('replaces only the timestamp renderer, preserving rules and overrides', () => {
    const fresh = storedWithRuleAndOverride();
    const next = withTimestampRenderer(fresh, true, NATIVE_ISO_TZ_SHANGHAI);
    expect(next.renderers['datazen.timestamp']).toEqual({
      enabled: true,
      options: NATIVE_ISO_TZ_SHANGHAI,
    });
    expect(next.rules).toEqual([RULE]);
    expect(next.columnOverrides).toEqual(fresh.columnOverrides);
    expect(next.version).toBe(1);
    // 原值不动。
    expect(fresh.renderers['datazen.timestamp']?.options).toEqual(NATIVE_UTC_ISO);
  });

  it('preserves other renderer entries', () => {
    const fresh: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'other.renderer': { enabled: true, options: NATIVE_UTC_ISO },
      },
    };
    const next = withTimestampRenderer(fresh, false, NATIVE_UTC_ISO);
    expect(next.renderers['other.renderer']).toEqual({
      enabled: true,
      options: NATIVE_UTC_ISO,
    });
    expect(next.renderers['datazen.timestamp']).toEqual({
      enabled: false,
      options: NATIVE_UTC_ISO,
    });
  });

  it('preserves unknown-version fields instead of writing back defaults', () => {
    const fresh = {
      version: 99,
      renderers: {},
      futureField: { kept: true },
    } as unknown as StoredValuePresentationSettings;
    const next = withTimestampRenderer(fresh, true, NATIVE_UTC_ISO);
    expect(next.version).toBe(99);
    expect(next.renderers['datazen.timestamp']).toEqual({
      enabled: true,
      options: NATIVE_UTC_ISO,
    });
    expect(next as unknown as Record<string, unknown>).toMatchObject({
      futureField: { kept: true },
    });
  });
});

describe('withValuePresentationRules', () => {
  it('replaces the rule list on version 1', () => {
    const fresh = storedWithRuleAndOverride();
    const next = withValuePresentationRules(fresh, []);
    expect(next.rules).toEqual([]);
    expect(next.renderers).toEqual(fresh.renderers);
    expect(next.columnOverrides).toEqual(fresh.columnOverrides);
  });

  it('returns fresh unchanged for an unknown version', () => {
    const fresh = {
      version: 99,
      renderers: { 'datazen.timestamp': { enabled: true, options: NATIVE_UTC_ISO } },
      rules: [{ name: 'future-rule' }],
    } as unknown as StoredValuePresentationSettings;
    expect(withValuePresentationRules(fresh, [])).toBe(fresh);
  });
});

describe('concurrent saves merge by path (T11)', () => {
  it('A changes the global format while B changes rules and overrides: neither loses fields', () => {
    // B 窗已保存的权威值：规则 + 列覆盖 + 旧格式。
    const authoritative = storedWithRuleAndOverride();
    // A 窗只改全局格式：updater 作用于权威值。
    const afterA = applyValuePresentationUpdate(authoritative, (fresh) =>
      withTimestampRenderer(fresh, true, NATIVE_ISO_TZ_SHANGHAI),
    );
    expect(afterA.renderers['datazen.timestamp']?.options).toEqual(NATIVE_ISO_TZ_SHANGHAI);
    expect(afterA.rules).toEqual([RULE]);
    expect(afterA.columnOverrides).toEqual(authoritative.columnOverrides);

    // B 窗随后保存一列覆盖：同样作用于（已含 A 格式的）权威值。
    const afterB = applyValuePresentationUpdate(afterA, (fresh) =>
      withColumnOverride(fresh, SOURCE, 'created_at', {
        mode: 'custom',
        rendererId: 'datazen.timestamp',
        options: {
          interpretation: { kind: 'unix', unit: 'seconds' },
          format: 'iso',
          displayTimeZone: { kind: 'utc' },
        },
      }),
    );
    expect(afterB.renderers['datazen.timestamp']?.options).toEqual(NATIVE_ISO_TZ_SHANGHAI);
    expect(afterB.rules).toEqual([RULE]);
    expect(afterB.columnOverrides?.[columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]).toEqual({
      mode: 'raw',
    });
    expect(
      afterB.columnOverrides?.[columnOverrideKey({ ...SOURCE, column: 'created_at' })],
    ).toEqual({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        interpretation: { kind: 'unix', unit: 'seconds' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
  });
});
