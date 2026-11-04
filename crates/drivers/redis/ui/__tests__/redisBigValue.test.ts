/**
 * 大 value 哨兵判定（本轨 E-3，PRD §3.3「大 value」/ §4 I-5 只读态②）。
 *
 * 断言口径：只看判定结果的位与字节数，不读任何英文字面量文案。
 * 关键负例：集合类的 `logicalLen` 是**元素个数**，绝不能拿来跟 64 KiB 比。
 */
import { describe, expect, it } from 'vitest';
import type { ValueFrame } from '../shared/types';
import { BIG_VALUE_SENTINEL_BYTES, judgeBigValue } from '../value-editors/redisBigValue';

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

describe('judgeBigValue', () => {
  it('uses the PRD sentinel: strictly more than 64 KiB is big', () => {
    expect(BIG_VALUE_SENTINEL_BYTES).toBe(65_536);
    expect(judgeBigValue(frame({ logicalLen: BIG_VALUE_SENTINEL_BYTES })).big).toBe(false);
    const verdict = judgeBigValue(frame({ logicalLen: BIG_VALUE_SENTINEL_BYTES + 1 }));
    expect(verdict).toMatchObject({ big: true, overSentinel: true, truncated: false });
    expect(verdict.bytes).toBe(BIG_VALUE_SENTINEL_BYTES + 1);
    expect(verdict.limitBytes).toBe(BIG_VALUE_SENTINEL_BYTES);
  });

  it('honours a caller-supplied threshold without mutating the default', () => {
    const verdict = judgeBigValue(frame({ logicalLen: 2_000 }), 1_000);
    expect(verdict.overSentinel).toBe(true);
    expect(verdict.limitBytes).toBe(1_000);
    expect(judgeBigValue(frame({ logicalLen: 2_000 })).big).toBe(false);
  });

  it('folds the backend truncation flag into the same verdict', () => {
    const verdict = judgeBigValue(frame({ truncated: true, logicalLen: 6_000_000, rawB64: null }));
    expect(verdict).toMatchObject({ big: true, truncated: true, overSentinel: true });
  });

  it('reports a truncated payload even when the length stays under the sentinel', () => {
    const verdict = judgeBigValue(frame({ truncated: true, logicalLen: 10 }));
    expect(verdict.big).toBe(true);
    expect(verdict.overSentinel).toBe(false);
  });

  it('never compares a collection element count against a byte threshold', () => {
    for (const keyType of ['hash', 'list', 'set', 'zset', 'stream']) {
      const verdict = judgeBigValue(frame({ keyType, logicalLen: 100_000 }));
      expect(verdict.big, keyType).toBe(false);
      expect(verdict.bytes, keyType).toBeNull();
    }
  });

  it('treats a missing frame as "unknown", not as "huge"', () => {
    // get_key_raw 是可选增强：读不到长度就禁编辑会误伤正常键。
    for (const missing of [null, undefined]) {
      const verdict = judgeBigValue(missing);
      expect(verdict).toMatchObject({
        big: false,
        truncated: false,
        overSentinel: false,
        bytes: null,
      });
    }
  });

  it('tolerates a non-finite logical length', () => {
    const verdict = judgeBigValue(
      frame({ logicalLen: Number.NaN as unknown as number }) as unknown as ValueFrame,
    );
    expect(verdict.big).toBe(false);
    expect(verdict.bytes).toBeNull();
  });
});
