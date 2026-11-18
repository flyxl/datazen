/**
 * useColumnPresentationSession — 列覆盖会话生效的 hook 接缝（issue-32 T09）。
 *
 * 网格（DataTable）与 aside（DetailPanel）各自调用
 * `useSessionValuePresentationSnapshot(baseSnapshot)`，把进程内会话
 * 覆盖（columnPresentationSession.ts）合并进宿主快照，再用合并后的
 * 快照建计划。两处读同一会话存储、同一合并函数，所以同一列覆盖在
 * cell 与 aside 即时一致（PRD 验收 2）；T10 接持久化时只改合并的
 * 输入来源，本 hook 的调用点不用动。
 *
 * 会话为空时 `applySessionColumnOverrides` 原样返回传入快照，不改
 * revision、不改对象身份。
 */

import { useMemo, useSyncExternalStore } from 'react';
import {
  applySessionColumnOverrides,
  getColumnPresentationSessionState,
  subscribeColumnPresentationSession,
  type ColumnPresentationSessionState,
} from '../lib/valuePresentation/columnPresentationSession';
import type { ValuePresentationSnapshot } from '../lib/valuePresentation/presentationPlan';

/** 订阅会话覆盖存储（revision 变化即重渲染）。 */
export function useColumnPresentationSessionState(): ColumnPresentationSessionState {
  return useSyncExternalStore(
    subscribeColumnPresentationSession,
    getColumnPresentationSessionState,
    getColumnPresentationSessionState,
  );
}

/** 把会话覆盖合并进宿主快照；会话为空时返回原快照本身。 */
export function useSessionValuePresentationSnapshot(
  baseSnapshot: ValuePresentationSnapshot,
): ValuePresentationSnapshot {
  const session = useColumnPresentationSessionState();
  return useMemo(() => applySessionColumnOverrides(baseSnapshot, session), [baseSnapshot, session]);
}
