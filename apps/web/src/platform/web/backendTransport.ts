/**
 * Web 后端传输（WR-E2）——HTTP/SSE `WebClient` 实现，替换 WR-E1 的
 * all-throw stub。上方消费面（assembly → facade → 域客户端）零改动。
 *
 * **覆盖账目**（对照 `docs/architecture/platform/http-api-contract.md` §4
 * 总账与 §6.1–§6.9 逐域表；分发表本体在 `./httpRoutes`）：
 *
 * - call 面 **41/42**：42 个域客户端 call 方法中 41 个上 HTTP（分发表 41 条，
 *   PATCH 一条路由双服务 update/disable）——`open_base64_with_dialog` 排除
 *   （§6.9/§8）；分发表本体在 `./httpRoutes`；
 * - events 域 **2 条路由走 `subscribe()` 两步流水线**（POST
 *   `/events/subscriptions` 铸造一次性 streamId → `EventSource`
 *   `GET /events/{streamId}`，契约 §6.6 v1.1），服务 `subscribeEvents`；
 * - legacy 轨 ~200 宿主命令不镜像（§8），与 `open_base64_with_dialog` 一并以
 *   `capability` 进 `unsupportedMethods`（`./webUnsupportedMethods`）；
 * - 合计对照 §4 总账的「HTTP 服务 42」：**42/42**（41 call + subscribeEvents；
 *   `createEventSubscription` 是流水线的登记步，无 `MethodMap` 键）。
 *
 * HTTP 约定（契约 §3 + team-server §3–§9）：
 *
 * - base URL 默认同源相对路径 `/api/v1`（创建选项可注入覆盖，测试用）；
 *   `fetch` 一律 `credentials: 'include'`（`__Host-dz_session` cookie，§4.4）；
 * - **CSRF**（§5）：会话绑定签名双提交令牌——非 GET 请求带 `x-dz-csrf-token`
 *   头（server 侧 `CSRF_HEADER` 的落点；令牌公开半在非 HttpOnly 的
 *   `__Host-dz_csrf` cookie，§4.4「前端需读取以放入请求头」），默认从该
 *   cookie 读取、可注入；POST 读（tableData 域）不携带幂等令牌但照过
 *   CSRF 头校验（契约 §3 规则 2），本层统一带头即覆盖；
 * - **成功侧**：2xx 解析 JSON；空体（§6.8 `commitQueryResultCell` 的 200
 *   空体）归 `undefined`；`readArtifact` 双模式解码在 `./httpRoutes`；
 * - **错误侧**：非 2xx 的四字段体 `{code, message, requestId,
 *   retryDisposition}`（team-server §9.2 / 概要 §6.3 wire 形状）经
 *   `deserializeApiError` 归一为 `ApiError`；网络层失败按同函数归一为
 *   `ServiceUnavailable`；
 * - **SSE**：`./eventStreamSource`；**cancel**：`EventSource` 走登记表
 *   close（§8.4 的先 close 义务），在途 fetch 走 AbortController 登记表；
 *   取消从不取消 SQL（§8.5）。
 *
 * 本文件与 helper 是 web 形态内核的落点；再往上（facade、域客户端）不感知
 * 传输机制——`packages/frontend/src/platform/web/stub.ts` 的编译期同形
 * 证明保持原样。
 */

import {
  ApiError,
  deserializeApiError,
  type BackendTransport,
  type ConnectionEvent,
  type EventEnvelope,
  type MethodMap,
  type SubscribeEventsRequest,
} from '@datazen/backend-client';

import { HTTP_ROUTES, type RouteSpec, type WebHttpMethod } from './httpRoutes';
import { createEventStreamIterable, type EventSourceFactory } from './eventStreamSource';
import { buildWebUnsupportedMethods } from './webUnsupportedMethods';

/** `backendId` 标签：装配在 facade 上，永不作为业务参数（桌面同款约定）。 */
export const WEB_BACKEND_ID = 'web';

/** 默认同源相对基路径（契约路由全部在 /api/v1 下）。 */
const DEFAULT_BASE_URL = '/api/v1';

/** CSRF 令牌的公开半边 cookie（team-server §4.4；server `CSRF_COOKIE` 同名）。 */
const CSRF_COOKIE_NAME = '__Host-dz_csrf';

/** CSRF 请求头（server `server/src/csrf.rs` 的 `CSRF_HEADER`；双提交 header 半边）。 */
const CSRF_HEADER_NAME = 'x-dz-csrf-token';

/** 创建选项；全部可注入（测试用），缺省即生产形态。 */
export interface WebTransportOptions {
  /** 覆盖同源基路径（如 `http://127.0.0.1:8080/api/v1`）；缺省 `/api/v1`。 */
  baseUrl?: string;
  /** fetch 实现；缺省 `globalThis.fetch`。 */
  fetchImpl?: typeof fetch;
  /** CSRF 令牌来源；缺省读 `__Host-dz_csrf` cookie（§4.4 非 HttpOnly 半边）。 */
  csrfTokenProvider?: () => string | null;
  /** EventSource 工厂；缺省 `new EventSource(url)`（原生 Last-Event-ID 自理）。 */
  eventSourceFactory?: EventSourceFactory;
}

/** SSE 流取消登记项：close 即 `EventSource.close()`。 */
interface StreamRegistration {
  close(): void;
}

function normalizeBaseUrl(value: string): string {
  const trimmed = value.replace(/\/+$/, '');
  return trimmed.length > 0 ? trimmed : DEFAULT_BASE_URL;
}

/** §4.4：`__Host-dz_csrf` 可被前端读取——document.cookie 解析出令牌。 */
function readCsrfCookie(): string | null {
  for (const cookie of document.cookie.split(';')) {
    const separator = cookie.indexOf('=');
    if (separator < 0) continue;
    if (cookie.slice(0, separator).trim() === CSRF_COOKIE_NAME) {
      return cookie.slice(separator + 1).trim();
    }
  }
  return null;
}

/**
 * 构建 web 后端传输。
 *
 * 泛型窄化的单点在 `call()`：分发表以 `keyof MethodMap` 键存取 `RouteSpec`，
 * 构造器在 `unknown` 语境消费 payload；表外键 = 排除面（§8）→
 * `CapabilityUnsupported`（kind `capability`，与 `unsupportedMethods` 声明
 * 同口径，guard 层先行拦截，此处兜底）。
 */
export function createWebBackendTransport(options: WebTransportOptions = {}): BackendTransport {
  const baseUrl = normalizeBaseUrl(options.baseUrl ?? DEFAULT_BASE_URL);
  const fetchImpl = options.fetchImpl ?? ((input: RequestInfo | URL, init?: RequestInit) => fetch(input, init));
  const csrfToken = options.csrfTokenProvider ?? readCsrfCookie;
  const eventSourceFactory: EventSourceFactory =
    options.eventSourceFactory ?? ((url: string) => new EventSource(url));

  /** SSE 流登记表：wire streamId → close（`cancel(opaque)` 的 SSE 半边）。 */
  const streams = new Map<string, StreamRegistration>();
  /** 在途 fetch 的 AbortController 登记表：内部自增 id → controller。 */
  const inFlight = new Map<string, AbortController>();
  let nextCallSeq = 0;

  /** 单个出站请求：CSRF 带头 + `credentials: 'include'` + 登记表挂摘。 */
  async function send(
    httpMethod: WebHttpMethod,
    url: string,
    body: unknown,
    methodLabel: string,
  ): Promise<Response> {
    const headers = new Headers();
    if (httpMethod !== 'GET') {
      // §5.2：cookie 认证的非安全方法一律带头（GET/HEAD 免；SSE 握手的
      // GET 由 EventSource 发出、天然不带头，用 Origin 证明替代，§8.1）。
      const token = csrfToken();
      if (token !== null && token.length > 0) headers.set(CSRF_HEADER_NAME, token);
    }
    if (body !== undefined) {
      headers.set('Content-Type', 'application/json');
    }
    const init: RequestInit = {
      method: httpMethod,
      credentials: 'include',
      headers,
    };

    // 登记表先挂后发：cancel 才可能命中在途请求。
    const callId = `call-${nextCallSeq++}`;
    const controller = new AbortController();
    inFlight.set(callId, controller);
    init.signal = controller.signal;
    try {
      if (body !== undefined) {
        init.body = JSON.stringify(body);
      }
      return await fetchImpl(url, init);
    } catch (error) {
      if (controller.signal.aborted) {
        // 本地取消：请求没有到达「被拒绝/被接受」的任何一侧。
        throw new ApiError('ServiceUnavailable', `${methodLabel} was cancelled locally.`, {
          cause: error,
        });
      }
      // 网络层失败 → `ServiceUnavailable`（errors.ts 的归一口径，原样留 cause）。
      throw deserializeApiError(error);
    } finally {
      inFlight.delete(callId);
    }
  }

  /** 非 2xx → wire 四字段体 → `ApiError`；体不可读 → 带状态码的 `ServiceUnavailable`。 */
  async function rejectionOf(response: Response, methodLabel: string): Promise<ApiError> {
    const text = await response.text();
    let parsed: unknown;
    try {
      parsed = text.length > 0 ? (JSON.parse(text) as unknown) : undefined;
    } catch {
      parsed = undefined;
    }
    if (typeof parsed === 'object' && parsed !== null) {
      return deserializeApiError(parsed);
    }
    return new ApiError(
      'ServiceUnavailable',
      `${methodLabel} failed: HTTP ${response.status} ${response.statusText}`.trim(),
      { cause: text.slice(0, 200) },
    );
  }

  /** 2xx 体 → JSON；空体 → undefined（§6.8 的 200 空体语义）。 */
  async function successBodyOf(response: Response, methodLabel: string): Promise<unknown> {
    const text = await response.text();
    if (text.length === 0) return undefined;
    try {
      return JSON.parse(text) as unknown;
    } catch (error) {
      throw new ApiError('ServiceUnavailable', `${methodLabel} returned unreadable JSON.`, {
        cause: error,
      });
    }
  }

  return {
    unsupportedMethods: buildWebUnsupportedMethods(),

    async call<K extends keyof MethodMap>(
      method: K,
      payload: MethodMap[K]['request'],
    ): Promise<MethodMap[K]['response']> {
      const route: RouteSpec | undefined = HTTP_ROUTES[method];
      if (route === undefined) {
        // 分发表之外的 keyof MethodMap 键 = legacy 宿主命令 + 排除面（§8）。
        throw new ApiError(
          'CapabilityUnsupported',
          `${String(method)} has no route on the web HTTP surface (contract §4 ledger / §8 exclusions).`,
          { unsupportedKind: 'capability' },
        );
      }

      const pathId = route.pathId?.(payload);
      const path = pathId === undefined ? route.pathTemplate : route.pathTemplate.replace('{id}', pathId);
      const query = route.query?.(payload) ?? '';
      const body = route.body === undefined ? payload : route.body(payload);
      const response = await send(route.httpMethod, `${baseUrl}${path}${query}`, body, String(method));
      if (!response.ok) {
        throw await rejectionOf(response, String(method));
      }
      if (route.decode !== undefined) {
        const raw = new Uint8Array(await response.arrayBuffer());
        return route.decode(payload, response, raw) as MethodMap[K]['response'];
      }
      const parsed = await successBodyOf(response, String(method));
      return parsed as MethodMap[K]['response'];
    },

    subscribe(
      _method: 'subscribeEvents',
      payload: SubscribeEventsRequest,
    ): AsyncIterable<EventEnvelope<ConnectionEvent>> {
      return createEventStreamIterable(
        {
          baseUrl,
          postSubscription: async (): Promise<string> => {
            const response = await send('POST', `${baseUrl}/events/subscriptions`, undefined, 'subscribeEvents');
            if (!response.ok) throw await rejectionOf(response, 'subscribeEvents');
            const parsed = await successBodyOf(response, 'subscribeEvents');
            const streamId =
              typeof parsed === 'object' && parsed !== null
                ? (parsed as Record<string, unknown>)['streamId']
                : undefined;
            if (typeof streamId !== 'string' || streamId.length === 0) {
              throw new ApiError(
                'ServiceUnavailable',
                'subscribeEvents: the subscription response carries no streamId.',
              );
            }
            return streamId;
          },
          eventSourceFactory,
          registerStream: (streamId, close) => {
            streams.set(streamId, { close });
          },
          unregisterStream: (streamId) => {
            streams.delete(streamId);
          },
        },
        {
          // payload.streamId 是调用方逻辑流 id（如 ExecutionReceipt.streamId）：
          // wire 上由注册表铸造一次性 id，信封回写逻辑 id（见 eventStreamSource）。
          logicalStreamId: payload.streamId,
          afterSequence: payload.afterSequence,
        },
      );
    },

    async cancel(opaque: string, _reason: string): Promise<void> {
      // SSE 半边：登记表命中 → EventSource.close()（§8.4：先 close 再处理
      // 登录/重建，防自动重连风暴）。
      const stream = streams.get(opaque);
      if (stream !== undefined) {
        streams.delete(opaque);
        stream.close();
        return;
      }
      // fetch 半边：在途请求命中 → abort（无端口调用方约定 opaque 的现状下，
      // 登记键为内部自增 id；对外语义见报告的顺延项）。
      const controller = inFlight.get(opaque);
      if (controller !== undefined) {
        inFlight.delete(opaque);
        controller.abort();
      }
      // 未命中 = 操作已结束：取消是幂等无操作，不是错误。
    },
  };
}
