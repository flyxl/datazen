/**
 * Web SSE 事件源（WR-E2）——契约 §6.6 v1.1 的两步订阅流水线：
 *
 * 1. `POST /api/v1/events/subscriptions`（普通 fetch：认证链内 + CSRF 头，
 *    web-server-host §5.2/§8.1）→ `{ streamId }`——streamId 由服务端 CSPRNG 铸造、
 *    注册表 TTL 内一次性消费，**重连先 POST 再 GET，旧 id 不重放**；
 * 2. `GET /api/v1/events/{streamId}?afterSequence=N`（浏览器原生 `EventSource`）：
 *    同源 cookie 自动携带；自动重连时浏览器自理 `Last-Event-ID` 头（§8.2
 *    的「优先头、其次查询参数」，首连无头、查询参数兜底）。
 *
 * wire 形态（WR-D5 落地，`crates/datazen-server/src/http/sse.rs`）：帧 = `id: <sequence>` /
 * `event: <payload kind>` / `data: <envelope JSON>`；信封与桌面 IPC 同形
 * （camelCase 逐字段；`session_handle` 是运行时值，wire 保留）。三类特殊帧：
 *
 * - `streamResetRequired` **合成帧**（缺口/慢消费者，无 `id` 行，data 只有
 *   `{reason}`）→ 映射为 types/ 词汇的 `streamResetRequired` 信封投递后终结；
 *   合成序列记 0：消费方按 `payload.kind` 终结（不参与续传游标），带
 *   `afterSequence` 的重订阅里 0 会被 streams.ts 的游标过滤掉——那时「流已
 *   结束」本身就是消费方的重建信号（§8.2：用 get 快照重建）；
 * - `auth_required` / `permission_revoked` / `session_lost` **终态事件**
 *   （§8.4，不在 types/ 的 `ConnectionEvent` 词汇内）→ 立即 `close()` 并
 *   结束迭代——§8.4 的客户端义务：先 close 再跳登录，否则浏览器自动重连
 *   形成登录风暴；消费方看到的迭代终局会驱动其恢复路径（下一个 API 调用
 *   以 401 显式失败）；
 * - `: keepalive` 注释行：`EventSource` 自行忽略，不进本层。
 *
 * 取消（§8.5）：结束迭代 / `cancel(opaque)` 只解除订阅（`EventSource.close()`），
 * **从不取消正在执行的 SQL**——显式取消只有 `cancelExecution` 一条路径。
 */

import { ApiError } from '@datazen/backend-client';
import type { ConnectionEvent, EventEnvelope } from '@datazen/backend-client';

/** EventSource 工厂（默认 `globalThis.EventSource`；测试可注入假体）。 */
export type EventSourceFactory = (url: string) => EventSource;

/** 信封里出现的服务端 payload 词汇（`ConnectionEvent` 的 kind 全集）。 */
const PAYLOAD_KINDS: readonly string[] = [
  'sessionChanged',
  'executionChanged',
  'resultChunk',
  'streamResetRequired',
];

/** §8.4 的终态事件名（先发后关流；types/ 词汇之外，不投递只终结）。 */
const TERMINAL_KINDS: readonly string[] = ['auth_required', 'permission_revoked', 'session_lost'];

/** 客户端投递缓冲上限（桌面同款 2048；超限即失败，不做无限缓冲）。 */
const CLIENT_BUFFER_LIMIT = 2048;

/** 本模块对宿主传输的依赖面：登记 POST、握手 URL、取消登记表。 */
export interface EventStreamContext {
  /** baseUrl（含 /api/v1），握手 URL 由它拼出。 */
  baseUrl: string;
  /** 第一步：POST /events/subscriptions（含 CSRF 头），返回铸造的 wire streamId。 */
  postSubscription: () => Promise<string>;
  eventSourceFactory: EventSourceFactory;
  /** 第二步握手成功后登记取消表（cancel(streamId) → close）。 */
  registerStream: (streamId: string, close: () => void) => void;
  unregisterStream: (streamId: string) => void;
}

/** subscribe('subscribeEvents', payload) 的 web 语义参数。 */
export interface EventStreamRequest {
  /**
   * 调用方的**逻辑**流 id（桌面语义的 `receipt.streamId`）。HTTP v1.1 由注册
   * 表铸造一次性 wire id，信封 `streamId` 在本层回写为逻辑 id——消费方
   * （如 ExecutionProjection）按 `receipt.streamId` 过滤事件，两种传输必须
   * 给它同一个值。
   */
  logicalStreamId: string;
  /** 续传游标：十进制字符串上 wire（§8.2「不得 Number()」，本层只 stringify）。 */
  afterSequence: number | null;
}

/**
 * 把 wire 帧解析为 types/ 信封；不可读/未知词汇返回 null（忽略该帧，不终结
 * ——终态裁决在服务端心跳复核，客户端不替它做）。
 */
function toEnvelope(
  data: string,
  frameKind: string,
  logicalStreamId: string,
): EventEnvelope<ConnectionEvent> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data) as unknown;
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return null;
  const record = parsed as Record<string, unknown>;

  // 适配层合成帧：无 id 行、data 只有 {reason}（D5 wire；不是完整信封）。
  if (frameKind === 'streamResetRequired' && !('payload' in record)) {
    const reason =
      typeof record['reason'] === 'string' ? record['reason'] : 'the event stream must be reset';
    // 合成信封按 types/ 形状补零值；唯一确定的字段是 payload.kind 与 reason。
    return {
      streamId: logicalStreamId,
      sequence: 0,
      runtimeEpoch: '',
      executionId: null,
      jobId: null,
      sessionHandle: null,
      contextRevision: null,
      payload: { kind: 'streamResetRequired', reason },
    } as unknown as EventEnvelope<ConnectionEvent>;
  }

  // 完整信封与桌面 IPC 同形；streamId 回写为调用方逻辑流 id（见 request 注释）。
  record['streamId'] = logicalStreamId;
  return record as unknown as EventEnvelope<ConnectionEvent>;
}

/** 建立两步流水线的 AsyncIterable。惰性：迭代开始才登记 + 握手。 */
export function createEventStreamIterable(
  context: EventStreamContext,
  request: EventStreamRequest,
): AsyncIterable<EventEnvelope<ConnectionEvent>> {
  return {
    [Symbol.asyncIterator]() {
      const queue: EventEnvelope<ConnectionEvent>[] = [];
      let failure: unknown = null;
      let finished = false;
      let closed = false;
      let registered: string | null = null;
      let source: EventSource | null = null;
      let wake: (() => void) | null = null;

      const signal = (): void => {
        const resume = wake;
        wake = null;
        resume?.();
      };

      // 终局：关流（§8.4 的 close 义务）→ 摘登记 → 唤醒等待者。
      const finish = (): void => {
        if (finished) return;
        finished = true;
        if (registered !== null) {
          context.unregisterStream(registered);
          registered = null;
        }
        source?.close();
        source = null;
        signal();
      };

      const fail = (error: unknown): void => {
        failure = error;
        finish();
      };

      const onFrame = (event: MessageEvent): void => {
        if (finished || closed) return;
        if (queue.length >= CLIENT_BUFFER_LIMIT) {
          fail(
            new ApiError(
              'ServiceUnavailable',
              'Event delivery buffer overflow; resubscribe to rebuild the projection.',
            ),
          );
          return;
        }
        const envelope = toEnvelope(event.data, event.type, request.logicalStreamId);
        if (envelope === null) return;
        queue.push(envelope);
        signal();
      };

      const onTerminal = (): void => {
        // 终态事件不在 types/ 词汇内：close + 终结（迭代返回 done）。
        finish();
      };

      const onError = (): void => {
        if (source === null) return;
        // CONNECTING = 浏览器自动重连（Last-Event-ID 自理），继续等待；
        // CLOSED = 致命失败（如登记 TTL 过期后的 404）→ 终结，消费方走
        // 恢复/重订阅（新订阅会重新 POST 铸造新 id，v1.1 语义）。
        if (source.readyState === EventSource.CLOSED) finish();
      };

      const pump = (async (): Promise<void> => {
        const wireStreamId = await context.postSubscription();
        if (closed) return;
        registered = wireStreamId;
        context.registerStream(wireStreamId, () => finish());

        const after =
          request.afterSequence === null ? '' : `?afterSequence=${request.afterSequence}`;
        source = context.eventSourceFactory(
          `${context.baseUrl}/events/${encodeURIComponent(wireStreamId)}${after}`,
        );
        for (const kind of PAYLOAD_KINDS) {
          source.addEventListener(kind, onFrame);
        }
        for (const kind of TERMINAL_KINDS) {
          source.addEventListener(kind, onTerminal);
        }
        source.onerror = onError;
      })();

      // pump 内部已逐段 try/catch 的等价物：任何意外都折成迭代失败而非
      // 未处理拒绝。
      pump.catch((error: unknown) => fail(error));

      return {
        next: async (): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> => {
          while (queue.length === 0) {
            if (failure !== null) {
              const error = failure;
              failure = null;
              throw error;
            }
            if (finished) return { done: true, value: undefined };
            await new Promise<void>((resolve) => {
              wake = resolve;
            });
          }
          const next = queue.shift();
          if (next === undefined) return { done: true, value: undefined };
          return { done: false, value: next };
        },
        return: async (): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> => {
          // 消费方跳出迭代 = 本地解除订阅（§8.5：不取消执行）。
          closed = true;
          finish();
          return { done: true, value: undefined };
        },
      };
    },
  };
}
