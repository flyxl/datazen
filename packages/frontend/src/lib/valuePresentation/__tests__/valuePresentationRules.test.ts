/**
 * valuePresentationRules（issue-32 T08）：规则模型、受限正则与匹配。
 *
 * 覆盖：规则验证（名/开关/条件/选项、未知字段、空条件拒绝、名内
 * 空白拒绝）；受限正则子集（合法 pattern、非法/复杂/回溯风险表达
 * 式拒绝、flags 与长度上限）；匹配（AND、连接/数据库/表/列/类型名、
 * 缺失来源不满足表级条件、禁用规则不参与、首条命中冻结）；数量
 * 上限；快照携带与计划应用；端到端 PRD 例（created_at/updated_at
 * → Unix 毫秒 → iso-tz → Asia/Shanghai → 2024-01-15T19:00:00+08:00）。
 */
import { describe, expect, it } from 'vitest';
import { ValuePresentationRegistryImpl } from '@datazen/extension-points';
import {
  buildColumnPresentationPlans,
  timestampContribution,
  TIMESTAMP_CONTRIBUTION_ID,
  type TimestampOptions,
  type ValuePresentationRule,
} from '../index';
import { buildValuePresentationSnapshot, type StoredValuePresentationSettings } from '../settings';
import {
  MAX_PATTERN_LENGTH,
  MAX_VALUE_PRESENTATION_RULES,
  describeNamePattern,
  describeRuleMatch,
  findDuplicateRuleName,
  findFirstMatchingRule,
  matchNamePattern,
  matchValuePresentationRule,
  validateRestrictedPattern,
  validateValuePresentationRule,
  validateValuePresentationRules,
  type ValuePresentationRuleContext,
} from '../valuePresentationRules';

const ENV = { locale: 'en-US', localTimeZone: 'UTC' } as const;

const UNIX_MS_SHANGHAI: TimestampOptions = {
  interpretation: { kind: 'unix', unit: 'milliseconds' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

/** PRD §3.1 例：BIGINT 且列名匹配 ^(created_at|updated_at)$ → Unix 毫秒。 */
const PRD_RULE_INPUT = {
  name: 'unix-ms-timestamps',
  enabled: true,
  match: {
    column: { kind: 'regex', source: '^(created_at|updated_at)$' },
    databaseTypeName: { kind: 'exact', value: 'BIGINT' },
  },
  options: UNIX_MS_SHANGHAI,
};

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function ruleWith(
  patch: Partial<ValuePresentationRule> & { match?: ValuePresentationRule['match'] },
): ValuePresentationRule {
  const validated = validateValuePresentationRule({
    ...PRD_RULE_INPUT,
    ...patch,
    match: patch.match ?? PRD_RULE_INPUT.match,
    options: patch.options ?? PRD_RULE_INPUT.options,
  });
  if (!validated.ok) throw new Error(`test fixture invalid: ${JSON.stringify(validated.problems)}`);
  return validated.rule;
}

describe('validateRestrictedPattern', () => {
  it.each([
    '^(created_at|updated_at)$',
    'created_at',
    '\\d+',
    '[a-z_]+',
    'a{2,3}',
    'a+?',
    'a*?',
    '(?:ab)c',
    '.*',
    '.+',
    'col_[0-9]{2}',
    '^\\w+$',
    'a{0,}',
    '{,3}',
    'price|total',
  ])('accepts %s', (source) => {
    expect(validateRestrictedPattern(source)).toEqual({ ok: true });
  });

  it('accepts the i flag only', () => {
    expect(validateRestrictedPattern('created_at', 'i')).toEqual({ ok: true });
    expect(validateRestrictedPattern('created_at', '')).toEqual({ ok: true });
    expect(validateRestrictedPattern('created_at', 'g')).toEqual({
      ok: false,
      reason: 'only the "i" flag is allowed',
    });
    expect(validateRestrictedPattern('created_at', 'm')).toEqual({
      ok: false,
      reason: 'only the "i" flag is allowed',
    });
  });

  it.each([
    ['\\1', 'backreference'],
    ['(a)\\k<x>', 'backreference'],
    ['(?=x)a', 'lookahead'],
    ['(?!x)a', 'lookahead'],
    ['(?<=x)a', 'lookbehind'],
    ['(?<!x)a', 'lookbehind'],
    ['(?P<x>a)', 'lookaround'],
    ['\\p{L}+', 'unsupported escape'],
    ['\\u0041', 'unsupported escape'],
    ['\\x41', 'unsupported escape'],
    ['(a+)+', 'nested quantifier'],
    ['(a*)*', 'nested quantifier'],
    ['(a|b)+', 'alternation'],
    ['(a|ab)+c', 'alternation'],
    ['((a|b)c)+', 'alternation'],
    ['(a|)+', 'alternation'],
    ['a**', 'nested quantifier'],
    ['a+*', 'nested quantifier'],
    ['a{101}', 'repetition count'],
    ['a{3,2}', 'repetition minimum'],
    ['a{2}{3}', 'repetition without a preceding atom'],
    ['{3}', 'repetition without a preceding atom'],
    ['(abc', 'unclosed group'],
    ['[abc', 'unclosed character class'],
    ['abc\\', 'trailing backslash'],
    ['', 'empty'],
  ])('rejects %s (%s)', (source) => {
    expect(validateRestrictedPattern(source).ok).toBe(false);
  });

  it('rejects patterns over the length limit', () => {
    const long = `a{${'1'.repeat(MAX_PATTERN_LENGTH)}}`;
    expect(long.length).toBeGreaterThan(MAX_PATTERN_LENGTH);
    expect(validateRestrictedPattern(long).ok).toBe(false);
    expect(validateRestrictedPattern('a'.repeat(MAX_PATTERN_LENGTH)).ok).toBe(true);
    expect(validateRestrictedPattern('a'.repeat(MAX_PATTERN_LENGTH + 1)).ok).toBe(false);
  });
});

describe('matchNamePattern', () => {
  it('matches exact values by equality', () => {
    expect(matchNamePattern({ kind: 'exact', value: 'orders' }, 'orders')).toBe(true);
    expect(matchNamePattern({ kind: 'exact', value: 'orders' }, 'Orders')).toBe(false);
    expect(matchNamePattern({ kind: 'exact', value: 'orders' }, 'orders_2')).toBe(false);
  });

  it('matches regex as a substring match without implicit anchors', () => {
    expect(matchNamePattern({ kind: 'regex', source: 'created' }, 'created_at')).toBe(true);
    expect(matchNamePattern({ kind: 'regex', source: '^created' }, 'created_at')).toBe(true);
    expect(matchNamePattern({ kind: 'regex', source: '^created$' }, 'created_at')).toBe(false);
    expect(
      matchNamePattern({ kind: 'regex', source: '^(created_at|updated_at)$' }, 'updated_at'),
    ).toBe(true);
    expect(
      matchNamePattern({ kind: 'regex', source: '^(created_at|updated_at)$' }, 'deleted_at'),
    ).toBe(false);
  });

  it('honours the i flag', () => {
    expect(
      matchNamePattern({ kind: 'regex', source: 'created_at', flags: 'i' }, 'CREATED_AT'),
    ).toBe(true);
    expect(matchNamePattern({ kind: 'regex', source: 'created_at' }, 'CREATED_AT')).toBe(false);
  });

  it('returns false instead of throwing for an uncompilable pattern', () => {
    expect(matchNamePattern({ kind: 'regex', source: '(a+' }, 'a')).toBe(false);
  });
});

describe('validateValuePresentationRule', () => {
  it('accepts the PRD example rule and trims the name', () => {
    const validated = validateValuePresentationRule({
      ...PRD_RULE_INPUT,
      name: '  unix-ms-timestamps  ',
    });
    expect(validated.ok).toBe(true);
    if (!validated.ok) return;
    expect(validated.rule.name).toBe('unix-ms-timestamps');
    expect(validated.rule.enabled).toBe(true);
    expect(validated.rule.options).toEqual(UNIX_MS_SHANGHAI);
    expect(Object.isFrozen(validated.rule)).toBe(true);
  });

  it('rejects an empty, whitespace-containing or over-long name', () => {
    for (const name of ['', '   ', 'has space', 'has\ttab', 'x'.repeat(65)]) {
      const validated = validateValuePresentationRule({ ...PRD_RULE_INPUT, name });
      expect(validated.ok).toBe(false);
      if (!validated.ok) {
        expect(validated.problems.some((p) => p.path === 'name')).toBe(true);
      }
    }
  });

  it('rejects a non-boolean enabled flag and unknown top-level fields', () => {
    expect(validateValuePresentationRule({ ...PRD_RULE_INPUT, enabled: 'yes' }).ok).toBe(false);
    const withExtra = validateValuePresentationRule({ ...PRD_RULE_INPUT, priority: 1 });
    expect(withExtra.ok).toBe(false);
    if (!withExtra.ok) {
      expect(withExtra.problems.some((p) => p.path === 'priority')).toBe(true);
    }
  });

  it('rejects a rule with no match condition', () => {
    const validated = validateValuePresentationRule({ ...PRD_RULE_INPUT, match: {} });
    expect(validated.ok).toBe(false);
    if (!validated.ok) {
      expect(validated.problems.some((p) => p.path === 'match')).toBe(true);
    }
  });

  it('rejects an empty exact value and an unknown match field', () => {
    expect(
      validateValuePresentationRule({
        ...PRD_RULE_INPUT,
        match: { column: { kind: 'exact', value: '  ' } },
      }).ok,
    ).toBe(false);
    const withExtra = validateValuePresentationRule({
      ...PRD_RULE_INPUT,
      match: { ...PRD_RULE_INPUT.match, owner: 'me' },
    });
    expect(withExtra.ok).toBe(false);
    if (!withExtra.ok) {
      expect(withExtra.problems.some((p) => p.path === 'match.owner')).toBe(true);
    }
  });

  it('rejects an invalid regex and an invalid TimestampOptions', () => {
    expect(
      validateValuePresentationRule({
        ...PRD_RULE_INPUT,
        match: { column: { kind: 'regex', source: '(a+)+' } },
      }).ok,
    ).toBe(false);
    // Unix 解释不许带 sourceTimeZone（T02 约束）。
    expect(
      validateValuePresentationRule({
        ...PRD_RULE_INPUT,
        options: { ...UNIX_MS_SHANGHAI, sourceTimeZone: { kind: 'utc' } },
      }).ok,
    ).toBe(false);
    // format 非法。
    expect(
      validateValuePresentationRule({
        ...PRD_RULE_INPUT,
        options: { ...UNIX_MS_SHANGHAI, format: 'rfc3339' },
      }).ok,
    ).toBe(false);
  });

  it('rejects a non-object input', () => {
    expect(validateValuePresentationRule(null).ok).toBe(false);
    expect(validateValuePresentationRule('rule').ok).toBe(false);
  });
});

describe('validateValuePresentationRules', () => {
  it('returns [] for non-array input and drops invalid entries', () => {
    expect(validateValuePresentationRules(undefined)).toEqual([]);
    expect(validateValuePresentationRules('x')).toEqual([]);
    const rules = validateValuePresentationRules([
      PRD_RULE_INPUT,
      { ...PRD_RULE_INPUT, name: '' },
      'not-a-rule',
      { ...PRD_RULE_INPUT, name: 'second', match: { table: { kind: 'exact', value: 't' } } },
    ]);
    expect(rules.map((r) => r.name)).toEqual(['unix-ms-timestamps', 'second']);
  });

  it('caps the list at MAX_VALUE_PRESENTATION_RULES', () => {
    const many = Array.from({ length: MAX_VALUE_PRESENTATION_RULES + 5 }, (_, i) => ({
      ...PRD_RULE_INPUT,
      name: `rule-${i}`,
    }));
    expect(validateValuePresentationRules(many)).toHaveLength(MAX_VALUE_PRESENTATION_RULES);
  });
});

describe('findDuplicateRuleName / describe helpers', () => {
  it('reports a duplicate name and ignores the edited index', () => {
    const rules = [ruleWith({ name: 'a' }), ruleWith({ name: 'b' }), ruleWith({ name: 'a' })];
    expect(findDuplicateRuleName(rules)).toBe('a');
    expect(findDuplicateRuleName(rules, 2)).toBeUndefined();
    expect(
      findDuplicateRuleName([ruleWith({ name: 'a' }), ruleWith({ name: 'b' })]),
    ).toBeUndefined();
  });

  it('describes patterns and matches', () => {
    expect(describeNamePattern({ kind: 'exact', value: 'BIGINT' })).toBe('BIGINT');
    expect(describeNamePattern({ kind: 'regex', source: '^a$', flags: 'i' })).toBe('/^a$/i');
    expect(describeNamePattern({ kind: 'regex', source: '^a$' })).toBe('/^a$/');
    expect(
      describeRuleMatch({
        connectionId: 'conn-1',
        table: { kind: 'exact', value: 'orders' },
        column: { kind: 'regex', source: '^created_at$' },
      }),
    ).toBe('connection=conn-1, table=orders, column=/^created_at$/');
  });
});

describe('matchValuePresentationRule', () => {
  const context: ValuePresentationRuleContext = {
    connectionId: 'conn-1',
    database: 'shop',
    namespace: 'public',
    table: 'orders',
    column: 'created_at',
    databaseTypeName: 'BIGINT',
  };

  it('matches when every condition holds (AND)', () => {
    const rule = ruleWith({
      match: {
        connectionId: 'conn-1',
        database: 'shop',
        namespace: 'public',
        table: { kind: 'exact', value: 'orders' },
        column: { kind: 'regex', source: '^created_at$' },
        databaseTypeName: { kind: 'exact', value: 'BIGINT' },
      },
    });
    expect(matchValuePresentationRule(rule, context)).toBe(true);
  });

  it('fails when any single condition fails', () => {
    const variants: Array<ValuePresentationRule['match']> = [
      { connectionId: 'conn-2' },
      { database: 'other' },
      { namespace: 'private' },
      { table: { kind: 'exact', value: 'customers' } },
      { column: { kind: 'exact', value: 'updated_at' } },
      { databaseTypeName: { kind: 'exact', value: 'VARCHAR' } },
      { column: { kind: 'regex', source: '^updated_at$' } },
      { databaseTypeName: { kind: 'regex', source: '^INT$' } },
    ];
    for (const match of variants) {
      expect(matchValuePresentationRule(ruleWith({ match }), context)).toBe(false);
    }
  });

  it('requires a source for table-level conditions (no guessing from aliases)', () => {
    const noSource: ValuePresentationRuleContext = {
      column: 'created_at',
      databaseTypeName: 'BIGINT',
    };
    // 表级条件（表名）因缺失来源而不满足。
    expect(
      matchValuePresentationRule(
        ruleWith({ match: { table: { kind: 'exact', value: 'orders' } } }),
        noSource,
      ),
    ).toBe(false);
    expect(
      matchValuePresentationRule(ruleWith({ match: { connectionId: 'conn-1' } }), noSource),
    ).toBe(false);
    // 仅列名/类型名条件仍可命中（查询结果列）。
    expect(matchValuePresentationRule(ruleWith({}), noSource)).toBe(true);
  });

  it('matches database type names case-insensitively only with the i flag', () => {
    const rule = ruleWith({
      match: { databaseTypeName: { kind: 'regex', source: '^bigint$' } },
    });
    expect(matchValuePresentationRule(rule, context)).toBe(false);
    const ruleI = ruleWith({
      match: { databaseTypeName: { kind: 'regex', source: '^bigint$', flags: 'i' } },
    });
    expect(matchValuePresentationRule(ruleI, context)).toBe(true);
  });
});

describe('findFirstMatchingRule', () => {
  const context: ValuePresentationRuleContext = {
    connectionId: 'conn-1',
    table: 'orders',
    column: 'created_at',
    databaseTypeName: 'BIGINT',
  };

  it('skips disabled rules and returns the first match', () => {
    const disabled = ruleWith({
      name: 'disabled-first',
      enabled: false,
      match: { column: { kind: 'exact', value: 'created_at' } },
    });
    const first = ruleWith({ name: 'first' });
    const second = ruleWith({
      name: 'second',
      options: {
        interpretation: { kind: 'unix', unit: 'seconds' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
    expect(findFirstMatchingRule([disabled, first, second], context)).toBe(first);
    expect(findFirstMatchingRule([disabled], context)).toBeUndefined();
    expect(findFirstMatchingRule([], context)).toBeUndefined();
  });
});

describe('rules in snapshot and plan (end to end)', () => {
  function storedWithRules(
    rules: ReadonlyArray<ValuePresentationRule>,
    enabled = true,
    options: TimestampOptions = NATIVE_UTC_ISO,
  ): StoredValuePresentationSettings {
    return {
      version: 1,
      renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled, options } },
      rules,
    };
  }

  it('PRD example: stored rules -> snapshot -> plan -> present', () => {
    const rule = ruleWith({});
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([rule]),
      ENV,
      registryWithTimestamp(),
      9,
    );
    expect(snapshot.rules).toEqual([rule]);
    const plans = buildColumnPresentationPlans(
      {
        columns: [
          { name: 'created_at', dataType: 'BIGINT' },
          { name: 'note', dataType: 'TEXT' },
        ],
        source: { connectionId: 'conn-1', database: 'shop', namespace: 'public', table: 'orders' },
        snapshot,
      },
      registryWithTimestamp(),
    );
    expect(plans).toHaveLength(2);
    // 命中列：规则生效，Unix 毫秒 → iso-tz → Asia/Shanghai。
    expect(plans[0]?.status).toBe('ready');
    expect(plans[0]?.matchedRuleName).toBe('unix-ms-timestamps');
    expect(plans[0]?.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
    // 不命中列：走全局默认（TEXT 列无默认渲染器 → default）。
    expect(plans[1]?.status).toBe('default');
    expect(plans[1]?.matchedRuleName).toBeUndefined();
  });

  it('a non-matching number column falls back to the global default', () => {
    const rule = ruleWith({});
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([rule]),
      ENV,
      registryWithTimestamp(),
      9,
    );
    const plans = buildColumnPresentationPlans(
      {
        columns: [{ name: 'id', dataType: 'BIGINT' }],
        source: { connectionId: 'conn-1', database: 'shop', namespace: 'public', table: 'orders' },
        snapshot,
      },
      registryWithTimestamp(),
    );
    // 全局默认只管原生时间值：number 列无默认渲染器 → default（保持数字）。
    expect(plans[0]?.status).toBe('default');
    expect(plans[0]?.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'no-renderer',
    });
  });

  it('the first matching rule freezes the whole column (no fallback)', () => {
    const first = ruleWith({ name: 'first-ms' });
    const second = ruleWith({
      name: 'second-s',
      options: {
        interpretation: { kind: 'unix', unit: 'seconds' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    });
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([first, second]),
      ENV,
      registryWithTimestamp(),
      9,
    );
    const plans = buildColumnPresentationPlans(
      {
        columns: [{ name: 'created_at', dataType: 'BIGINT' }],
        source: { connectionId: 'conn-1', database: 'shop', namespace: 'public', table: 'orders' },
        snapshot,
      },
      registryWithTimestamp(),
    );
    expect(plans[0]?.matchedRuleName).toBe('first-ms');
    // 若回落到 second（Unix 秒），1705316400000 会被当成秒而越界；
    // 冻结在 first 则为毫秒解释。
    expect(plans[0]?.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it('a column override beats a matching rule', () => {
    const rule = ruleWith({});
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([rule]),
      ENV,
      registryWithTimestamp(),
      9,
    );
    const plans = buildColumnPresentationPlans(
      {
        columns: [{ name: 'created_at', dataType: 'BIGINT' }],
        source: { connectionId: 'conn-1', database: 'shop', namespace: 'public', table: 'orders' },
        snapshot: {
          ...snapshot,
          overrides: {
            '["conn-1","shop","public","orders","created_at"]': { mode: 'raw' },
          },
        },
      },
      registryWithTimestamp(),
    );
    expect(plans[0]?.status).toBe('raw');
    expect(plans[0]?.matchedRuleName).toBeUndefined();
  });

  it('snapshot carries no rules when the renderer is disabled', () => {
    const rule = ruleWith({});
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([rule], false),
      ENV,
      registryWithTimestamp(),
      9,
    );
    expect(snapshot.rules).toEqual([]);
    expect(snapshot.enabledRendererIds).toEqual([]);
  });

  it('snapshot still carries rules when the global options are invalid', () => {
    const rule = ruleWith({});
    const badOptions = {
      interpretation: { kind: 'native' },
      format: 'rfc3339',
      displayTimeZone: { kind: 'utc' },
    } as unknown as TimestampOptions;
    const snapshot = buildValuePresentationSnapshot(
      storedWithRules([rule], true, badOptions),
      ENV,
      registryWithTimestamp(),
      9,
    );
    // 全局 options 无效 → 不进 configs，但规则自带完整 options，仍生效。
    expect(snapshot.configs[TIMESTAMP_CONTRIBUTION_ID]).toBeUndefined();
    expect(snapshot.rules).toEqual([rule]);
    const plans = buildColumnPresentationPlans(
      {
        columns: [{ name: 'created_at', dataType: 'BIGINT' }],
        source: { connectionId: 'conn-1', database: 'shop', namespace: 'public', table: 'orders' },
        snapshot,
      },
      registryWithTimestamp(),
    );
    expect(plans[0]?.status).toBe('ready');
    expect(plans[0]?.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
  });

  it('an unknown top-level version yields an empty snapshot and keeps its version', () => {
    const snapshot = buildValuePresentationSnapshot(
      {
        version: 99,
        renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
        rules: [ruleWith({})],
      },
      ENV,
      registryWithTimestamp(),
      9,
    );
    expect(snapshot.rules).toEqual([]);
    expect(snapshot.configs).toEqual({});
  });
});
