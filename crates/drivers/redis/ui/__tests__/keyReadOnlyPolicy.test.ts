/**
 * I-5 只读态判定（本轨 E-3，PRD §4 I-5「只读态只剩两种，且必须给原因」）。
 *
 * 穷举式口径：把 9 个视图 × {正常载荷 / 超哨兵 / 后端截断 / 集合类 / 无帧} 全跑一遍，
 * 只允许 `hex`、`binary` 与两种大 value 命中只读；其余一律可编辑。
 * 任何额外命中都意味着 E-2 删掉的「查看/编辑」两态换了个地方复活。
 */
import { describe, expect, it } from 'vitest';
import type { ValueFrame } from '../shared/types';
import { VIEWS, type ViewMode } from '../value-editors/valueView/views';
import { CODECS } from '../value-editors/valueView/codecs';
import {
  BYTE_ONLY_VIEWS,
  isByteOnlyView,
  READ_ONLY_REASONS,
  resolveReadOnlyPolicy,
} from '../value-editors/keyReadOnlyPolicy';
import { BIG_VALUE_SENTINEL_BYTES } from '../value-editors/redisBigValue';

function frame(over: Partial<ValueFrame> = {}): ValueFrame {
  return {
    key: 'user:1',
    keyType: 'string',
    ttl: -1,
    logicalLen: 5,
    memBytes: 5,
    rawB64: 'aGVsbG8=',
    truncated: false,
    ...over,
  };
}

const SMALL = frame();
const OVER_SENTINEL = frame({ logicalLen: BIG_VALUE_SENTINEL_BYTES + 1 });
const TRUNCATED = frame({ truncated: true, logicalLen: 6_000_000, rawB64: null });
const HUGE_HASH = frame({ keyType: 'hash', logicalLen: 100_000, rawB64: null });

describe('resolveReadOnlyPolicy — view dimension', () => {
  it('lists the byte-projection views exactly once and they are all real views', () => {
    expect([...BYTE_ONLY_VIEWS].sort()).toEqual(['binary', 'hex']);
    for (const view of BYTE_ONLY_VIEWS) {
      expect(VIEWS).toContain(view);
      expect(isByteOnlyView(view)).toBe(true);
    }
  });

  it('is editable for every non-byte view on a normal payload', () => {
    const textViews = VIEWS.filter((v) => !isByteOnlyView(v));
    expect(textViews.length).toBe(VIEWS.length - BYTE_ONLY_VIEWS.length);
    for (const view of textViews) {
      const policy = resolveReadOnlyPolicy({ frame: SMALL, view });
      expect(policy.readOnly, view).toBe(false);
      expect(policy.reason, view).toBeNull();
    }
  });

  it('makes exactly hex + binary read-only, each with its own reason', () => {
    for (const view of BYTE_ONLY_VIEWS) {
      const policy = resolveReadOnlyPolicy({ frame: SMALL, view });
      expect(policy.readOnly, view).toBe(true);
      expect(policy.reason?.id, view).toBe('binary-view');
      expect(policy.reason?.i18nKey, view).toBe(READ_ONLY_REASONS['binary-view'].i18nKey);
    }
  });

  it('never leaves a read-only surface without a reason (and vice versa)', () => {
    for (const view of VIEWS) {
      const policy = resolveReadOnlyPolicy({ frame: SMALL, view });
      expect(policy.readOnly).toBe(policy.reason !== null);
      if (policy.reason) expect(policy.reason.i18nKey.startsWith('redis.detail.')).toBe(true);
    }
  });

  it('tolerates a missing view selection', () => {
    for (const view of [null, undefined] as Array<ViewMode | null | undefined>) {
      expect(resolveReadOnlyPolicy({ frame: SMALL, view }).readOnly).toBe(false);
      expect(isByteOnlyView(view)).toBe(false);
    }
  });
});

describe('resolveReadOnlyPolicy — payload dimension', () => {
  it('flags a byte-only view first: it is the actionable one', () => {
    const policy = resolveReadOnlyPolicy({ frame: TRUNCATED, view: 'hex' });
    expect(policy.readOnly).toBe(true);
    expect(policy.reason?.id).toBe('binary-view');
    // 数据事实仍在结果里，文案层要多少字节都能拿到。
    expect(policy.bigValue.truncated).toBe(true);
  });

  it('reads a truncated payload as read-only in any text view', () => {
    for (const view of VIEWS.filter((v) => !isByteOnlyView(v))) {
      const policy = resolveReadOnlyPolicy({ frame: TRUNCATED, view });
      expect(policy.readOnly, view).toBe(true);
      expect(policy.reason?.id, view).toBe('big-value');
    }
  });

  it('reads an over-sentinel string as read-only', () => {
    const policy = resolveReadOnlyPolicy({ frame: OVER_SENTINEL, view: 'utf8' });
    expect(policy.readOnly).toBe(true);
    expect(policy.reason?.id).toBe('big-value');
    expect(policy.bigValue.bytes).toBe(BIG_VALUE_SENTINEL_BYTES + 1);
  });

  // BUG-006：I-5 ② 的两个成因必须各说各的事实文案（同一 big-value 状态）。
  it('splits the big-value copy: truncated says incomplete, over-sentinel says complete', () => {
    const truncatedPolicy = resolveReadOnlyPolicy({ frame: TRUNCATED, view: 'utf8' });
    expect(truncatedPolicy.reason?.i18nKey).toBe(READ_ONLY_REASONS['big-value'].i18nKey);

    const overPolicy = resolveReadOnlyPolicy({ frame: OVER_SENTINEL, view: 'utf8' });
    expect(overPolicy.reason?.i18nKey).toBe('redis.detail.readonly.bigValueComplete');
    // 定位口径不变：仍是 big-value 只读态，分支只换文案事实。
    expect(overPolicy.reason?.id).toBe('big-value');
    expect(overPolicy.readOnly).toBe(true);
  });

  it('keeps a large collection editable (element count is not bytes)', () => {
    const policy = resolveReadOnlyPolicy({ frame: HUGE_HASH, view: 'utf8' });
    expect(policy.readOnly).toBe(false);
    expect(policy.bigValue.big).toBe(false);
  });

  it('treats a missing frame as editable, not as unknown-huge', () => {
    expect(resolveReadOnlyPolicy({}).readOnly).toBe(false);
    expect(resolveReadOnlyPolicy({ frame: null, view: 'utf8' }).readOnly).toBe(false);
  });
});

describe('resolveReadOnlyPolicy — codec choice is not a read-only axis', () => {
  it('ignores decoding entirely: a codec preview never locks the editor', () => {
    // 旧实现里「已解码 ⇒ 只能看」是第三种只读态，E-2/E-3 要它消失。
    // 判定函数没有 codec 入参即是保证；这里把 9 个 codec 全枚举一遍视图维度。
    for (const _codec of CODECS) {
      for (const view of VIEWS) {
        const policy = resolveReadOnlyPolicy({ frame: SMALL, view });
        expect(policy.readOnly, `${_codec}/${view}`).toBe(isByteOnlyView(view));
      }
    }
    expect(CODECS.length).toBe(9);
  });
});
