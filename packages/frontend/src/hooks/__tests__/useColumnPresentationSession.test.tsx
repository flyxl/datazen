/**
 * useColumnPresentationSession（issue-32 T09）：会话覆盖合并进快照的 hook。
 *
 * renderHook + act 范式：空会话时返回原快照本身；会话写入后合并快照
 * 的 overrides 变化、revision 递增并触发重渲染；reset 后恢复原快照。
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import type { ColumnSource } from '@datazen/extension-points';
import { useSessionValuePresentationSnapshot } from '../useColumnPresentationSession';
import {
  resetColumnPresentationSession,
  setSessionColumnOverride,
} from '../../lib/valuePresentation/columnPresentationSession';
import {
  columnOverrideKey,
  type ValuePresentationSnapshot,
} from '../../lib/valuePresentation/presentationPlan';

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const BASE: ValuePresentationSnapshot = {
  revision: 2,
  environment: { locale: 'en-US', localTimeZone: 'UTC' },
  configs: {},
  enabledRendererIds: [],
  overrides: {},
  rules: [],
};

beforeEach(() => {
  resetColumnPresentationSession();
});

describe('useSessionValuePresentationSnapshot', () => {
  it('returns the base snapshot itself while the session is empty', () => {
    const { result } = renderHook(() => useSessionValuePresentationSnapshot(BASE));
    expect(result.current).toBe(BASE);
  });

  it('merges a session override and bumps the revision on write', () => {
    const { result } = renderHook(() => useSessionValuePresentationSnapshot(BASE));
    act(() => {
      setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'raw' });
    });
    expect(result.current).not.toBe(BASE);
    expect(result.current.revision).toBe(3);
    expect(result.current.overrides[columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]).toEqual({
      mode: 'raw',
    });
  });

  it('returns to the base snapshot after the session is reset', () => {
    const { result } = renderHook(() => useSessionValuePresentationSnapshot(BASE));
    act(() => {
      setSessionColumnOverride(SOURCE, 'epoch_ms', { mode: 'raw' });
    });
    expect(result.current.revision).toBe(3);
    act(() => {
      resetColumnPresentationSession();
    });
    expect(result.current).toBe(BASE);
  });
});
