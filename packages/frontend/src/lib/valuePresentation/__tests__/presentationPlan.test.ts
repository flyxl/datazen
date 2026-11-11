/**
 * presentationPlan（issue-32 T04）：宿主展示管线与列展示计划。
 *
 * 覆盖：语义种映射全表、columnOverrideKey 稳定性与大小写保留、默认路由、
 * enabled / config / registry 缺失三态、raw / custom / inherit 三态（含
 * custom 选项无效、版本不一致、bypass enabled）、prepare 抛异常隔离、
 * present 回退 reason、缓存键、空列与重名列、查询结果无 source 时覆盖
 * 不可用、PRD 锚点端到端与 BUG-003 不回归。
 */
import { describe, expect, it, vi } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  type ColumnPresentationOverride,
  type ColumnSource,
  type ConfigValidationResult,
  type PresentationConfigValue,
  type ValuePresentationContribution,
  type VersionedPresentationConfig,
} from '@datazen/extension-points';
import {
  buildColumnPresentationPlans,
  buildPresentationColumnContext,
  columnOverrideKey,
  computePresentationPlanCacheKey,
  createDefaultValuePresentationSnapshot,
  fallbackPresentationReason,
  resolvePresentationEnvironment,
  resolvePresentationSemanticKind,
  type ColumnPresentationPlan,
  type PresentationPlanColumn,
  type ValuePresentationSnapshot,
} from '../presentationPlan';
import { TIMESTAMP_CONTRIBUTION_ID, timestampContribution, type TimestampOptions } from '../index';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const ENV_SHANGHAI = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' } as const;

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const NATIVE_SHANGHAI_ISO_TZ: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const UNIX_MS_SHANGHAI_ISO_TZ: TimestampOptions = {
  interpretation: { kind: 'unix', unit: 'milliseconds' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

/**
 * TimestampOptions → VersionedPresentationConfig（逐字段重建为 JSON 值，
 * 不用断言跳过验证）。
 */
function timestampConfig(options: TimestampOptions): VersionedPresentationConfig {
  const interpretation: PresentationConfigValue =
    options.interpretation.kind === 'unix'
      ? { kind: 'unix', unit: options.interpretation.unit }
      : { kind: options.interpretation.kind };
  const displayTimeZone: PresentationConfigValue =
    options.displayTimeZone.kind === 'iana'
      ? { kind: 'iana', name: options.displayTimeZone.name }
      : { kind: options.displayTimeZone.kind };
  const config: { readonly [key: string]: PresentationConfigValue } = {
    interpretation,
    format: options.format,
    displayTimeZone,
  };
  if (options.sourceTimeZone !== undefined) {
    const sourceTimeZone: PresentationConfigValue =
      options.sourceTimeZone.kind === 'iana'
        ? { kind: 'iana', name: options.sourceTimeZone.name }
        : { kind: options.sourceTimeZone.kind };
    return {
      configVersion: 1,
      config: { ...config, sourceTimeZone },
    };
  }
  return { configVersion: 1, config };
}

interface SnapshotOptions {
  revision?: number;
  configs?: Readonly<Record<string, VersionedPresentationConfig>>;
  enabled?: ReadonlyArray<string>;
  overrides?: Readonly<Record<string, ColumnPresentationOverride>>;
}

function makeSnapshot(options: SnapshotOptions = {}): ValuePresentationSnapshot {
  return {
    revision: options.revision ?? 1,
    environment: ENV_SHANGHAI,
    configs: options.configs ?? {
      [TIMESTAMP_CONTRIBUTION_ID]: timestampConfig(NATIVE_SHANGHAI_ISO_TZ),
    },
    enabledRendererIds: options.enabled ?? [TIMESTAMP_CONTRIBUTION_ID],
    overrides: options.overrides ?? {},
  };
}

/** 全新 registry（不碰全局单例）；需要时注册真实 timestamp 贡献。 */
function makeRegistry(withTimestamp = true): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  if (withTimestamp) registry.register(timestampContribution);
  return registry;
}

function col(
  name: string,
  dataType?: string,
  cellType?: PresentationPlanColumn['cellType'],
  hints?: PresentationPlanColumn['hints'],
): PresentationPlanColumn {
  return { name, dataType, cellType, hints };
}

function planFor(
  columns: ReadonlyArray<PresentationPlanColumn>,
  snapshot: ValuePresentationSnapshot,
  registry: ValuePresentationRegistryImpl,
  source: Readonly<Omit<ColumnSource, 'column'>> | undefined = SOURCE,
): ColumnPresentationPlan {
  const plans = buildColumnPresentationPlans({ columns, source, snapshot }, registry);
  expect(plans).toHaveLength(1);
  const plan = plans[0];
  if (plan === undefined) throw new Error('expected one plan');
  return plan;
}

// ---------------------------------------------------------------------------
// 语义种映射全表
// ---------------------------------------------------------------------------

describe('resolvePresentationSemanticKind', () => {
  it.each([
    // datetime：带时区 → instant，不带 → local-datetime。
    ['timestamptz → instant', col('c', 'timestamptz'), 'instant'],
    ['timestamp with time zone → instant', col('c', 'timestamp with time zone'), 'instant'],
    ['datetimeoffset → instant', col('c', 'datetimeoffset'), 'instant'],
    ['timestamp → local-datetime', col('c', 'timestamp'), 'local-datetime'],
    ['datetime → local-datetime', col('c', 'datetime'), 'local-datetime'],
    [
      '显式 datetime + 无 dataType → local-datetime',
      col('c', undefined, 'datetime'),
      'local-datetime',
    ],
    ['显式 datetime + zoned dataType → instant', col('c', 'timestamptz', 'datetime'), 'instant'],
    // date / time。
    ['date → date', col('c', 'date'), 'date'],
    ['显式 date → date', col('c', undefined, 'date'), 'date'],
    ['time → time', col('c', 'time'), 'time'],
    ['显式 time → time', col('c', undefined, 'time'), 'time'],
    // number。
    ['bigint → number', col('c', 'bigint'), 'number'],
    ['integer → number', col('c', 'integer'), 'number'],
    ['decimal → number', col('c', 'decimal(10,2)'), 'number'],
    ['显式 number → number', col('c', undefined, 'number'), 'number'],
    // text。
    ['varchar → text', col('c', 'varchar(10)'), 'text'],
    ['text → text', col('c', 'text'), 'text'],
    ['显式 enum → text', col('c', undefined, 'enum'), 'text'],
    ['显式 uuid → text', col('c', 'uuid', 'uuid'), 'text'],
    // other。
    ['boolean → other', col('c', 'boolean'), 'other'],
    ['jsonb → other', col('c', 'jsonb'), 'other'],
    ['bytea → other', col('c', 'bytea'), 'other'],
    ['未知类型 → other', col('c', 'some-weird-type'), 'other'],
    ['无 dataType → other', col('c'), 'other'],
    ['显式 unknown → other', col('c', undefined, 'unknown'), 'other'],
  ] as const)('%s', (_label, column, expected) => {
    expect(resolvePresentationSemanticKind(column)).toBe(expected);
  });

  it('驱动 hints 决定 zoned：zonedTypes 命中 → instant', () => {
    expect(
      resolvePresentationSemanticKind(
        col('c', 'my_tz_type', 'datetime', { zonedTypes: ['my_tz_type'] }),
      ),
    ).toBe('instant');
  });

  it('驱动 overrides 优先：SQL Server rowversion 不是时间 → other', () => {
    expect(
      resolvePresentationSemanticKind(
        col('c', 'rowversion', undefined, {
          overrides: [{ match: 'exact', pattern: 'rowversion', cellType: 'bytes' }],
        }),
      ),
    ).toBe('other');
  });
});

describe('buildPresentationColumnContext', () => {
  it('组装列名、数据库类型、语义种与来源', () => {
    const context = buildPresentationColumnContext(col('created_at', 'timestamptz'), {
      ...SOURCE,
      column: 'created_at',
    });
    expect(context).toEqual({
      columnName: 'created_at',
      databaseTypeName: 'timestamptz',
      semanticKind: 'instant',
      source: { ...SOURCE, column: 'created_at' },
    });
  });

  it('无来源时 source 缺省', () => {
    const context = buildPresentationColumnContext(col('n', 'integer'));
    expect(context.source).toBeUndefined();
    expect(context.semanticKind).toBe('number');
  });
});

// ---------------------------------------------------------------------------
// columnOverrideKey
// ---------------------------------------------------------------------------

describe('columnOverrideKey', () => {
  const full: ColumnSource = {
    connectionId: 'conn-1',
    database: 'shop',
    namespace: 'public',
    table: 'Orders',
    column: 'Created_At',
  };

  it('稳定：同一输入两次键相同，且等于 JSON 数组字面量', () => {
    expect(columnOverrideKey(full)).toBe(columnOverrideKey(full));
    expect(columnOverrideKey(full)).toBe(
      JSON.stringify(['conn-1', 'shop', 'public', 'Orders', 'Created_At']),
    );
  });

  it('不做大小写归一：表名 / 列名大小写不同 → 键不同', () => {
    expect(columnOverrideKey({ ...full, table: 'orders' })).not.toBe(columnOverrideKey(full));
    expect(columnOverrideKey({ ...full, column: 'created_at' })).not.toBe(columnOverrideKey(full));
  });

  it('缺省字段与空字符串归一为同一键', () => {
    const noDatabase: ColumnSource = {
      connectionId: 'c',
      table: 't',
      column: 'x',
    };
    const emptyDatabase: ColumnSource = {
      connectionId: 'c',
      database: '',
      namespace: '',
      table: 't',
      column: 'x',
    };
    expect(columnOverrideKey(noDatabase)).toBe(columnOverrideKey(emptyDatabase));
  });
});

// ---------------------------------------------------------------------------
// 默认快照与环境解析
// ---------------------------------------------------------------------------

describe('createDefaultValuePresentationSnapshot / resolvePresentationEnvironment', () => {
  it('默认快照：revision 0、三空集合，时间列一律 disabled', () => {
    const snapshot = createDefaultValuePresentationSnapshot({
      locale: 'en-US',
      localTimeZone: 'Asia/Shanghai',
    });
    expect(snapshot.revision).toBe(0);
    expect(snapshot.configs).toEqual({});
    expect(snapshot.enabledRendererIds).toEqual([]);
    expect(snapshot.overrides).toEqual({});
    const plan = planFor([col('created_at', 'timestamptz')], snapshot, makeRegistry());
    expect(plan.status).toBe('disabled');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
  });

  it('默认快照不传 environment 时按当前环境解析', () => {
    const snapshot = createDefaultValuePresentationSnapshot();
    expect(snapshot.environment.locale).toBe('en-US');
    expect(typeof snapshot.environment.localTimeZone).toBe('string');
    expect(snapshot.environment.localTimeZone).not.toBe('');
  });

  it('resolvePresentationEnvironment：显式 locale 优先', () => {
    const env = resolvePresentationEnvironment('zh-CN');
    expect(env.locale).toBe('zh-CN');
    expect(typeof env.localTimeZone).toBe('string');
    expect(env.localTimeZone).not.toBe('');
  });

  it('resolvePresentationEnvironment：缺省取 navigator.language', () => {
    expect(resolvePresentationEnvironment().locale).toBe('en-US');
  });
});

// ---------------------------------------------------------------------------
// 默认路由
// ---------------------------------------------------------------------------

describe('buildColumnPresentationPlans: default routing', () => {
  it.each([
    ['instant', col('created_at', 'timestamptz')],
    ['local-datetime', col('created_at', 'timestamp')],
    ['date', col('d', 'date')],
    ['time', col('t', 'time')],
  ] as const)('时间语义列（%s）默认路由到 datazen.timestamp → ready', (_kind, column) => {
    const plan = planFor([column], makeSnapshot(), makeRegistry());
    expect(plan.status).toBe('ready');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.prepared).toBeDefined();
  });

  it.each([
    ['number', col('amount', 'bigint')],
    ['text', col('name', 'varchar(10)')],
    ['other', col('flag', 'boolean')],
    ['other', col('payload', 'jsonb')],
  ] as const)('无默认渲染器的语义（%s）→ default，present 回退 no-renderer', (_kind, column) => {
    const plan = planFor([column], makeSnapshot(), makeRegistry());
    expect(plan.status).toBe('default');
    expect(plan.rendererId).toBeUndefined();
    expect(plan.reason).toBe('no-renderer');
    expect(plan.present(42)).toEqual({ kind: 'unchanged', reason: 'no-renderer' });
  });
});

describe('buildColumnPresentationPlans: 缺失三态', () => {
  it('enabled 缺失 → disabled（即使 registry 与配置都在）', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      makeSnapshot({ enabled: [] }),
      makeRegistry(),
    );
    expect(plan.status).toBe('disabled');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.reason).toBe('disabled');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
  });

  it('registry 缺失 → missing-renderer（enabled 与配置都在）', () => {
    const plan = planFor([col('created_at', 'timestamptz')], makeSnapshot(), makeRegistry(false));
    expect(plan.status).toBe('missing-renderer');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.reason).toBe('missing-renderer');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'missing-renderer',
    });
  });

  it('配置缺失 → missing-config', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      makeSnapshot({ configs: {} }),
      makeRegistry(),
    );
    expect(plan.status).toBe('missing-config');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.reason).toBe('missing-config');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'missing-config',
    });
  });

  it('缺失检查顺序：disabled 优先于 missing-renderer', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      makeSnapshot({ enabled: [] }),
      makeRegistry(false),
    );
    expect(plan.status).toBe('disabled');
  });
});

// ---------------------------------------------------------------------------
// 覆盖三态：raw / custom / inherit
// ---------------------------------------------------------------------------

describe('buildColumnPresentationPlans: overrides', () => {
  function overrideSnapshot(
    mode: ColumnPresentationOverride,
    columnName = 'created_at',
  ): ValuePresentationSnapshot {
    return makeSnapshot({
      overrides: {
        [columnOverrideKey({ ...SOURCE, column: columnName })]: mode,
      },
    });
  }

  it('raw → raw，不调 prepare（prepare 会抛的贡献也不会被调用）', () => {
    const prepare = vi.fn((): never => {
      throw new Error('prepare must not be called for raw');
    });
    const throwing: ValuePresentationContribution = {
      id: TIMESTAMP_CONTRIBUTION_ID,
      configVersion: 1,
      validateConfig: (): ConfigValidationResult => ({ ok: true, config: {} }),
      prepare,
    };
    const registry = new ValuePresentationRegistryImpl();
    registry.register(throwing);
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      overrideSnapshot({ mode: 'raw' }),
      registry,
    );
    expect(plan.status).toBe('raw');
    expect(plan.reason).toBe('raw-override');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'raw-override',
    });
    expect(prepare).not.toHaveBeenCalled();
  });

  it('custom 有效 → ready，且 bypass enabled 门禁（enabled 为空仍 ready）', () => {
    const snapshot = overrideSnapshot(
      {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: UNIX_MS_SHANGHAI_ISO_TZ,
      },
      'event_ms',
    );
    // 显式把 enabled 清空：custom 不应受全局开关影响。
    const disabledSnapshot: ValuePresentationSnapshot = {
      ...snapshot,
      enabledRendererIds: [],
    };
    const plan = planFor([col('event_ms', 'bigint')], disabledSnapshot, makeRegistry());
    expect(plan.status).toBe('ready');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it('custom 目标渲染器缺失 → missing-renderer', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      overrideSnapshot({
        mode: 'custom',
        rendererId: 'datazen.nope',
        options: NATIVE_SHANGHAI_ISO_TZ,
      }),
      makeRegistry(),
    );
    expect(plan.status).toBe('missing-renderer');
    expect(plan.rendererId).toBe('datazen.nope');
    expect(plan.reason).toBe('missing-renderer');
  });

  it('custom 选项无效 → invalid-config', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      overrideSnapshot({
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'bogus' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      }),
      makeRegistry(),
    );
    expect(plan.status).toBe('invalid-config');
    expect(plan.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(plan.reason).toBe('invalid-config');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });

  it('inherit → 走默认路由（enabled + 配置齐全时 ready）', () => {
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      overrideSnapshot({ mode: 'inherit' }),
      makeRegistry(),
    );
    expect(plan.status).toBe('ready');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('inherit + enabled 为空 → disabled（inherit 不绕过门禁）', () => {
    const snapshot = overrideSnapshot({ mode: 'inherit' });
    const plan = planFor(
      [col('created_at', 'timestamptz')],
      { ...snapshot, enabledRendererIds: [] },
      makeRegistry(),
    );
    expect(plan.status).toBe('disabled');
  });

  it('版本不一致 → invalid-config，且不调 prepare', () => {
    const prepare = vi.fn((): never => {
      throw new Error('prepare must not be called on version mismatch');
    });
    const throwing: ValuePresentationContribution = {
      id: TIMESTAMP_CONTRIBUTION_ID,
      configVersion: 1,
      validateConfig: (): ConfigValidationResult => ({ ok: true, config: {} }),
      prepare,
    };
    const registry = new ValuePresentationRegistryImpl();
    registry.register(throwing);
    const snapshot = makeSnapshot({
      configs: {
        [TIMESTAMP_CONTRIBUTION_ID]: {
          configVersion: 99,
          config: timestampConfig(NATIVE_SHANGHAI_ISO_TZ).config,
        },
      },
    });
    const plan = planFor([col('created_at', 'timestamptz')], snapshot, registry);
    expect(plan.status).toBe('invalid-config');
    expect(plan.reason).toBe('invalid-config');
    expect(prepare).not.toHaveBeenCalled();
  });

  it('全局配置体无效 → invalid-config（prepare 仍被调用，由 T03 兜底；宿主只认版本与存在性）', () => {
    // 宿主层：configs[rendererId] 存在且版本一致即调 prepare；配置体
    // 的语义校验在贡献 prepare 内完成（T03 返回 invalid-config 回退）。
    const snapshot = makeSnapshot({
      configs: {
        [TIMESTAMP_CONTRIBUTION_ID]: {
          configVersion: 1,
          config: {
            interpretation: { kind: 'bogus' },
            format: 'iso-tz',
            displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
          },
        },
      },
    });
    const plan = planFor([col('created_at', 'timestamptz')], snapshot, makeRegistry());
    expect(plan.status).toBe('ready');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'invalid-config',
    });
  });
});

// ---------------------------------------------------------------------------
// prepare 抛异常隔离
// ---------------------------------------------------------------------------

describe('buildColumnPresentationPlans: prepare throws', () => {
  it('prepare 抛异常 → error / plan-error，present 不含原始行值', () => {
    const prepare = vi.fn((): never => {
      throw new Error('boom');
    });
    const throwing: ValuePresentationContribution = {
      id: TIMESTAMP_CONTRIBUTION_ID,
      configVersion: 1,
      validateConfig: (): ConfigValidationResult => ({ ok: true, config: {} }),
      prepare,
    };
    const registry = new ValuePresentationRegistryImpl();
    registry.register(throwing);
    const plan = planFor([col('created_at', 'timestamptz')], makeSnapshot(), registry);
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(plan.status).toBe('error');
    expect(plan.reason).toBe('plan-error');
    const result = plan.present('2026-01-15T14:30:00Z');
    expect(result).toEqual({ kind: 'unchanged', reason: 'plan-error' });
    expect(JSON.stringify(result)).not.toContain('2026-01-15');
  });
});

// ---------------------------------------------------------------------------
// present 回退 reason 全表
// ---------------------------------------------------------------------------

describe('fallbackPresentationReason', () => {
  it.each([
    ['default', 'no-renderer'],
    ['raw', 'raw-override'],
    ['disabled', 'disabled'],
    ['missing-renderer', 'missing-renderer'],
    ['missing-config', 'missing-config'],
    ['invalid-config', 'invalid-config'],
    ['error', 'plan-error'],
  ] as const)('%s → %s', (status, reason) => {
    expect(fallbackPresentationReason(status)).toBe(reason);
  });
});

// ---------------------------------------------------------------------------
// 缓存键
// ---------------------------------------------------------------------------

describe('computePresentationPlanCacheKey', () => {
  const base = {
    source: SOURCE,
    columns: [col('created_at', 'timestamptz'), col('amount', 'bigint')] as const,
    configRevision: 3,
    contributionRevision: 5,
    locale: 'en-US',
    localTimeZone: 'Asia/Shanghai',
  };

  it('相同输入键稳定', () => {
    expect(computePresentationPlanCacheKey(base)).toBe(computePresentationPlanCacheKey(base));
    expect(computePresentationPlanCacheKey(base)).toBe(
      computePresentationPlanCacheKey({
        source: { ...SOURCE },
        columns: [col('created_at', 'timestamptz'), col('amount', 'bigint')],
        configRevision: 3,
        contributionRevision: 5,
        locale: 'en-US',
        localTimeZone: 'Asia/Shanghai',
      }),
    );
  });

  it('任一因子变化键变化', () => {
    const key = computePresentationPlanCacheKey(base);
    const variants = [
      { ...base, source: { ...SOURCE, connectionId: 'conn-2' } },
      { ...base, source: { ...SOURCE, table: 'items' } },
      { ...base, source: undefined },
      { ...base, columns: [col('created_at', 'timestamptz'), col('amount', 'integer')] },
      { ...base, columns: [col('created_at', 'timestamp'), col('amount', 'bigint')] },
      { ...base, columns: [col('other', 'timestamptz'), col('amount', 'bigint')] },
      { ...base, configRevision: 4 },
      { ...base, contributionRevision: 6 },
      { ...base, locale: 'zh-CN' },
      { ...base, localTimeZone: 'UTC' },
      { ...base, columnsRevision: 7 },
      { ...base, columnsRevision: 'v2' },
    ];
    for (const variant of variants) {
      expect(computePresentationPlanCacheKey(variant)).not.toBe(key);
    }
  });

  it('列顺序变化键变化', () => {
    expect(
      computePresentationPlanCacheKey({
        ...base,
        columns: [col('amount', 'bigint'), col('created_at', 'timestamptz')],
      }),
    ).not.toBe(computePresentationPlanCacheKey(base));
  });

  it('columnsRevision 相同则键相同', () => {
    expect(computePresentationPlanCacheKey({ ...base, columnsRevision: 7 })).toBe(
      computePresentationPlanCacheKey({ ...base, columnsRevision: 7 }),
    );
    expect(computePresentationPlanCacheKey({ ...base, columnsRevision: 7 })).not.toBe(
      computePresentationPlanCacheKey({ ...base, columnsRevision: 8 }),
    );
  });
});

// ---------------------------------------------------------------------------
// 空列、重名列、无来源列
// ---------------------------------------------------------------------------

describe('buildColumnPresentationPlans: shape', () => {
  it('输入 columns 为空返回空数组', () => {
    expect(
      buildColumnPresentationPlans({ columns: [], snapshot: makeSnapshot() }, makeRegistry()),
    ).toEqual([]);
  });

  it('同一列名出现两次时两项独立成计划（不去重、不抛错）', () => {
    const plans = buildColumnPresentationPlans(
      {
        columns: [col('created_at', 'timestamptz'), col('created_at', 'timestamptz')],
        source: SOURCE,
        snapshot: makeSnapshot(),
      },
      makeRegistry(),
    );
    expect(plans).toHaveLength(2);
    expect(plans[0]?.status).toBe('ready');
    expect(plans[1]?.status).toBe('ready');
    expect(plans[0]).not.toBe(plans[1]);
    expect(plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    expect(plans[1]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('计划按输入 columns 顺序返回', () => {
    const plans = buildColumnPresentationPlans(
      {
        columns: [col('amount', 'bigint'), col('created_at', 'timestamptz'), col('name', 'text')],
        source: SOURCE,
        snapshot: makeSnapshot(),
      },
      makeRegistry(),
    );
    expect(plans.map((plan) => plan.columnName)).toEqual(['amount', 'created_at', 'name']);
    expect(plans.map((plan) => plan.status)).toEqual(['default', 'ready', 'default']);
  });

  it('查询结果无 source 时覆盖不可用（raw 覆盖不生效，走默认路由）', () => {
    const snapshot = makeSnapshot({
      overrides: {
        [columnOverrideKey({ ...SOURCE, column: 'created_at' })]: { mode: 'raw' },
      },
    });
    const plans = buildColumnPresentationPlans(
      { columns: [col('created_at', 'timestamptz')], snapshot },
      makeRegistry(),
    );
    expect(plans).toHaveLength(1);
    expect(plans[0]?.status).toBe('ready');
    expect(plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });
});

// ---------------------------------------------------------------------------
// PRD 锚点端到端 + BUG-003 不回归
// ---------------------------------------------------------------------------

describe('PRD anchors end-to-end (via plan.present)', () => {
  it('2026-01-15T14:30:00Z + Asia/Shanghai → iso-tz 含 22:30 与 +08:00', () => {
    const plan = planFor([col('created_at', 'timestamptz')], makeSnapshot(), makeRegistry());
    expect(plan.status).toBe('ready');
    const result = plan.present('2026-01-15T14:30:00Z');
    expect(result).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    if (result.kind === 'text') {
      expect(result.text).toContain('22:30');
      expect(result.text).toContain('+08:00');
    }
  });

  it('Unix 1705316400000（毫秒）→ 2024-01-15T19:00:00+08:00（全局 unix 配置，datetime 列）', () => {
    const snapshot = makeSnapshot({
      configs: {
        [TIMESTAMP_CONTRIBUTION_ID]: timestampConfig(UNIX_MS_SHANGHAI_ISO_TZ),
      },
    });
    const plan = planFor([col('created_at', 'timestamp')], snapshot, makeRegistry());
    expect(plan.status).toBe('ready');
    expect(plan.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it('BUG-003 不回归：Unix 1705316400123 经 plan.present 含 .123（BIGINT 列 custom 覆盖）', () => {
    const snapshot = makeSnapshot({
      enabled: [],
      overrides: {
        [columnOverrideKey({ ...SOURCE, column: 'event_ms' })]: {
          mode: 'custom',
          rendererId: TIMESTAMP_CONTRIBUTION_ID,
          options: UNIX_MS_SHANGHAI_ISO_TZ,
        },
      },
    });
    const plan = planFor([col('event_ms', 'bigint')], snapshot, makeRegistry());
    expect(plan.status).toBe('ready');
    expect(plan.present(1705316400123)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00.123+08:00',
      interpretation: 'unix',
    });
    expect(plan.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });
});
