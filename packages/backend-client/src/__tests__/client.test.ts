/**
 * 门面测试：载荷形状、错误归一、未绑定纪律、流桥接。
 *
 * 三条与规格直接对应的断言：
 * - **载荷里没有 backendId**：21 个方法的载荷都不许携带后端标识，backendId 只用来选门面；
 * - **未绑定即抛 §7.4 的固定文案**，绝不静默回退到 Tauri；
 * - **结束迭代不触发取消**：`subscribeEvents` 的流被 `break` 时，只 `return()` 源迭代器，
 *   绝不发 `cancelExecution`（取消是业务动作，必须显式发起）。
 */

import { afterEach, describe, expect, it } from 'vitest';

import {
  UNBOUND_MESSAGE,
  apiErrorCode,
  boundBackendIds,
  clearBackendClients,
  createBackendClient,
  currentBackendId,
  getBackendClient,
  requireBackendClient,
  selectBackend,
  setBackendClient,
} from '@datazen/backend-client';
import type {
  BackendError,
  BackendTransport,
  ConnectionEvent,
  EventEnvelope,
  MethodMap,
  MethodName,
  SessionHandle,
  SessionView,
} from '@datazen/backend-client';

interface RecordedCall {
  readonly method: MethodName;
  readonly payload: unknown;
}

type Responder = (method: MethodName, payload: unknown) => unknown;

function recordingTransport(responder: Responder = () => null): {
  readonly transport: BackendTransport;
  readonly calls: RecordedCall[];
} {
  const calls: RecordedCall[] = [];
  const transport: BackendTransport = {
    async call<K extends keyof MethodMap>(
      method: K,
      payload: MethodMap[K]['request'],
    ): Promise<MethodMap[K]['response']> {
      calls.push({ method, payload });
      return (await responder(method, payload)) as MethodMap[K]['response'];
    },
  };
  return { transport, calls };
}

const HANDLE: SessionHandle = { dbSessionId: 'db-1', runtimeEpoch: 'ep-7' };

const SESSION: SessionView = {
  handle: HANDLE,
  connectionId: 'conn-1',
  configRevision: '3',
  owner: { kind: 'clientSession', clientInstanceId: 'cli-1', purpose: 'editor' },
  initialTarget: {
    connectionId: 'conn-1',
    namespace: { database: 'app', catalog: null, schema: null, path: [] },
    object: null,
  },
  observedContext: {
    namespace: { database: 'app', catalog: null, schema: null, path: [] },
    searchPath: null,
    effectiveIdentity: 'app_user',
    transactionState: 'none',
    autocommit: true,
    confidence: 'confirmed',
  },
  contextRevision: '1',
  state: 'ready',
  attachmentState: 'attached',
  activeExecutionId: null,
  expiresAt: null,
};

function envelope(sequence: number, payload: ConnectionEvent): EventEnvelope<ConnectionEvent> {
  return {
    streamId: 'stream-1',
    sequence: String(sequence),
    runtimeEpoch: 'ep-7',
    executionId: null,
    jobId: null,
    sessionHandle: HANDLE,
    contextRevision: '1',
    payload,
  };
}

afterEach(() => {
  clearBackendClients();
});

describe('门面载荷', () => {
  it('无参方法把 null 载荷交给传输层', async () => {
    const { transport, calls } = recordingTransport(() => []);
    const client = createBackendClient('local', transport);
    await client.listConnections();
    expect(calls).toEqual([{ method: 'listConnections', payload: null }]);
  });

  it('单参方法收敛成带命名字段的对象载荷', async () => {
    const { transport, calls } = recordingTransport(() => null);
    const client = createBackendClient('local', transport);
    await client.getSession(HANDLE);
    await client.getExecution('exe-1');
    await client.cancelExecution('exe-1');
    await client.getJob('job-1');
    expect(calls).toEqual([
      { method: 'getSession', payload: { handle: HANDLE } },
      { method: 'getExecution', payload: { executionId: 'exe-1' } },
      { method: 'cancelExecution', payload: { executionId: 'exe-1' } },
      { method: 'getJob', payload: { jobId: 'job-1' } },
    ]);
  });

  it('listJobs 默认过滤器为 null，显式过滤器原样透传', async () => {
    const { transport, calls } = recordingTransport(() => []);
    const client = createBackendClient('local', transport);
    await client.listJobs();
    await client.listJobs({ states: ['failed'], owner: null, after: null, limit: 50 });
    expect(calls[0]?.payload).toEqual({ filter: null });
    expect(calls[1]?.payload).toEqual({
      filter: { states: ['failed'], owner: null, after: null, limit: 50 },
    });
  });

  it('issueSubmissionToken 把可空会话句柄显式传下去', async () => {
    const { transport, calls } = recordingTransport(() => null);
    const client = createBackendClient('local', transport);
    await client.issueSubmissionToken('createProfile');
    await client.issueSubmissionToken('openSession', HANDLE);
    expect(calls.map((call) => call.payload)).toEqual([
      { operation: 'createProfile', sessionHandle: null },
      { operation: 'openSession', sessionHandle: HANDLE },
    ]);
  });

  it('任何载荷都不携带 backendId', async () => {
    const { transport, calls } = recordingTransport(() => null);
    const client = createBackendClient('team-remote', transport);
    await client.listConnections();
    await client.getSession(HANDLE);
    await client.issueSubmissionToken('startJob');
    await client.cancelJob('job-1', 'idem-1');
    expect(calls.length).toBeGreaterThan(0);
    for (const call of calls) {
      // `listConnections` 的载荷是 null（无参方法），其余是对象；两者都不许带后端标识。
      const keys = call.payload === null ? [] : Object.keys(call.payload as object);
      expect(JSON.stringify(call.payload)).not.toContain('team-remote');
      expect(keys).not.toContain('backendId');
    }
  });
});

describe('错误归一', () => {
  it('业务拒绝转成带 apiError 的 BackendError，code 与 requestId 保持可读', async () => {
    const payload = {
      code: 'contextConflict',
      message: 'session context moved on',
      requestId: 'req-9',
      retryDisposition: 'never',
    };
    const { transport } = recordingTransport((method) => {
      if (method === 'getSession') throw payload;
      return null;
    });
    const client = createBackendClient('local', transport);

    const failure = await client.getSession(HANDLE).catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(Error);
    const error = failure as BackendError;
    expect(error.kind).toBe('transport');
    expect(apiErrorCode(error)).toBe('contextConflict');
    expect(error.apiError?.requestId).toBe('req-9');
    expect(error.message).toContain('contextConflict');
  });

  it('传输崩溃归为 transport 且 apiError 为 null', async () => {
    const { transport } = recordingTransport(() => {
      throw new Error('socket hang up');
    });
    const client = createBackendClient('local', transport);
    const failure = await client.listConnections().catch((error: unknown) => error);
    expect((failure as BackendError).kind).toBe('transport');
    expect((failure as BackendError).apiError).toBeNull();
    expect((failure as Error).message).toContain('socket hang up');
  });

  it('已是 BackendError 的载荷不被二次包装', async () => {
    const original = Object.assign(new Error('already normalized'), {
      kind: 'transport',
      apiError: null,
    }) as BackendError;
    const { transport } = recordingTransport(() => {
      throw original;
    });
    const client = createBackendClient('local', transport);
    await expect(client.listConnections()).rejects.toBe(original);
  });
});

describe('未绑定纪律', () => {
  it('未注入时 requireBackendClient 抛 §7.4 的固定文案', () => {
    expect(() => requireBackendClient()).toThrow(UNBOUND_MESSAGE);
    expect(() => requireBackendClient('web')).toThrow(UNBOUND_MESSAGE);
    expect(() => requireBackendClient('web')).toThrow('backendId: "web"');
  });

  it('getBackendClient 只探测不抛错', () => {
    expect(getBackendClient()).toBeNull();
    expect(getBackendClient('web')).toBeNull();
  });

  it('setBackendClient 同时设为当前门面，selectBackend 只切引用', () => {
    const { transport } = recordingTransport();
    const local = createBackendClient('local', transport);
    const web = createBackendClient('web', transport);

    setBackendClient('local', local);
    setBackendClient('web', web);
    expect(currentBackendId()).toBe('web');
    expect(boundBackendIds()).toEqual(['local', 'web']);

    expect(selectBackend('local')).toBe(local);
    expect(currentBackendId()).toBe('local');
    expect(requireBackendClient()).toBe(local);
    expect(requireBackendClient('web')).toBe(web);
    expect(() => selectBackend('missing')).toThrow(UNBOUND_MESSAGE);
  });
});

describe('subscribeEvents 桥接', () => {
  it('传输未实现 subscribe 时抛 BackendError，而不是静默降级', () => {
    const { transport } = recordingTransport();
    const client = createBackendClient('local', transport);
    const failure = (() => {
      try {
        return client.subscribeEvents({ streamId: 's', afterSequence: null });
      } catch (error) {
        return error;
      }
    })();
    expect(failure).toBeInstanceOf(Error);
    expect((failure as BackendError).kind).toBe('transport');
    expect((failure as BackendError).apiError).toBeNull();
  });

  it('流按序交付事件；break 只结束订阅，不触发 cancelExecution', async () => {
    let released = false;
    const source: AsyncIterable<EventEnvelope<ConnectionEvent>> = {
      [Symbol.asyncIterator]() {
        let sequence = 0;
        return {
          async next() {
            await Promise.resolve();
            if (sequence >= 3) return { value: undefined, done: true };
            sequence += 1;
            return {
              value: envelope(sequence, { kind: 'streamResetRequired', reason: 'server restart' }),
              done: false,
            };
          },
          async return() {
            released = true;
            return { value: undefined, done: true };
          },
        };
      },
    };

    const calls: RecordedCall[] = [];
    const transport: BackendTransport = {
      async call<K extends keyof MethodMap>(
        method: K,
        payload: MethodMap[K]['request'],
      ): Promise<MethodMap[K]['response']> {
        calls.push({ method, payload });
        // 只断言「发了哪些方法、载荷长什么样」，响应值不被读取。
        return null as unknown as MethodMap[K]['response'];
      },
      subscribe: () => source,
    };
    const client = createBackendClient('local', transport);

    const received: string[] = [];
    for await (const event of client.subscribeEvents({
      streamId: 'stream-1',
      afterSequence: null,
    })) {
      received.push(event.sequence);
      if (received.length === 2) break;
    }

    expect(received).toEqual(['1', '2']);
    await Promise.resolve();
    await Promise.resolve();
    expect(released).toBe(true);
    expect(calls).toEqual([]);
  });

  it('源流报错时消费侧收到带 apiError 的 BackendError', async () => {
    const source: AsyncIterable<EventEnvelope<ConnectionEvent>> = {
      [Symbol.asyncIterator]() {
        return {
          async next(): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> {
            throw {
              code: 'sessionLost',
              message: 'runtime restarted',
              requestId: 'req-3',
              retryDisposition: 'never',
            };
          },
          async return() {
            return { value: undefined, done: true };
          },
        };
      },
    };
    const transport: BackendTransport = {
      async call<K extends keyof MethodMap>(
        _method: K,
        _payload: MethodMap[K]['request'],
      ): Promise<MethodMap[K]['response']> {
        // 只断言「发了哪些方法、载荷长什么样」，响应值不被读取。
        return null as unknown as MethodMap[K]['response'];
      },
      subscribe: () => source,
    };
    const client = createBackendClient('local', transport);

    const failure = await client
      .subscribeEvents({ streamId: 'stream-1', afterSequence: null })
      .next()
      .catch((error: unknown) => error);
    expect(apiErrorCode(failure)).toBe('sessionLost');
  });

  it('源流正常结束则消费侧正常 done', async () => {
    const source: AsyncIterable<EventEnvelope<ConnectionEvent>> = {
      [Symbol.asyncIterator]() {
        let delivered = false;
        return {
          async next() {
            if (delivered) return { value: undefined, done: true };
            delivered = true;
            return {
              value: envelope(1, { kind: 'sessionChanged', session: SESSION }),
              done: false,
            };
          },
          async return() {
            return { value: undefined, done: true };
          },
        };
      },
    };
    const transport: BackendTransport = {
      async call<K extends keyof MethodMap>(
        _method: K,
        _payload: MethodMap[K]['request'],
      ): Promise<MethodMap[K]['response']> {
        // 只断言「发了哪些方法、载荷长什么样」，响应值不被读取。
        return null as unknown as MethodMap[K]['response'];
      },
      subscribe: () => source,
    };
    const client = createBackendClient('local', transport);

    const seen: number[] = [];
    for await (const event of client.subscribeEvents({
      streamId: 'stream-1',
      afterSequence: null,
    })) {
      seen.push(Number(event.sequence));
    }
    expect(seen).toEqual([1]);
  });
});
