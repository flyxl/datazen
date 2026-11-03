/**
 * The minimal HTTP wire the WR-F1 wire-direct domain clients speak
 * (mcp / security, and the web aiHost calls assembled on the same shape).
 *
 * Why this exists (registered, WR-F1 三缺口合拢):
 *
 * 1. `MethodMap` is **frozen** for this change: the intersection lives in
 *    `transport.ts` and the compile-time key-consistency assertion
 *    (`FacadeCoversMethodMap`) lives in `client.ts` — both files are outside
 *    the change's whitelist, and adding keys to `MethodMap` would fail the
 *    frozen assertion (every non-gateway key must be a facade method).
 *    The wire-direct domains therefore ride an injected HTTP wire instead of
 *    `transport.call`, leaving the `BackendTransport` main face untouched.
 * 2. Architecture guard F-07 scans this whole package and fails on the
 *    literal `fetch(` / `XMLHttpRequest` — the transport-agnostic promise.
 *    This file contains **no network primitive**: the fetch implementation is
 *    always injected (`HttpWireOptions.fetchImpl`), and a use without one is
 *    a loud wiring error (§7.4 semantics), never a silent fallback.
 * 3. Error normalization mirrors the web transport's discipline: network-level
 *    failures become `ServiceUnavailable` (`deserializeApiError`), and the
 *    §13 four-field rejection body `{code, message, requestId,
 *    retryDisposition}` becomes an `ApiError` with its fields preserved.
 *
 * The wire deliberately carries no in-flight registry and no cancel path: the
 * wire-direct domains are short, user-initiated calls; the transport-level
 * cancel surface (§8.4/§8.5) stays with `BackendTransport`.
 */

import { ApiError, deserializeApiError } from '../errors';

/** HTTP methods the wire-direct domain faces use (§6 v1 面，无 HEAD/OPTIONS)。 */
export type HttpWireMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

/** One outbound request on the wire. `body === undefined` means no body. */
export interface HttpWireRequest {
  readonly method: HttpWireMethod;
  /** Path relative to the wire base, with a leading `/` (e.g. `/secrets/resolve`, `/mcp`). */
  readonly path: string;
  readonly body?: unknown;
  /** Extra headers merged over the wire defaults (e.g. the MCP session header). */
  readonly headers?: Readonly<Record<string, string>>;
}

/**
 * The injected HTTP mechanism: one request in, the raw `Response` out.
 * Response-shape interpretation (JSON-RPC error vs §13 body vs session
 * headers) stays with each domain client — the wire is transport only.
 */
export type HttpWire = (request: HttpWireRequest) => Promise<Response>;

/** Wire construction options. Everything is injectable (tests / embedding). */
export interface HttpWireOptions {
  /**
   * Base URL the request paths resolve against. Defaults to the same-origin
   * `/api/v1` (the §6 domain surface); the empty string `''` selects the
   * server root (the `/mcp` face is a sibling of `/api/v1`, not under it).
   */
  readonly baseUrl?: string;
  /**
   * The outbound fetch implementation. **Required in practice** — this
   * package may not inline a network primitive (F-07), so there is no
   * default here; a use without an injected implementation throws a
   * locatable wiring error instead of degrading (§7.4).
   */
  readonly fetchImpl?: typeof fetch;
  /**
   * CSRF token source for non-GET requests. Defaults to reading the public
   * cookie half (`__Host-dz_csrf`, team-server §4.4) when a `document` is
   * present; node test environments inject a provider instead.
   */
  readonly csrfTokenProvider?: () => string | null;
}

/** CSRF 令牌的公开半边 cookie（team-server §4.4；server `CSRF_COOKIE` 同名）。 */
const CSRF_COOKIE_NAME = '__Host-dz_csrf';

/** CSRF 请求头（server `server/src/csrf.rs` 的 `CSRF_HEADER`；双提交 header 半边）。 */
const CSRF_HEADER_NAME = 'x-dz-csrf-token';

/** §6 域面的同源缺省基路径（web transport 的 `DEFAULT_BASE_URL` 同值）。 */
const DEFAULT_API_BASE = '/api/v1';

/** §4.4：`__Host-dz_csrf` 可被前端读取——document.cookie 解析出令牌。 */
function readCsrfCookie(): string | null {
  if (typeof document === 'undefined') return null;
  for (const cookie of document.cookie.split(';')) {
    const separator = cookie.indexOf('=');
    if (separator < 0) continue;
    if (cookie.slice(0, separator).trim() === CSRF_COOKIE_NAME) {
      return cookie.slice(separator + 1).trim();
    }
  }
  return null;
}

function normalizeBase(baseUrl: string | undefined): string {
  if (baseUrl === undefined) return DEFAULT_API_BASE;
  return baseUrl.replace(/\/+$/, '');
}

/**
 * Build one wire over the injected options. The returned function is the
 * only place this package touches the network mechanism — and even there it
 * only *calls through* the injected implementation (no primitive literal of
 * its own, keeping the F-07 scan green).
 */
export function createHttpWire(options: HttpWireOptions = {}): HttpWire {
  const base = normalizeBase(options.baseUrl);
  const fetchImpl = options.fetchImpl;
  const csrfToken = options.csrfTokenProvider ?? readCsrfCookie;

  return async (request: HttpWireRequest): Promise<Response> => {
    if (fetchImpl === undefined) {
      // §7.4：缺注入是装配错误，如实指名，绝不静默降级。
      throw new ApiError(
        'ServiceUnavailable',
        'The HTTP wire has no fetch implementation injected; construct it via HttpWireOptions.fetchImpl (the transport-agnostic package may not inline one).',
      );
    }
    const headers = new Headers(request.headers);
    if (request.body !== undefined) {
      headers.set('Content-Type', 'application/json');
    }
    if (request.method !== 'GET') {
      // §5.2：cookie 认证的非安全方法一律随请求带上公开半边令牌（web
      // transport 同款统一规则；/mcp 面当前在服务端 L4 的 under_api 豁免
      // 区，带上是无害且面向未来的）。
      const token = csrfToken();
      if (token !== null && token.length > 0) headers.set(CSRF_HEADER_NAME, token);
    }
    try {
      return await fetchImpl(`${base}${request.path}`, {
        method: request.method,
        credentials: 'include',
        headers,
        ...(request.body === undefined ? {} : { body: JSON.stringify(request.body) }),
      });
    } catch (error) {
      // 网络层失败 → 归一为 ServiceUnavailable（原样留 cause）。
      throw deserializeApiError(error);
    }
  };
}

/**
 * 非 2xx 的统一拒绝侧：§13 四字段体 → `ApiError`（字段保真）；体不可读 →
 * 带状态码的 `ServiceUnavailable`。与 web transport 的 `rejectionOf` 同语义，
 * 服务于 wire-direct 域（§6.3 wire 形状）。
 */
export async function apiErrorFromWireResponse(
  response: Response,
  methodLabel: string,
): Promise<ApiError> {
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

/**
 * 2xx 的统一成功侧：JSON 体解析；空体归 `undefined`（204 语义）。
 * 调用方先自行确认 `response.ok`。
 */
export async function jsonBodyOfWireResponse(
  response: Response,
  methodLabel: string,
): Promise<unknown> {
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
