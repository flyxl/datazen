/**
 * useValuePresentation（issue-32 T04）：宿主接线 hook。
 *
 * renderHook + act 范式（见 useColumnStats.test.ts）。全部用全新
 * ValuePresentationRegistryImpl，不动全局单例。
 *
 * 覆盖：初始 plans 正确；向全新 registry 注册 timestamp 贡献后重渲染，
 * plans 由 missing-renderer 变 ready；unregister 后变回 missing-renderer；
 * snapshot revision 变化重建；presentValue 查不到列返回 no-plan。
 */
import { describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  ValuePresentationRegistryImpl,
  type ColumnSource,
  type PresentationConfigValue,
  type VersionedPresentationConfig,
} from '@datazen/extension-points';
import { useValuePresentation } from '../useValuePresentation';
import type { ValuePresentationSnapshot } from '../../lib/valuePresentation/presentationPlan';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  type TimestampOptions,
} from '../../lib/valuePresentation/index';

const ENV_SHANGHAI = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' } as const;

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const COLUMNS = [{ name: 'created_at', dataType: 'timestamptz' }] as const;

const NATIVE_SHANGHAI_ISO_TZ: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

function timestampConfig(options: TimestampOptions): VersionedPresentationConfig {
  const interpretation: PresentationConfigValue =
    options.interpretation.kind === 'unix'
      ? { kind: 'unix', unit: options.interpretation.unit }
      : { kind: options.interpretation.kind };
  const displayTimeZone: PresentationConfigValue =
    options.displayTimeZone.kind === 'iana'
      ? { kind: 'iana', name: options.displayTimeZone.name }
      : { kind: options.displayTimeZone.kind };
  return {
    configVersion: 1,
    config: { interpretation, format: options.format, displayTimeZone },
  };
}

function makeSnapshot(options: TimestampOptions, revision = 1): ValuePresentationSnapshot {
  return {
    revision,
    environment: ENV_SHANGHAI,
    configs: { [TIMESTAMP_CONTRIBUTION_ID]: timestampConfig(options) },
    enabledRendererIds: [TIMESTAMP_CONTRIBUTION_ID],
    overrides: {},
  };
}

describe('useValuePresentation (issue-32 T04)', () => {
  it('初始 plans 正确：时间列 ready，presentValue 走同一计划', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const snapshot = makeSnapshot(NATIVE_SHANGHAI_ISO_TZ);
    const { result } = renderHook(() =>
      useValuePresentation({ columns: COLUMNS, source: SOURCE, snapshot, registry }),
    );

    expect(result.current.contributionRevision).toBe(registry.revision);
    expect(result.current.plans).toHaveLength(1);
    expect(result.current.plans[0]?.status).toBe('ready');
    expect(result.current.plans[0]?.rendererId).toBe(TIMESTAMP_CONTRIBUTION_ID);
    expect(result.current.planByColumn.get('created_at')?.status).toBe('ready');
    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    expect(typeof result.current.cacheKey).toBe('string');
    expect(result.current.cacheKey).not.toBe('');
  });

  it('注册 timestamp 贡献后重渲染：plans 由 missing-renderer 变 ready', () => {
    const registry = new ValuePresentationRegistryImpl();
    const snapshot = makeSnapshot(NATIVE_SHANGHAI_ISO_TZ);
    const { result, rerender } = renderHook(() =>
      useValuePresentation({ columns: COLUMNS, source: SOURCE, snapshot, registry }),
    );

    expect(result.current.plans[0]?.status).toBe('missing-renderer');
    expect(result.current.contributionRevision).toBe(0);

    act(() => {
      registry.register(timestampContribution);
    });
    rerender();

    expect(result.current.contributionRevision).toBe(1);
    expect(result.current.plans[0]?.status).toBe('ready');
    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('unregister 后变回 missing-renderer', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const snapshot = makeSnapshot(NATIVE_SHANGHAI_ISO_TZ);
    const { result, rerender } = renderHook(() =>
      useValuePresentation({ columns: COLUMNS, source: SOURCE, snapshot, registry }),
    );

    expect(result.current.plans[0]?.status).toBe('ready');

    act(() => {
      registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    });
    rerender();

    expect(result.current.plans[0]?.status).toBe('missing-renderer');
    expect(result.current.plans[0]?.reason).toBe('missing-renderer');
    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'missing-renderer',
    });
  });

  it('snapshot revision 变化重建：同一列的 present 输出随新配置变化', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result, rerender } = renderHook(
      ({ snapshot }: { snapshot: ValuePresentationSnapshot }) =>
        useValuePresentation({ columns: COLUMNS, source: SOURCE, snapshot, registry }),
      { initialProps: { snapshot: makeSnapshot(NATIVE_SHANGHAI_ISO_TZ, 1) } },
    );

    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    const firstCacheKey = result.current.cacheKey;

    rerender({ snapshot: makeSnapshot(NATIVE_UTC_ISO, 2) });

    expect(result.current.cacheKey).not.toBe(firstCacheKey);
    expect(result.current.plans[0]?.status).toBe('ready');
    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
  });

  it('重名列：plans 各有一项，planByColumn 只保留第一项', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const snapshot = makeSnapshot(NATIVE_SHANGHAI_ISO_TZ);
    const { result } = renderHook(() =>
      useValuePresentation({
        columns: [
          { name: 'created_at', dataType: 'timestamptz' },
          { name: 'created_at', dataType: 'timestamptz' },
        ],
        source: SOURCE,
        snapshot,
        registry,
      }),
    );

    expect(result.current.plans).toHaveLength(2);
    expect(result.current.plans[0]?.status).toBe('ready');
    expect(result.current.plans[1]?.status).toBe('ready');
    expect(result.current.planByColumn.size).toBe(1);
    expect(result.current.planByColumn.get('created_at')).toBe(result.current.plans[0]);
    expect(result.current.presentValue('created_at', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
  });

  it('presentValue 查不到列返回 no-plan', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const snapshot = makeSnapshot(NATIVE_SHANGHAI_ISO_TZ);
    const { result } = renderHook(() =>
      useValuePresentation({ columns: COLUMNS, source: SOURCE, snapshot, registry }),
    );

    expect(result.current.presentValue('no_such_column', '2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'no-plan',
    });
    expect(result.current.planByColumn.has('no_such_column')).toBe(false);
  });
});
