/**
 * columnOverrides（issue-32 T10）：表列覆盖持久化与来源隔离。
 *
 * 覆盖：键形状与覆盖值验证（无效键 / 无效覆盖 / inherit 条目丢弃、
 * 200 条上限）、withColumnOverride 的纯函数语义（写 / 删 / 无变化
 * 原值返回 / 未知版本不动）、saveColumnOverride 的保存链路（成功
 * 清除会话条目、失败保留会话条目与旧配置、未知版本与无来源拒绝）、
 * 隔离矩阵（连接 / 数据库 / schema / 列 / 大小写 / missing-vs-empty）、
 * 覆盖优先于规则、重启恢复模拟（JSON 往返 → 重建快照 → 计划仍命中
 * 覆盖）。全部用全新 ValuePresentationRegistryImpl，不动全局单例；
 * 会话存储每测前 resetColumnPresentationSession 隔离。
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  type ColumnPresentationOverride,
  type ColumnSource,
} from '@datazen/extension-points';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  applySessionColumnOverrides,
  buildColumnPresentationPlans,
  buildValuePresentationSnapshot,
  columnOverrideFromSnapshot,
  columnOverrideKey,
  getColumnPresentationSessionState,
  parseStoredValuePresentationSettings,
  resetColumnPresentationSession,
  timestampContribution,
  type StoredValuePresentationSettings,
  type TimestampOptions,
} from '../index';
import {
  MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES,
  saveColumnOverride,
  validateStoredColumnOverrides,
  withColumnOverride,
} from '../columnOverrides';

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

const UNIX_SECONDS_UTC: TimestampOptions = {
  interpretation: { kind: 'unix', unit: 'seconds' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const RAW: ColumnPresentationOverride = { mode: 'raw' };
const CUSTOM_MS: ColumnPresentationOverride = {
  mode: 'custom',
  rendererId: TIMESTAMP_CONTRIBUTION_ID,
  options: UNIX_MS_SHANGHAI,
};
const CUSTOM_SECONDS: ColumnPresentationOverride = {
  mode: 'custom',
  rendererId: TIMESTAMP_CONTRIBUTION_ID,
  options: UNIX_SECONDS_UTC,
};

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function keyFor(source: Readonly<Omit<ColumnSource, 'column'>>, column: string): string {
  return columnOverrideKey({ ...source, column });
}

function storedWithOverrides(
  overrides: Readonly<Record<string, ColumnPresentationOverride>>,
): StoredValuePresentationSettings {
  return {
    version: 1,
    renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
    columnOverrides: overrides,
  };
}

beforeEach(() => {
  resetColumnPresentationSession();
});

describe('validateStoredColumnOverrides', () => {
  it('keeps valid raw and custom entries', () => {
    const parsed = validateStoredColumnOverrides({
      [keyFor(SOURCE, 'epoch_ms')]: RAW,
      [keyFor(SOURCE, 'created_at')]: CUSTOM_MS,
    });
    expect(parsed).toEqual({
      [keyFor(SOURCE, 'epoch_ms')]: RAW,
      [keyFor(SOURCE, 'created_at')]: CUSTOM_MS,
    });
  });

  it('returns an empty record for non-object input', () => {
    for (const input of [undefined, null, 'x', 42, true, [], ['a']]) {
      expect(validateStoredColumnOverrides(input)).toEqual({});
    }
  });

  it('drops entries with a malformed key', () => {
    const parsed = validateStoredColumnOverrides({
      'not-a-key': RAW,
      '["conn-1"]': RAW,
      '["conn-1","shop","public","orders"]': RAW,
      '[1,"shop","public","orders","c"]': RAW,
      '[null,"shop","public","orders","c"]': RAW,
      [keyFor(SOURCE, 'epoch_ms')]: RAW,
    });
    expect(parsed).toEqual({ [keyFor(SOURCE, 'epoch_ms')]: RAW });
  });

  it('drops invalid overrides and inherit entries', () => {
    const parsed = validateStoredColumnOverrides({
      [keyFor(SOURCE, 'a')]: { mode: 'inherit' },
      [keyFor(SOURCE, 'b')]: { mode: 'bogus' },
      [keyFor(SOURCE, 'c')]: { mode: 'custom', rendererId: 'acme.other', options: {} },
      [keyFor(SOURCE, 'd')]: 'not-an-object',
      [keyFor(SOURCE, 'e')]: RAW,
    });
    expect(parsed).toEqual({ [keyFor(SOURCE, 'e')]: RAW });
  });

  it('keeps at most MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES entries', () => {
    expect(MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES).toBe(200);
    const input: Record<string, ColumnPresentationOverride> = {};
    for (let i = 0; i < MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES + 1; i += 1) {
      input[JSON.stringify([`conn-${i}`, 'db', 'ns', 't', 'c'])] = RAW;
    }
    expect(Object.keys(validateStoredColumnOverrides(input))).toHaveLength(
      MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES,
    );
  });
});

describe('withColumnOverride', () => {
  it('adds a raw override and preserves renderers and rules', () => {
    const stored: StoredValuePresentationSettings = {
      version: 1,
      renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
      rules: [],
    };
    const next = withColumnOverride(stored, SOURCE, 'epoch_ms', RAW);
    expect(next).not.toBe(stored);
    expect(next.columnOverrides).toEqual({ [keyFor(SOURCE, 'epoch_ms')]: RAW });
    expect(next.renderers).toBe(stored.renderers);
    expect(next.rules).toBe(stored.rules);
    expect(stored.columnOverrides).toBeUndefined();
  });

  it('adds a custom override keyed by the full source identity', () => {
    const next = withColumnOverride({ version: 1, renderers: {} }, SOURCE, 'created_at', CUSTOM_MS);
    expect(next.columnOverrides).toEqual({ [keyFor(SOURCE, 'created_at')]: CUSTOM_MS });
  });

  it('deletes the entry for inherit and returns the original when nothing is stored', () => {
    const stored = storedWithOverrides({ [keyFor(SOURCE, 'epoch_ms')]: RAW });
    const next = withColumnOverride(stored, SOURCE, 'epoch_ms', { mode: 'inherit' });
    expect(next.columnOverrides).toEqual({});
    expect(withColumnOverride(stored, SOURCE, 'other', { mode: 'inherit' })).toBe(stored);
  });

  it('returns the original stored value for an unknown top-level version', () => {
    const stored: StoredValuePresentationSettings = { version: 99, renderers: {} };
    expect(withColumnOverride(stored, SOURCE, 'epoch_ms', RAW)).toBe(stored);
  });

  it('returns the original stored value when the override is unchanged', () => {
    const stored = storedWithOverrides({ [keyFor(SOURCE, 'epoch_ms')]: RAW });
    expect(withColumnOverride(stored, SOURCE, 'epoch_ms', RAW)).toBe(stored);
  });

  it('returns the original stored value for an invalid override', () => {
    const stored: StoredValuePresentationSettings = { version: 1, renderers: {} };
    expect(withColumnOverride(stored, SOURCE, 'epoch_ms', { mode: 'bogus' } as never)).toBe(stored);
  });

  it('refuses a new entry past the cap but still allows replacing an existing one', () => {
    const overrides: Record<string, ColumnPresentationOverride> = {};
    for (let i = 0; i < MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES; i += 1) {
      overrides[JSON.stringify([`conn-${i}`, 'db', 'ns', 't', 'c'])] = RAW;
    }
    const stored: StoredValuePresentationSettings = {
      version: 1,
      renderers: {},
      columnOverrides: overrides,
    };
    expect(withColumnOverride(stored, { connectionId: 'conn-new', table: 't' }, 'c', RAW)).toBe(
      stored,
    );
    const replaced = withColumnOverride(
      stored,
      { connectionId: 'conn-0', database: 'db', namespace: 'ns', table: 't' },
      'c',
      CUSTOM_MS,
    );
    expect(replaced.columnOverrides?.[JSON.stringify(['conn-0', 'db', 'ns', 't', 'c'])]).toEqual(
      CUSTOM_MS,
    );
  });
});

describe('saveColumnOverride', () => {
  function persistSpy() {
    return vi.fn(async (_next: StoredValuePresentationSettings) => {});
  }

  it('persists the override and clears the session entry on success', async () => {
    const persist = persistSpy();
    const stored: StoredValuePresentationSettings = { version: 1, renderers: {} };
    const result = await saveColumnOverride({
      stored,
      source: SOURCE,
      columnName: 'epoch_ms',
      override: RAW,
      persist,
    });
    expect(result).toEqual({ ok: true });
    expect(persist).toHaveBeenCalledTimes(1);
    expect(persist.mock.calls[0]![0].columnOverrides).toEqual({
      [keyFor(SOURCE, 'epoch_ms')]: RAW,
    });
    // 成功后会话条目已清除，不再遮住持久值。
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });

  it('keeps the session entry and the old config when persistence fails', async () => {
    const persist = vi.fn(async (_next: StoredValuePresentationSettings) => {
      throw new Error('disk full');
    });
    const stored: StoredValuePresentationSettings = { version: 1, renderers: {} };
    const result = await saveColumnOverride({
      stored,
      source: SOURCE,
      columnName: 'epoch_ms',
      override: CUSTOM_MS,
      persist,
    });
    expect(result).toEqual({ ok: false, reason: 'save-failed' });
    expect(persist).toHaveBeenCalledTimes(1);
    // 会话条目保留：用户刚改的配置不丢，旧配置继续生效。
    expect(getColumnPresentationSessionState().overrides).toEqual({
      [keyFor(SOURCE, 'epoch_ms')]: CUSTOM_MS,
    });
  });

  it('deletes the stored entry for inherit and clears the session entry', async () => {
    const persist = persistSpy();
    const stored = storedWithOverrides({
      [keyFor(SOURCE, 'epoch_ms')]: CUSTOM_MS,
      [keyFor(SOURCE, 'other')]: RAW,
    });
    const result = await saveColumnOverride({
      stored,
      source: SOURCE,
      columnName: 'epoch_ms',
      override: { mode: 'inherit' },
      persist,
    });
    expect(result).toEqual({ ok: true });
    expect(persist.mock.calls[0]![0].columnOverrides).toEqual({
      [keyFor(SOURCE, 'other')]: RAW,
    });
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });

  it('refuses to save without a source and never touches persistence or session', async () => {
    const persist = persistSpy();
    const result = await saveColumnOverride({
      stored: { version: 1, renderers: {} },
      source: undefined,
      columnName: 'epoch_ms',
      override: RAW,
      persist,
    });
    expect(result).toEqual({ ok: false, reason: 'no-source' });
    expect(persist).not.toHaveBeenCalled();
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });

  it('refuses an invalid override without touching persistence or session', async () => {
    const persist = persistSpy();
    const result = await saveColumnOverride({
      stored: { version: 1, renderers: {} },
      source: SOURCE,
      columnName: 'epoch_ms',
      override: { mode: 'bogus' } as never,
      persist,
    });
    expect(result).toEqual({ ok: false, reason: 'invalid-override' });
    expect(persist).not.toHaveBeenCalled();
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });

  it('refuses to write overrides for an unknown top-level version', async () => {
    const persist = persistSpy();
    const result = await saveColumnOverride({
      stored: { version: 99, renderers: {} },
      source: SOURCE,
      columnName: 'epoch_ms',
      override: RAW,
      persist,
    });
    expect(result).toEqual({ ok: false, reason: 'unknown-version' });
    expect(persist).not.toHaveBeenCalled();
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });

  it('refuses a new entry past the cap without touching persistence', async () => {
    const overrides: Record<string, ColumnPresentationOverride> = {};
    for (let i = 0; i < MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES; i += 1) {
      overrides[JSON.stringify([`conn-${i}`, 'db', 'ns', 't', 'c'])] = RAW;
    }
    const persist = persistSpy();
    const result = await saveColumnOverride({
      stored: { version: 1, renderers: {}, columnOverrides: overrides },
      source: { connectionId: 'conn-new', table: 't' },
      columnName: 'c',
      override: RAW,
      persist,
    });
    expect(result).toEqual({ ok: false, reason: 'too-many-overrides' });
    expect(persist).not.toHaveBeenCalled();
  });

  it('succeeds without a persistence write when deleting a missing entry', async () => {
    const persist = persistSpy();
    const result = await saveColumnOverride({
      stored: { version: 1, renderers: {} },
      source: SOURCE,
      columnName: 'epoch_ms',
      override: { mode: 'inherit' },
      persist,
    });
    expect(result).toEqual({ ok: true });
    expect(persist).not.toHaveBeenCalled();
  });

  it('clears the session entry when saving an override identical to the stored one (T10-BUG-001)', async () => {
    const persist = persistSpy();
    const stored = storedWithOverrides({ [keyFor(SOURCE, 'epoch_ms')]: CUSTOM_MS });
    const result = await saveColumnOverride({
      stored,
      source: SOURCE,
      columnName: 'epoch_ms',
      override: CUSTOM_MS,
      persist,
    });
    expect(result).toEqual({ ok: true });
    // 无变化不写持久化，但乐观写进会话的条目必须清除，否则与持久
    // 值相同的残留条目会抬高合并快照 revision。
    expect(persist).not.toHaveBeenCalled();
    expect(getColumnPresentationSessionState().overrides).toEqual({});
    const snapshot = buildValuePresentationSnapshot(stored, ENV, registryWithTimestamp(), 1);
    expect(applySessionColumnOverrides(snapshot)).toBe(snapshot);
    expect(snapshot.revision).toBe(1);
  });
});

describe('column override isolation matrix (issue-32 T10)', () => {
  function matrixStored(): StoredValuePresentationSettings {
    return storedWithOverrides({
      [keyFor(SOURCE, 'created_at')]: RAW,
      [keyFor({ ...SOURCE, connectionId: 'conn-2' }, 'created_at')]: CUSTOM_MS,
      [keyFor({ ...SOURCE, database: 'otherdb' }, 'created_at')]: CUSTOM_SECONDS,
      [keyFor({ ...SOURCE, namespace: 'private' }, 'created_at')]: RAW,
      [keyFor(SOURCE, 'updated_at')]: CUSTOM_MS,
      [keyFor({ ...SOURCE, table: 'Orders' }, 'created_at')]: CUSTOM_SECONDS,
    });
  }

  it('keeps the same table name apart across connections, databases and schemas', () => {
    const snapshot = buildValuePresentationSnapshot(
      matrixStored(),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'created_at')).toEqual(RAW);
    expect(
      columnOverrideFromSnapshot(snapshot, { ...SOURCE, connectionId: 'conn-2' }, 'created_at'),
    ).toEqual(CUSTOM_MS);
    expect(
      columnOverrideFromSnapshot(snapshot, { ...SOURCE, database: 'otherdb' }, 'created_at'),
    ).toEqual(CUSTOM_SECONDS);
    expect(
      columnOverrideFromSnapshot(snapshot, { ...SOURCE, namespace: 'private' }, 'created_at'),
    ).toEqual(RAW);
    // 同表不同列互不串；无条目列跟随规则（inherit）。
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'updated_at')).toEqual(CUSTOM_MS);
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'deleted_at')).toEqual({
      mode: 'inherit',
    });
  });

  it('treats table and column names with different case as different sources', () => {
    const snapshot = buildValuePresentationSnapshot(
      matrixStored(),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(
      columnOverrideFromSnapshot(snapshot, { ...SOURCE, table: 'Orders' }, 'created_at'),
    ).toEqual(CUSTOM_SECONDS);
    expect(
      columnOverrideFromSnapshot(snapshot, { ...SOURCE, table: 'ORDERS' }, 'created_at'),
    ).toEqual({ mode: 'inherit' });
    expect(columnOverrideFromSnapshot(snapshot, SOURCE, 'Created_At')).toEqual({
      mode: 'inherit',
    });
  });

  it('does not collide a missing database/namespace with an empty string', () => {
    const missingDb = keyFor({ connectionId: 'conn-1', table: 'orders' }, 'created_at');
    const emptyDb = keyFor({ connectionId: 'conn-1', database: '', table: 'orders' }, 'created_at');
    expect(missingDb).not.toBe(emptyDb);
    const missingNs = keyFor(
      { connectionId: 'conn-1', database: 'shop', table: 'orders' },
      'created_at',
    );
    const emptyNs = keyFor(
      { connectionId: 'conn-1', database: 'shop', namespace: '', table: 'orders' },
      'created_at',
    );
    expect(missingNs).not.toBe(emptyNs);
    const snapshot = buildValuePresentationSnapshot(
      storedWithOverrides({ [missingDb]: RAW, [emptyDb]: CUSTOM_MS }),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(
      columnOverrideFromSnapshot(
        snapshot,
        { connectionId: 'conn-1', table: 'orders' },
        'created_at',
      ),
    ).toEqual(RAW);
    expect(
      columnOverrideFromSnapshot(
        snapshot,
        { connectionId: 'conn-1', database: '', table: 'orders' },
        'created_at',
      ),
    ).toEqual(CUSTOM_MS);
  });

  it('plans stay isolated per source: raw in one connection, custom in another', () => {
    const snapshot = buildValuePresentationSnapshot(
      matrixStored(),
      ENV,
      registryWithTimestamp(),
      1,
    );
    const registry = registryWithTimestamp();
    const planConn1 = buildColumnPresentationPlans(
      { columns: [{ name: 'created_at', dataType: 'bigint' }], source: SOURCE, snapshot },
      registry,
    )[0]!;
    const planConn2 = buildColumnPresentationPlans(
      {
        columns: [{ name: 'created_at', dataType: 'bigint' }],
        source: { ...SOURCE, connectionId: 'conn-2' },
        snapshot,
      },
      registry,
    )[0]!;
    expect(planConn1.status).toBe('raw');
    expect(planConn1.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'raw-override',
    });
    expect(planConn2.status).toBe('ready');
    expect(planConn2.matchedRuleName).toBeUndefined();
  });

  it('never applies an override to a sourceless query column', () => {
    const snapshot = buildValuePresentationSnapshot(
      matrixStored(),
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(columnOverrideFromSnapshot(snapshot, undefined, 'created_at')).toEqual({
      mode: 'inherit',
    });
    const plan = buildColumnPresentationPlans(
      {
        columns: [{ name: 'created_at', dataType: 'timestamp with time zone' }],
        source: undefined,
        snapshot,
      },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('ready');
    expect(plan.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
  });
});

describe('override beats rule; restart restores the override', () => {
  const RULE = {
    name: 'unix-ms',
    enabled: true,
    match: { column: { kind: 'exact', value: 'epoch_ms' } },
    options: UNIX_MS_SHANGHAI,
  };

  function storedWithRuleAndOverride(
    override: ColumnPresentationOverride,
  ): StoredValuePresentationSettings {
    return {
      version: 1,
      renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
      rules: [RULE] as StoredValuePresentationSettings['rules'],
      columnOverrides: { [keyFor(SOURCE, 'epoch_ms')]: override },
    };
  }

  it('a raw override blocks the matching rule', () => {
    const snapshot = buildValuePresentationSnapshot(
      storedWithRuleAndOverride(RAW),
      ENV,
      registryWithTimestamp(),
      1,
    );
    const plan = buildColumnPresentationPlans(
      {
        columns: [{ name: 'epoch_ms', dataType: 'bigint' }],
        source: SOURCE,
        snapshot,
      },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('raw');
    expect(plan.matchedRuleName).toBeUndefined();
    expect(plan.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'raw-override',
    });
  });

  it('a custom override wins over the matching rule without recording a rule hit', () => {
    const snapshot = buildValuePresentationSnapshot(
      storedWithRuleAndOverride(CUSTOM_SECONDS),
      ENV,
      registryWithTimestamp(),
      1,
    );
    const plan = buildColumnPresentationPlans(
      {
        columns: [{ name: 'epoch_ms', dataType: 'bigint' }],
        source: SOURCE,
        snapshot,
      },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('ready');
    expect(plan.matchedRuleName).toBeUndefined();
    // Unix 秒解释（覆盖），不是规则的 Unix 毫秒解释。
    expect(plan.present(1705316400)).toEqual({
      kind: 'text',
      text: '2024-01-15T11:00:00Z',
      interpretation: 'unix',
    });
  });

  it('without an override the same column still hits the rule', () => {
    const stored = storedWithRuleAndOverride(RAW);
    const { columnOverrides: _dropped, ...withoutOverride } = stored;
    void _dropped;
    const snapshot = buildValuePresentationSnapshot(
      withoutOverride,
      ENV,
      registryWithTimestamp(),
      1,
    );
    const plan = buildColumnPresentationPlans(
      {
        columns: [{ name: 'epoch_ms', dataType: 'bigint' }],
        source: SOURCE,
        snapshot,
      },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('ready');
    expect(plan.matchedRuleName).toBe('unix-ms');
  });

  it('restores the override after a simulated restart (store rebuild → snapshot rebuild → plan rebuild)', async () => {
    // 保存 → 持久 JSON → 模拟重启：store 重建（重新 parse）→ 快照
    // 重建 → 计划重建，覆盖仍在且仍优先于规则。
    const persist = vi.fn(async (next: StoredValuePresentationSettings) => {
      savedJson = JSON.stringify(next);
    });
    let savedJson = '';
    const saved = await saveColumnOverride({
      stored: {
        version: 1,
        renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO } },
        rules: [RULE] as StoredValuePresentationSettings['rules'],
      },
      source: SOURCE,
      columnName: 'epoch_ms',
      override: CUSTOM_SECONDS,
      persist,
    });
    expect(saved).toEqual({ ok: true });
    expect(persist).toHaveBeenCalledTimes(1);

    const reloaded = parseStoredValuePresentationSettings(JSON.parse(savedJson));
    expect(reloaded.columnOverrides).toEqual({
      [keyFor(SOURCE, 'epoch_ms')]: CUSTOM_SECONDS,
    });
    const snapshot = buildValuePresentationSnapshot(reloaded, ENV, registryWithTimestamp(), 1);
    const plan = buildColumnPresentationPlans(
      {
        columns: [{ name: 'epoch_ms', dataType: 'bigint' }],
        source: SOURCE,
        snapshot,
      },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('ready');
    expect(plan.matchedRuleName).toBeUndefined();
    expect(plan.present(1705316400)).toEqual({
      kind: 'text',
      text: '2024-01-15T11:00:00Z',
      interpretation: 'unix',
    });
  });

  it('restores a raw override after a simulated restart even with the renderer disabled', () => {
    const savedJson = JSON.stringify(storedWithOverrides({ [keyFor(SOURCE, 'c')]: RAW }));
    const reloaded = parseStoredValuePresentationSettings(JSON.parse(savedJson));
    // 总开关关闭时快照仍带覆盖：raw 不依赖总开关。
    const snapshot = buildValuePresentationSnapshot(
      { ...reloaded, renderers: {} },
      ENV,
      registryWithTimestamp(),
      1,
    );
    expect(snapshot.overrides).toEqual({ [keyFor(SOURCE, 'c')]: RAW });
    const plan = buildColumnPresentationPlans(
      { columns: [{ name: 'c', dataType: 'bigint' }], source: SOURCE, snapshot },
      registryWithTimestamp(),
    )[0]!;
    expect(plan.status).toBe('raw');
  });
});
