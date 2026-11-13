/**
 * columnPresentationSession（issue-32 T09）：会话覆盖存储与合并。
 *
 * 覆盖：inherit 删除条目（无条目时不涨 revision）、raw / custom 按
 * 来源键写入、合并时会话为空原样返回快照（同一对象、同一 revision）、
 * 会话值胜过快照自带值、revision 单调递增、reset 只供测试隔离。
 */
import { describe, expect, it, beforeEach, vi } from 'vitest';
import type { ColumnSource } from '@datazen/extension-points';
import {
  applySessionColumnOverrides,
  clearSessionColumnOverride,
  getColumnPresentationSessionState,
  resetColumnPresentationSession,
  setSessionColumnOverride,
  subscribeColumnPresentationSession,
} from '../columnPresentationSession';
import { columnOverrideKey, type ValuePresentationSnapshot } from '../presentationPlan';

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-US', localTimeZone: 'UTC' } as const;

function baseSnapshot(
  overrides: ValuePresentationSnapshot['overrides'] = {},
): ValuePresentationSnapshot {
  return {
    revision: 3,
    environment: ENVIRONMENT,
    configs: {},
    enabledRendererIds: [],
    overrides,
    rules: [],
  };
}

beforeEach(() => {
  resetColumnPresentationSession();
});

describe('setSessionColumnOverride / clearSessionColumnOverride', () => {
  it('stores raw and custom overrides keyed by the full column source', () => {
    setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'raw' });
    const state = getColumnPresentationSessionState();
    expect(state.revision).toBe(1);
    expect(state.overrides[columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]).toEqual({
      mode: 'raw',
    });
    // Same table name under another connection is a different key.
    setSessionColumnOverride({ ...SOURCE, connectionId: 'conn-2' }, 'epoch_ms', { mode: 'raw' });
    expect(Object.keys(getColumnPresentationSessionState().overrides)).toHaveLength(2);
  });

  it('saving inherit deletes the entry (clear restores inheritance)', () => {
    setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'raw' });
    expect(getColumnPresentationSessionState().revision).toBe(1);
    setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'inherit' });
    expect(getColumnPresentationSessionState().overrides).toEqual({});
    expect(getColumnPresentationSessionState().revision).toBe(2);
  });

  it('saving inherit with no entry is a no-op (no revision bump, no notify)', () => {
    const listener = vi.fn();
    const unsubscribe = subscribeColumnPresentationSession(listener);
    setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'inherit' });
    expect(getColumnPresentationSessionState().revision).toBe(0);
    expect(listener).not.toHaveBeenCalled();
    unsubscribe();
  });

  it('clearSessionColumnOverride removes the entry and notifies subscribers', () => {
    setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'raw' });
    const listener = vi.fn();
    const unsubscribe = subscribeColumnPresentationSession(listener);
    clearSessionColumnOverride(SOURCE, 'epoch_ms');
    expect(getColumnPresentationSessionState().overrides).toEqual({});
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    const unsubscribe2 = subscribeColumnPresentationSession(listener);
    expect(typeof unsubscribe2).toBe('function');
    unsubscribe2();
  });
});

describe('applySessionColumnOverrides', () => {
  it('returns the very same snapshot (identity + revision) when the session is empty', () => {
    const snapshot = baseSnapshot();
    expect(applySessionColumnOverrides(snapshot)).toBe(snapshot);
  });

  it('merges session overrides over the snapshot ones and bumps the revision', () => {
    const snapshot = baseSnapshot({
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
    });
    setSessionColumnOverride(SOURCE, 'epoch_ms', {
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
    });
    setSessionColumnOverride(SOURCE, 'other', { mode: 'raw' });
    const merged = applySessionColumnOverrides(snapshot);
    expect(merged).not.toBe(snapshot);
    expect(merged.revision).toBe(3 + 2);
    expect(merged.overrides[columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]).toEqual({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
    });
    expect(merged.overrides[columnOverrideKey({ ...SOURCE, column: 'other' })]).toEqual({
      mode: 'raw',
    });
    // The base snapshot is not mutated.
    expect(snapshot.overrides[columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]).toEqual({
      mode: 'raw',
    });
  });
});
