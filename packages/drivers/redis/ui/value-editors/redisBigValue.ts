//! 大 value 哨兵判定（PRD §3.3「大 value」/ §5 点名的 `redisBigValue.ts`）。
//!
//! PRD 给的探针是 `GETRANGE 0 65537`——回包长度 > 65536 即判定"这是个大对象，
//! 别把它整个塞进编辑器"。该探针的目的从来不是多一次往返，而是**先量长度再决定
//! 要不要传整包**；`get_key_raw` 已经在同一次 pipeline 里回了 `STRLEN`
//! （`ValueFrame.logicalLen`），也就是同一个判定位，所以这里直接读它：
//! **零额外命令**，语义与 GETRANGE 探针等价。
//!
//! 只有 string 键的 `logicalLen` 是字节数：集合类（hash/list/set/zset/stream）它是
//! HLEN/LLEN/SCARD/ZCARD/XLEN 的**元素个数**，拿元素个数和 64 KiB 比会把一个
//! 十万成员的 hash 误判成大 value，所以必须先按类型短路。

import type { ValueFrame } from '../shared/types';

/** 哨兵阈值：与 PRD §3.3 的 `GETRANGE 0 65537` 一致（严格大于即判定）。 */
export const BIG_VALUE_SENTINEL_BYTES = 65_536;

/** 只有 string 通道的 `logicalLen` 才是字节长度。 */
export const BYTE_LENGTH_KEY_TYPES: readonly string[] = ['string'];

export interface BigValueVerdict {
  /** true ⇒ 命中大 value / 后端截断，编辑区进入 I-5 只读态②。 */
  big: boolean;
  /** 后端 `get_key_raw` 已把载荷截掉（`ValueFrame.truncated`）。 */
  truncated: boolean;
  /** 前端按长度阈值判定（未截断但超过哨兵）。 */
  overSentinel: boolean;
  /** 参与判定的字节数；不可得时 null。 */
  bytes: number | null;
  /** 阈值，供提示条文案回填。 */
  limitBytes: number;
}

const NO_VERDICT: BigValueVerdict = {
  big: false,
  truncated: false,
  overSentinel: false,
  bytes: null,
  limitBytes: BIG_VALUE_SENTINEL_BYTES,
};

/**
 * 折叠 `truncated` 与长度阈值两条判定路径。
 *
 * `frame` 允许为 null（`get_key_raw` 是可选增强，失败时 frame 为空），此时不判定
 * 为大 value —— 读不到长度不等于值很大，把未知当成"禁编辑"会误伤正常键。
 */
export function judgeBigValue(
  frame: Pick<ValueFrame, 'keyType' | 'logicalLen' | 'truncated'> | null | undefined,
  limitBytes: number = BIG_VALUE_SENTINEL_BYTES,
): BigValueVerdict {
  if (!frame) return { ...NO_VERDICT, limitBytes };
  const truncated = frame.truncated === true;
  const isByteLengthType = BYTE_LENGTH_KEY_TYPES.includes(frame.keyType);
  const len = Number.isFinite(frame.logicalLen) ? frame.logicalLen : null;
  const overSentinel = isByteLengthType && len !== null && len > limitBytes;
  return {
    big: truncated || overSentinel,
    truncated,
    overSentinel,
    bytes: isByteLengthType ? len : null,
    limitBytes,
  };
}
