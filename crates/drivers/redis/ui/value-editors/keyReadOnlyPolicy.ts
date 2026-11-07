//! I-5 只读态判定（PRD §3.3 编辑区 / §4 I-5，本轨 E-3）。
//!
//! PRD 的口径是**穷举式**的：只读态只有两种，其余一律可编辑——
//!   ① 字节视图（Hex / Binary）：显示的是字节投影，按原样回写会毁掉载荷；
//!   ② 大 value（后端 `frame.truncated` 或前端 `redisBigValue` 哨兵判定）：
//!      载荷不完整，编辑半成品再写回去等于用截断内容覆盖原值。
//! 任何第三种"看起来像只读"的分支（旧的两态切换、codec 已解码、集合类空态…）
//! 都属于要删掉的冗余，因此这里刻意不做兜底：未知输入一律返回"可编辑"。
//!
//! 两种只读态都必须给出**原因文案**（I-5 原文"给出原因"），`i18nKey` 是
//! `redis.detail.readonly.*`，测试按 `data-readonly-reason` 断言而不是英文字面量。

import type { ValueFrame } from '../shared/types';
import { judgeBigValue, type BigValueVerdict } from './redisBigValue';
import type { ViewMode } from './valueView/views';

/** 字节投影视图：渲染出来的是 hex dump / 位串，不能原样回写成值。 */
export const BYTE_ONLY_VIEWS: readonly ViewMode[] = ['hex', 'binary'];

export type ReadOnlyReasonId = 'binary-view' | 'big-value';

export interface ReadOnlyReason {
  id: ReadOnlyReasonId;
  /** 原因文案的 i18n key（渲染进只读提示条）。 */
  i18nKey: string;
}

export const READ_ONLY_REASONS: Record<ReadOnlyReasonId, ReadOnlyReason> = {
  'binary-view': { id: 'binary-view', i18nKey: 'redis.detail.readonly.binaryView' },
  'big-value': { id: 'big-value', i18nKey: 'redis.detail.readonly.bigValue' },
};

/**
 * BUG-006: I-5 只读态② 的第二个成因 —— 超哨兵但**载荷完整**（后端截断阈值
 * 5 MiB、前端哨兵 64 KiB ⇒ 64 KiB ~ 5 MiB 是常态区间，`truncated === false`）。
 * 同一个 `big-value` 状态（定位 id / data 口径不变），但文案事实必须分支：
 * 真截断才说"载荷不完整"，否则与旁边缺席的 truncated 徽标自相矛盾。
 */
export const BIG_VALUE_COMPLETE_REASON: ReadOnlyReason = {
  id: 'big-value',
  i18nKey: 'redis.detail.readonly.bigValueComplete',
};

export interface ReadOnlyPolicy {
  readOnly: boolean;
  /** 只读时非空；可编辑时恒为 null（I-5 的"必须给原因"由此保证）。 */
  reason: ReadOnlyReason | null;
  bigValue: BigValueVerdict;
}

/** 视图是否属于"不可原样回写"的字节投影。 */
export function isByteOnlyView(view: ViewMode | null | undefined): boolean {
  return !!view && BYTE_ONLY_VIEWS.includes(view);
}

/**
 * 解析当前编辑面的只读态。
 *
 * 优先级：字节视图在前——它由用户主动选择、可当场切回去，而大 value 是数据事实；
 * 两者同时命中时提示"先切回文本视图"比提示"值太大"更可操作。
 */
export function resolveReadOnlyPolicy(input: {
  frame?: Pick<ValueFrame, 'keyType' | 'logicalLen' | 'truncated'> | null;
  view?: ViewMode | null;
}): ReadOnlyPolicy {
  const bigValue = judgeBigValue(input.frame ?? null);
  if (isByteOnlyView(input.view)) {
    return { readOnly: true, reason: READ_ONLY_REASONS['binary-view'], bigValue };
  }
  if (bigValue.big) {
    // BUG-006: two causes, two honest copies (same `big-value` state):
    // backend-truncated ⇒ the payload really is incomplete; over-sentinel-only
    // ⇒ the payload is COMPLETE and merely over the editable size budget.
    const reason = bigValue.truncated ? READ_ONLY_REASONS['big-value'] : BIG_VALUE_COMPLETE_REASON;
    return { readOnly: true, reason, bigValue };
  }
  return { readOnly: false, reason: null, bigValue };
}
