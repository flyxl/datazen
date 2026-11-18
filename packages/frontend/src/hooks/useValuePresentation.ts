/**
 * useValuePresentation — 宿主展示管线接线 hook（issue-32 T04，设计 §7）。
 *
 * 面板层共享 hook：列计划通过本 hook 产出，再由 T05 经 props / context
 * 注入 DataTable 与 DetailPanel；组件本身不直接读 Store 或 IPC。
 *
 * - 用 useSyncExternalStore 订阅 registry（subscribe + getSnapshot 返回
 *   revision），注册 / 卸载触发重建；
 * - plans 用 useMemo 以 computePresentationPlanCacheKey 的结果为依赖重建
 *   （不要逐渲染重建 prepare）；
 * - 不读 Store / DOM / IPC / 文件；snapshot 由调用方传入（T06 将来从
 *   store 提供）。
 */

import { useCallback, useMemo, useSyncExternalStore } from 'react';
import type {
  ColumnSource,
  PresentationResult,
  PresentationValue,
  ValuePresentationRegistry,
} from '@datazen/extension-points';
import { valuePresentationRegistry } from '@datazen/extension-points';
import {
  buildColumnPresentationPlans,
  computePresentationPlanCacheKey,
  type ColumnPresentationPlan,
  type PresentationPlanColumn,
  type ValuePresentationSnapshot,
} from '../lib/valuePresentation/presentationPlan';

/** useValuePresentation 的输入。 */
export interface UseValuePresentationInput {
  readonly columns: ReadonlyArray<PresentationPlanColumn>;
  /** 表身份；缺省表示查询结果等无来源列，覆盖不可用。 */
  readonly source?: Readonly<Omit<ColumnSource, 'column'>>;
  readonly snapshot: ValuePresentationSnapshot;
  /** 缺省为全局 valuePresentationRegistry。 */
  readonly registry?: ValuePresentationRegistry;
}

/** useValuePresentation 的输出。 */
export interface UseValuePresentationResult {
  readonly plans: ReadonlyArray<ColumnPresentationPlan>;
  readonly planByColumn: ReadonlyMap<string, ColumnPresentationPlan>;
  readonly presentValue: (columnName: string, value: PresentationValue) => PresentationResult;
  readonly contributionRevision: number;
  readonly cacheKey: string;
}

export function useValuePresentation(
  input: Readonly<UseValuePresentationInput>,
): UseValuePresentationResult {
  const { columns, source, snapshot } = input;
  const registry: ValuePresentationRegistry = input.registry ?? valuePresentationRegistry;

  const subscribe = useCallback(
    (onStoreChange: () => void): (() => void) => registry.subscribe(onStoreChange),
    [registry],
  );
  const getSnapshot = useCallback((): number => registry.revision, [registry]);
  const contributionRevision = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const cacheKey = computePresentationPlanCacheKey({
    source,
    columns,
    configRevision: snapshot.revision,
    contributionRevision,
    locale: snapshot.environment.locale,
    localTimeZone: snapshot.environment.localTimeZone,
  });

  // 依赖只取 cacheKey：缓存键已覆盖全部失效因子（来源身份、列元数据、
  // 配置修订、贡献修订、locale、localTimeZone），键不变即计划不变，
  // 不逐渲染重建 prepare。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const plans = useMemo(
    () => buildColumnPresentationPlans({ columns, source, snapshot }, registry),
    [cacheKey],
  );

  const planByColumn = useMemo(() => {
    const map = new Map<string, ColumnPresentationPlan>();
    for (const plan of plans) {
      // 同名列出现两次时保留第一项（plans 本身仍按输入顺序各有一项）。
      if (!map.has(plan.columnName)) {
        map.set(plan.columnName, plan);
      }
    }
    return map as ReadonlyMap<string, ColumnPresentationPlan>;
  }, [plans]);

  const presentValue = useCallback(
    (columnName: string, value: PresentationValue): PresentationResult => {
      const plan = planByColumn.get(columnName);
      if (plan === undefined) {
        return { kind: 'unchanged', reason: 'no-plan' };
      }
      return plan.present(value);
    },
    [planByColumn],
  );

  return { plans, planByColumn, presentValue, contributionRevision, cacheKey };
}
