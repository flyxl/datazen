/**
 * columnPresentationSession — 列覆盖的会话生效接缝（issue-32 T09，设计 §7/§9）。
 *
 * T09 的交付边界（与 T10 的分工，写进注释与汇报）：
 *
 * - T09 只交付面板 + 两处入口 + 覆盖在**当前会话内**的生效：面板保存
 *   的覆盖写进本模块的进程内会话存储（`setSessionColumnOverride`），
 *   再由 `useSessionValuePresentationSnapshot`（hooks 目录）合并进快
 *   照；网格与 aside 各自从同一合并快照建计划，所以 cell / aside /
 *   预览显示结果一致（PRD 验收 2）。
 * - 本模块**不是持久化存储**：不写 settingsStore、不写后端、不写磁
 *   盘；刷新 / 重开后会话覆盖丢失。这是 T09 的有意边界，不是遗漏。
 * - T10（表列覆盖持久化与来源隔离）要接的口子：
 *   1. 在 `StoredValuePresentationSettings` 中加表列覆盖字段，并让
 *      `buildValuePresentationSnapshot`（settings.ts）把它填进快照
 *      `overrides`（目前恒 `{}`）；
 *   2. 面板保存时除写会话存储外（或改为）经
 *      `updateValuePresentationSettings` 写持久化，写成功后清除本
 *      模块里同一键的会话条目，以免会话值遮住已持久化的值；
 *   3. 覆盖键继续用 `columnOverrideKey`（连接/数据库/namespace/
 *      表/列，保留大小写、缺省与空串不碰撞），来源隔离的判定仍以
 *      T04/T05 已有的 ColumnSource 为准；
 *   4. 本模块的合并函数 `applySessionColumnOverrides` 是唯一的合
 *      并点——T10 不应在 DataTable / DetailPanel 里再各写一份合并。
 *
 * 语义：
 * - `inherit` 不存条目：保存跟随规则即删除本列的会话覆盖（清除覆盖
 *   恢复继承，PRD 验收 4）；
 * - `raw` / `custom` 按 `columnOverrideKey` 存一条；
 * - 合并时会话覆盖后于快照自带 overrides（会话值胜出）；会话为空时
 *   原样返回传入快照（同一对象、同一 revision），不扰动既有快照测试
 *   的 revision 透传不变量；
 * - revision = 快照 revision + 会话 revision：两者都只增不减，和单
 *   调递增，所以合并 revision 也单调递增，计划缓存键随之失效。
 *
 * 纯逻辑 + 一个模块级外部存储（useSyncExternalStore 订阅）；无 React /
 * DOM / Store / IPC 依赖。`resetColumnPresentationSession` 只供测试
 * 隔离用，生产代码不调用。
 */

import type { ColumnPresentationOverride, ColumnSource } from '@datazen/extension-points';
import { columnOverrideKey, type ValuePresentationSnapshot } from './presentationPlan';

/** 会话覆盖状态：键 → 覆盖；revision 每次写入 +1。 */
export interface ColumnPresentationSessionState {
  readonly overrides: Readonly<Record<string, ColumnPresentationOverride>>;
  readonly revision: number;
}

const EMPTY_OVERRIDES: Readonly<Record<string, ColumnPresentationOverride>> = {};

let sessionState: ColumnPresentationSessionState = { overrides: EMPTY_OVERRIDES, revision: 0 };
const listeners = new Set<() => void>();

export function getColumnPresentationSessionState(): ColumnPresentationSessionState {
  return sessionState;
}

export function subscribeColumnPresentationSession(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function emitColumnPresentationSessionChange(): void {
  for (const listener of listeners) {
    listener();
  }
}

/**
 * 保存一列的会话覆盖。`inherit` 即删除本列条目（无条目时不写、不
 * 涨 revision、不通知）；`raw` / `custom` 按来源键写入（调用方已
 * 经 `validateColumnPresentationOverride` 校验，非法覆盖禁存）。
 */
export function setSessionColumnOverride(
  source: Readonly<Omit<ColumnSource, 'column'>>,
  columnName: string,
  override: Readonly<ColumnPresentationOverride>,
): void {
  const key = columnOverrideKey({ ...source, column: columnName });
  if (override.mode === 'inherit') {
    if (!(key in sessionState.overrides)) return;
    const next = { ...sessionState.overrides };
    delete next[key];
    sessionState = { overrides: next, revision: sessionState.revision + 1 };
    emitColumnPresentationSessionChange();
    return;
  }
  sessionState = {
    overrides: { ...sessionState.overrides, [key]: override },
    revision: sessionState.revision + 1,
  };
  emitColumnPresentationSessionChange();
}

/** 清除一列的会话覆盖（等价于保存 `inherit`）。 */
export function clearSessionColumnOverride(
  source: Readonly<Omit<ColumnSource, 'column'>>,
  columnName: string,
): void {
  setSessionColumnOverride(source, columnName, { mode: 'inherit' });
}

/** 测试隔离用：把会话存储恢复为空（revision 归零并通知订阅者）。 */
export function resetColumnPresentationSession(): void {
  sessionState = { overrides: EMPTY_OVERRIDES, revision: 0 };
  emitColumnPresentationSessionChange();
}

/**
 * 把会话覆盖合并进快照（唯一的合并点，见文件头 T10 口子 4）。
 * 会话为空时原样返回传入快照；否则返回新快照：overrides 为快照
 * 自带值 + 会话值（会话胜出），revision 为两者之和。
 */
export function applySessionColumnOverrides(
  snapshot: ValuePresentationSnapshot,
  session: ColumnPresentationSessionState = getColumnPresentationSessionState(),
): ValuePresentationSnapshot {
  if (session.revision === 0 || Object.keys(session.overrides).length === 0) {
    return snapshot;
  }
  return {
    ...snapshot,
    overrides: { ...snapshot.overrides, ...session.overrides },
    revision: snapshot.revision + session.revision,
  };
}
