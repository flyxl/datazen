/**
 * 推送事件流：一个可被 `for await … of` 消费的缓冲队列。
 *
 * 为什么需要它：`BackendTransport.subscribe` 返回 `AsyncIterable`，而 Tauri IPC 与
 * HTTP SSE 的推送节奏不同（前者由宿主主动推、后者由服务端事件源驱动）。前端要的是
 * **与传输无关**的消费形态，所以本包提供一个中立的推送队列，传输适配器往里 `push`，
 * 消费方照常 `for await`。
 *
 * **`return()` 的纪律**（概要 §7.3）：切换 backendId 时旧门面的流被 `return()`，
 * 这**只是结束迭代**。它**不得**触发 `cancelExecution`——那是一次业务动作，
 * 必须由调用方显式发起。收尾时只调 `detach`（`onDetach`，由适配器用来释放底层订阅），
 * 绝不碰取消接口。
 *
 * **溢出策略**：缓冲满时**报错并结束**，绝不静默丢块。丢块会产出「看起来完整其实缺块」
 * 的结果，这在连接 §13 的语义下比直接失败危险得多。
 */

import type { ConnectionEvent, EventEnvelope } from './types/event';

/** 推送队列句柄。`push` / `fail` / `end` 由传输适配器调用。 */
export interface PushEventStream<T> extends AsyncIterableIterator<T> {
  /** 投递一个值。流已结束则丢弃（结束是终态，不得复活）。 */
  push(value: T): boolean;
  /** 以错误结束。已在等待的消费者立刻拿到拒绝。 */
  fail(error: unknown): void;
  /** 正常结束。已在等待的消费者拿到 `{ done: true }`。 */
  end(): void;
  /** 是否已结束（正常或异常）。 */
  readonly closed: boolean;
  /** 当前缓冲深度，测试与诊断用。 */
  readonly buffered: number;
}

/** 构造参数。 */
export interface PushEventStreamOptions {
  /**
   * 消费方结束迭代（`return()`）时的收尾回调。适配器在这里释放底层订阅。
   * **不得**在这里发起任何业务取消。
   */
  readonly onDetach?: () => void;
  /** 缓冲上限，超出即报错结束。默认 4096。 */
  readonly maxBuffered?: number;
}

const DEFAULT_MAX_BUFFERED = 4096;

interface Waiter<T> {
  readonly resolve: (result: IteratorResult<T>) => void;
  readonly reject: (error: unknown) => void;
}

/** 内部信号：缓冲溢出。用 `Error` 抛出，`name` 固定便于测试与日志定位。 */
export class EventStreamOverflowError extends Error {
  readonly maxBuffered: number;

  constructor(maxBuffered: number) {
    super(
      `Event stream buffer overflowed at ${maxBuffered} pending items; the subscription was closed instead of dropping events.`,
    );
    this.name = 'EventStreamOverflowError';
    this.maxBuffered = maxBuffered;
  }
}

interface PushStreamInternal<T> {
  readonly buffer: T[];
  readonly waiters: Waiter<T>[];
  settled: boolean;
  failure: unknown;
  detachCalled: boolean;
}

/**
 * 创建一个推送事件流。
 *
 * 实现要点：等待中的 `next()` 用挂起的 Promise 表示，缓冲与等待队列互斥使用，
 * 因此不需要在 push 时扫描等待者，也不需要任何定时器（无轮询、无 sleep）。
 */
export function createPushEventStream<T>(options: PushEventStreamOptions = {}): PushEventStream<T> {
  const maxBuffered = options.maxBuffered ?? DEFAULT_MAX_BUFFERED;
  const state: PushStreamInternal<T> = {
    buffer: [],
    waiters: [],
    settled: false,
    failure: undefined,
    detachCalled: false,
  };

  const settleWaiter = (waiter: Waiter<T>, result: IteratorResult<T>): void => {
    waiter.resolve(result);
  };

  const fail = (error: unknown): void => {
    if (state.settled) return;
    state.settled = true;
    state.failure = error;
    const pending = state.waiters.splice(0, state.waiters.length);
    for (const waiter of pending) waiter.reject(error);
  };

  const end = (): void => {
    if (state.settled) return;
    state.settled = true;
    const pending = state.waiters.splice(0, state.waiters.length);
    for (const waiter of pending) settleWaiter(waiter, { value: undefined, done: true });
  };

  const stream: PushEventStream<T> = {
    push(value: T): boolean {
      if (state.settled) return false;
      const waiter = state.waiters.shift();
      if (waiter !== undefined) {
        settleWaiter(waiter, { value, done: false });
        return true;
      }
      if (state.buffer.length >= maxBuffered) {
        fail(new EventStreamOverflowError(maxBuffered));
        return false;
      }
      state.buffer.push(value);
      return true;
    },
    fail,
    end,
    get closed(): boolean {
      return state.settled;
    },
    get buffered(): number {
      return state.buffer.length;
    },
    next(): Promise<IteratorResult<T>> {
      if (state.buffer.length > 0) {
        const value = state.buffer.shift() as T;
        return Promise.resolve({ value, done: false });
      }
      if (state.settled) {
        if (state.failure !== undefined) return Promise.reject(state.failure);
        return Promise.resolve({ value: undefined, done: true });
      }
      return new Promise<IteratorResult<T>>((resolve, reject) => {
        state.waiters.push({ resolve, reject });
      });
    },
    async return(value?: unknown): Promise<IteratorResult<T>> {
      if (!state.detachCalled) {
        state.detachCalled = true;
        options.onDetach?.();
      }
      // 结束迭代后立刻让等待中的消费者醒来，否则 `break` 出去的 `for await` 会挂死。
      end();
      if (state.failure !== undefined) return Promise.reject(state.failure);
      return { value: value as T, done: true };
    },
    throw(error?: unknown): Promise<IteratorResult<T>> {
      if (!state.detachCalled) {
        state.detachCalled = true;
        options.onDetach?.();
      }
      fail(error ?? new Error('Event stream was thrown.'));
      return Promise.reject(state.failure);
    },
    [Symbol.asyncIterator](): AsyncIterableIterator<T> {
      return stream;
    },
  };

  return stream;
}

/** 连接的推送事件流。 */
export type ConnectionEventStream = PushEventStream<EventEnvelope<ConnectionEvent>>;

/** 收集流中全部事件直到结束。测试与小窗口消费用；长流不要用（无界增长）。 */
export async function drainStream<T>(stream: AsyncIterable<T>): Promise<T[]> {
  const collected: T[] = [];
  for await (const item of stream) collected.push(item);
  return collected;
}

/**
 * 保留一个流的最近 `limit` 条，用于「只关心最新状态」的订阅方。
 * 超出上限时丢**最旧**的——状态类消息可以丢时序，但订阅方会看到序号空洞。
 */
export async function collectLatest<T>(stream: AsyncIterable<T>, limit: number): Promise<T[]> {
  const keep = Math.max(1, Math.trunc(limit));
  const collected: T[] = [];
  for await (const item of stream) {
    collected.push(item);
    if (collected.length > keep) collected.shift();
  }
  return collected;
}
