/**
 * WR-E2 SSE 事件源测试（P2b/E2）——契约 §6.6 v1.1 的两步订阅流水线，经
 * `createWebBackendTransport().subscribe()` 全链路驱动（假 fetch + 注入的
 * 假 EventSource 工厂）：
 *
 * 1. `POST /api/v1/events/subscriptions`：CSRF 头 + `credentials: 'include'`
 *    + 无请求体 → `{ streamId }`（服务端铸造的一次性 wire id）；
 * 2. `GET /api/v1/events/{streamId}?afterSequence=N`：流 URL 由 wire id 插值
 *    （encodeURIComponent 抽查）、afterSequence 十进制上 wire、null 无查询串；
 *
 * 帧语义（WR-D5 wire）：`id`/`event`/`data` 帧映射为 `EventEnvelope` 且
 * `streamId` 回写为调用方逻辑流 id；不可读帧/未登记事件种（心跳注释行不会
 * 产生事件）忽略不终结；`streamResetRequired` 合成帧（无 id 行、data 只有
 * `{reason}`）→ sequence 0 合成信封投递、流继续；`auth_required` /
 * `permission_revoked` / `session_lost` 终态事件 → `EventSource.close()` +
 * 迭代终结（§8.4 先 close 再恢复）；`cancel(wireStreamId)` 走登记表 close。
 *
 * 测试基建注意：`createEventStreamIterable` 每次 `[Symbol.asyncIterator]()`
 * 都铸造一条全新流水线——所有用例一律**单迭代器**驱动（`startStream` 取一次
 * 迭代器后只对它 next/return），避免二次握手互相污染。
 *
 * `streamResetRequired` 的「投递后终结」在本实现里表现为普通信封投递
 * （`onFrame` 只入队），终结裁决由消费方按 `payload.kind` 做——测试以实现
 * 为准断言「投递 + 流继续」。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@datazen/backend-client';
import type {
  ConnectionEvent,
  EventEnvelope,
  SubscribeEventsRequest,
} from '@datazen/backend-client';

import { createWebBackendTransport } from '../backendTransport';
import type { EventSourceFactory } from '../eventStreamSource';

type Envelope = EventEnvelope<ConnectionEvent>;
type StreamIterator = AsyncIterator<Envelope> & { return?: () => Promise<IteratorResult<Envelope>> };

/* ------------------------------------------------------------------ */
/* 假 EventSource：记录 URL 与监听器，手动 emit 帧供断言。               */
/* ------------------------------------------------------------------ */

class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static created: FakeEventSource[] = [];

  readonly readyState = FakeEventSource.OPEN;
  readonly url: string;
  onerror: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;
  closeCount = 0;

  private readonly listeners = new Map<string, (event: MessageEvent) => void>();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.created.push(this);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void): void {
    this.listeners.set(type, listener);
  }

  emit(type: string, data: string): void {
    this.listeners.get(type)?.({ type, data } as unknown as MessageEvent);
  }

  close(): void {
    this.closeCount += 1;
  }

  get closed(): boolean {
    return this.closeCount > 0;
  }
}

const factory: EventSourceFactory = (url: string) => new FakeEventSource(url) as unknown as EventSource;

/* ------------------------------------------------------------------ */
/* 假 fetch + 装配 + 轮询基建。                                          */
/* ------------------------------------------------------------------ */

const CSRF_TOKEN = 'csrf-token-sse';

interface RecordedRequest {
  url: RequestInfo | URL;
  init: RequestInit;
}

const calls: RecordedRequest[] = [];

function stubPostSubscription(status = 200, body: unknown = { streamId: 'wire-abc' }): void {
  calls.length = 0;
  FakeEventSource.created.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      calls.push({ url, init: init ?? {} });
      return new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
}

/** SubscribeEventsRequest 的 Id/Counter 是品牌类型：测试里经 unknown 注入。 */
function subscribeRequest(logicalStreamId: string, afterSequence: number | null): SubscribeEventsRequest {
  return { streamId: logicalStreamId, afterSequence } as unknown as SubscribeEventsRequest;
}

function newTransport(): ReturnType<typeof createWebBackendTransport> {
  return createWebBackendTransport({
    csrfTokenProvider: () => CSRF_TOKEN,
    eventSourceFactory: factory,
  });
}

interface StreamBundle {
  transport: ReturnType<typeof createWebBackendTransport>;
}

function bundle(): StreamBundle {
  return { transport: newTransport() };
}

/** 等待异步两步握手完成（POST resolve → EventSource 铸造）。 */
async function untilHandshake(): Promise<FakeEventSource> {
  for (let i = 0; i < 200 && FakeEventSource.created.length === 0; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  const source = FakeEventSource.created[0];
  if (source === undefined) throw new Error('the two-step handshake never produced an EventSource');
  return source;
}

/** 单迭代器取流：`[Symbol.asyncIterator]()` 只调用一次，握手后返回迭代器。 */
async function startStream(
  transport: ReturnType<typeof createWebBackendTransport>,
  request: SubscribeEventsRequest,
): Promise<{ iterator: StreamIterator; source: FakeEventSource }> {
  const iterator = transport.subscribe!('subscribeEvents', request)[Symbol.asyncIterator]() as StreamIterator;
  const source = await untilHandshake();
  return { iterator, source };
}

/** 连取至多 limit 个信封（迭代先 done 即提前返回）。 */
async function take(iterator: StreamIterator, limit: number): Promise<Envelope[]> {
  const received: Envelope[] = [];
  while (received.length < limit) {
    const step = await iterator.next();
    if (step.done === true) break;
    received.push(step.value);
  }
  return received;
}

/** 完整信封 wire 形状（桌面 IPC 同形；streamId 在实现里被回写）。 */
function wireEnvelope(sequence: string): Record<string, unknown> {
  return {
    streamId: 'wire-abc',
    sequence,
    runtimeEpoch: 'epoch-1',
    executionId: 'exec-1',
    jobId: null,
    sessionHandle: null,
    contextRevision: null,
    payload: { kind: 'executionChanged', executionId: 'exec-1', state: 'running' },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/* ------------------------------------------------------------------ */
/* 两步握手。                                                            */
/* ------------------------------------------------------------------ */

describe('两步订阅流水线（§6.6 v1.1）', () => {
  it('先 POST /events/subscriptions（CSRF + credentials + 无体）再 EventSource GET 流 URL', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { source } = await startStream(transport, subscribeRequest('logical-1', 41));

    expect(calls).toHaveLength(1);
    const post = calls[0]!;
    expect(String(post.url)).toBe('/api/v1/events/subscriptions');
    expect(post.init.method).toBe('POST');
    expect(post.init.credentials).toBe('include');
    expect(post.init.body).toBeUndefined();
    const headers = new Headers(post.init.headers);
    expect(headers.get('x-dz-csrf-token')).toBe(CSRF_TOKEN);
    expect(headers.get('content-type')).toBeNull();

    expect(source.url).toBe('/api/v1/events/wire-abc?afterSequence=41');
  });

  it('afterSequence=null：GET 流 URL 不带查询参数（首连语义）', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { source } = await startStream(transport, subscribeRequest('logical-1', null));
    expect(source.url).toBe('/api/v1/events/wire-abc');
  });

  it('wire streamId 上 wire 前 encodeURIComponent（{id} 插值规则）', async () => {
    stubPostSubscription(200, { streamId: 'id with/space' });
    const { transport } = bundle();
    const { source } = await startStream(transport, subscribeRequest('logical-1', null));
    expect(source.url).toBe('/api/v1/events/id%20with%2Fspace');
  });

  it('POST 订阅失败（503 信封）→ ApiError 且不铸造 EventSource', async () => {
    stubPostSubscription(503, {
      code: 'ServiceUnavailable',
      message: 'registry unavailable',
      requestId: 'req-sse-1',
      retryDisposition: 'safeRead',
    });
    const { transport } = bundle();
    const iterator = transport.subscribe!('subscribeEvents', subscribeRequest('logical-1', null))[Symbol.asyncIterator]() as StreamIterator;
    await expect(iterator.next()).rejects.toMatchObject({
      code: 'ServiceUnavailable',
      requestId: 'req-sse-1',
      retryDisposition: 'safeRead',
    });
    expect(FakeEventSource.created).toHaveLength(0);
  });

  it('POST 响应缺 streamId → ServiceUnavailable 且不铸造 EventSource', async () => {
    stubPostSubscription(200, {});
    const { transport } = bundle();
    const iterator = transport.subscribe!('subscribeEvents', subscribeRequest('logical-1', null))[Symbol.asyncIterator]() as StreamIterator;
    const rejection = await iterator.next().then(
      () => null,
      (error: unknown) => error,
    );
    expect(rejection).toBeInstanceOf(ApiError);
    expect((rejection as ApiError).code).toBe('ServiceUnavailable');
    expect(FakeEventSource.created).toHaveLength(0);
  });
});

/* ------------------------------------------------------------------ */
/* 帧语义：映射 / 回写 / 忽略 / 合成帧。                                 */
/* ------------------------------------------------------------------ */

describe('帧 → EventEnvelope 映射与 streamId 逻辑回写', () => {
  it('id/event/data 帧 → 信封；信封 streamId == 调用方逻辑 id；未登记种与坏帧忽略', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', 41));

    // 心跳注释行（`: keepalive`）在真实 EventSource 不产生事件；未登记事件种
    // 同样不进本层。坏 JSON 帧忽略且不终结。
    source.emit('keepalive', '{}');
    source.emit('executionChanged', '{not json');
    source.emit('executionChanged', JSON.stringify(wireEnvelope('42')));
    source.emit('resultChunk', JSON.stringify({ ...wireEnvelope('43'), payload: { kind: 'resultChunk' } }));

    const received = await take(iterator, 2);
    expect(received).toHaveLength(2);
    expect(received[0]!.streamId).toBe('logical-1');
    expect(received[0]!.sequence).toBe('42');
    expect(received[0]!.runtimeEpoch).toBe('epoch-1');
    expect(received[0]!.executionId).toBe('exec-1');
    expect(received[0]!.payload).toMatchObject({ kind: 'executionChanged' });
    expect(received[1]!.streamId).toBe('logical-1');
    expect(received[1]!.sequence).toBe('43');
    expect(received[1]!.payload).toMatchObject({ kind: 'resultChunk' });
  });

  it('streamResetRequired 合成帧（无 id 行、data 只有 reason）→ sequence 0 合成信封，流继续', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', null));

    source.emit('streamResetRequired', JSON.stringify({ reason: 'gap-detected' }));
    source.emit(
      'sessionChanged',
      JSON.stringify({ ...wireEnvelope('44'), payload: { kind: 'sessionChanged' } }),
    );

    const received = await take(iterator, 2);
    expect(received).toHaveLength(2);
    const reset = received[0]!;
    expect(reset.streamId).toBe('logical-1');
    expect(reset.sequence).toBe(0);
    expect(reset.runtimeEpoch).toBe('');
    expect(reset.executionId).toBeNull();
    expect(reset.jobId).toBeNull();
    expect(reset.sessionHandle).toBeNull();
    expect(reset.contextRevision).toBeNull();
    expect(reset.payload).toEqual({ kind: 'streamResetRequired', reason: 'gap-detected' });
    // 实现语义：合成帧投递后迭代继续（终结裁决在消费方 payload.kind）。
    expect(received[1]!.sequence).toBe('44');
  });

  it('合成帧缺 reason → 缺省文案；无 payload 字段即按合成帧判别', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', null));

    source.emit('streamResetRequired', '{}');

    const received = await take(iterator, 1);
    expect(received[0]!.payload).toEqual({
      kind: 'streamResetRequired',
      reason: 'the event stream must be reset',
    });
  });
});

/* ------------------------------------------------------------------ */
/* 终态与取消（§8.4/§8.5）。                                             */
/* ------------------------------------------------------------------ */

describe('终态与取消（§8.4/§8.5）', () => {
  it.each(['auth_required', 'permission_revoked', 'session_lost'] as const)(
    '%s 终态事件：close + 迭代终结（done 而非 throw），不投递信封',
    async (kind) => {
      stubPostSubscription();
      const { transport } = bundle();
      const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', null));

      source.emit(kind, JSON.stringify({ reason: 'reauth' }));

      const received = await take(iterator, 3);
      expect(received).toEqual([]);
      expect(source.closed).toBe(true);
    },
  );

  it('消费方 return()（for-await break 的等价路径）：本地解除订阅，从不取消 SQL', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', null));

    source.emit('executionChanged', JSON.stringify(wireEnvelope('1')));
    const received = await take(iterator, 1);
    expect(received).toHaveLength(1);

    // 消费方跳出迭代 = iterator.return()（for-await break 自动调用）。
    expect(iterator.return).toBeDefined();
    await iterator.return!();
    expect(source.closed).toBe(true);
    await expect(iterator.next()).resolves.toEqual({ done: true, value: undefined });
    // 摘登记：cancel(wireStreamId) 未命中 = 幂等无操作（不抛、不二次 close）。
    await expect(transport.cancel!('wire-abc', 'after return')).resolves.toBeUndefined();
    expect(source.closeCount).toBe(1);
  });

  it('transport.cancel(wireStreamId) → EventSource.close() + 迭代终结（§8.4 SSE 半边）', async () => {
    stubPostSubscription();
    const { transport } = bundle();
    const { iterator, source } = await startStream(transport, subscribeRequest('logical-1', null));

    await transport.cancel!('wire-abc', 'user navigated away');
    expect(source.closed).toBe(true);
    await expect(iterator.next()).resolves.toEqual({ done: true, value: undefined });
    // 幂等：重复 cancel 同一 id 不再二次 close。
    await transport.cancel!('wire-abc', 'again');
    expect(source.closeCount).toBe(1);
  });
});
