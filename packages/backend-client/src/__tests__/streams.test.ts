/**
 * 推送流测试。
 *
 * 三条纪律各有断言：
 * 1. `return()` 只调 `onDetach`，**不**触发任何业务取消；
 * 2. 缓冲溢出**报错结束**，绝不静默丢事件（丢块会产出「看起来完整其实缺块」的结果）；
 * 3. 结束后不得复活（`push` 返回 false）。
 */

import { describe, expect, it } from 'vitest';

import {
  EventStreamOverflowError,
  collectLatest,
  createPushEventStream,
  drainStream,
} from '@datazen/backend-client';

describe('createPushEventStream', () => {
  it('push 的值按顺序交付给消费者', async () => {
    const stream = createPushEventStream<number>();
    expect(stream.push(1)).toBe(true);
    expect(stream.push(2)).toBe(true);
    stream.end();
    await expect(drainStream(stream)).resolves.toEqual([1, 2]);
    expect(stream.closed).toBe(true);
  });

  it('等待中的 next() 在 push 时立刻兑现', async () => {
    const stream = createPushEventStream<string>();
    const pending = stream.next();
    stream.push('x');
    await expect(pending).resolves.toEqual({ value: 'x', done: false });
    await stream.return?.();
  });

  it('结束后 push 返回 false（终态不得复活）', async () => {
    const stream = createPushEventStream<number>();
    stream.end();
    expect(stream.push(1)).toBe(false);
    expect(stream.buffered).toBe(0);
  });

  it('fail 让等待中的消费者拿到拒绝', async () => {
    const stream = createPushEventStream<number>();
    const pending = stream.next();
    stream.fail(new Error('stream broke'));
    await expect(pending).rejects.toThrow('stream broke');
    await expect(stream.next()).rejects.toThrow('stream broke');
  });

  it('return() 只调一次 onDetach，且不改变失败语义', async () => {
    let detachCount = 0;
    const stream = createPushEventStream<number>({ onDetach: () => (detachCount += 1) });
    await stream.return?.();
    await stream.return?.();
    expect(detachCount).toBe(1);
    expect(stream.closed).toBe(true);
  });

  it('缓冲溢出时以 EventStreamOverflowError 结束，而不是丢弃事件', async () => {
    const stream = createPushEventStream<number>({ maxBuffered: 2 });
    expect(stream.push(1)).toBe(true);
    expect(stream.push(2)).toBe(true);
    expect(stream.push(3)).toBe(false);
    expect(stream.closed).toBe(true);

    // 缓冲里的两条仍按序交出；第三条以错误形式暴露，而不是被悄悄丢掉。
    await expect(stream.next()).resolves.toEqual({ value: 1, done: false });
    await expect(stream.next()).resolves.toEqual({ value: 2, done: false });
    await expect(stream.next()).rejects.toBeInstanceOf(EventStreamOverflowError);
  });

  it('throw() 既 detach 也让流失败', async () => {
    let detached = false;
    const stream = createPushEventStream<number>({
      onDetach: () => {
        detached = true;
      },
    });
    await expect(stream.throw?.(new Error('consumer exploded'))).rejects.toThrow(
      'consumer exploded',
    );
    expect(detached).toBe(true);
    expect(stream.closed).toBe(true);
  });

  it('collectLatest 只保留最近 limit 条', async () => {
    const stream = createPushEventStream<number>();
    for (const value of [1, 2, 3, 4]) stream.push(value);
    stream.end();
    await expect(collectLatest(stream, 2)).resolves.toEqual([3, 4]);
  });
});
