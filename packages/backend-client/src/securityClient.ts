/**
 * The security domain client (server-side secret management) — WR-F1#2.
 *
 * Two methods, one frozen wire contract with the server-side route track
 * (轨 4-e；路径/体/响应逐字冻结，两轨不得各自漂移)：
 *
 * - `security.resolveSecret` — `POST /api/v1/secrets/resolve`,
 *   body `{ domain, key }` → `200 { value }`. The plaintext value answers
 *   **only** to the authenticated principal's own request (L2 cookie
 *   session); a missing key is a §13 `NotFound`.
 * - `security.putSecret` — `PUT /api/v1/secrets/{domain}/{key}`,
 *   body `{ value }` → `204` (empty).
 *
 * ## 红线（red line, registered）
 *
 * Secret plaintext is allowed **only inside requests made by the
 * authenticated principal itself** — it crosses the wire exactly twice per
 * secret (one resolve answer, one put body), never in a list, never in an
 * echo, never in an enumeration surface: this client deliberately exposes no
 * list/get-all face, and the resolve response carries nothing but the single
 * resolved `value`. Server side (WR-D4 `ServerSecretVault`) stores ciphertext
 * only; references (`{domain, key}`) are the only secret coordinate the
 * frontend is allowed to hold.
 *
 * ## Why this client rides an injected HTTP wire instead of `transport.call`
 * (registered)
 *
 * `MethodMap` is type-frozen for this change: the domain-map intersection
 * lives in `transport.ts` and the facade key-consistency assertion lives in
 * `client.ts` — both outside the change's whitelist, and a new key would
 * break the frozen `FacadeCoversMethodMap` proof. The route shapes are
 * therefore registered as the WR-F1 extension ledger in the web side's
 * `httpRoutes.ts` (`WR_F1_HTTP_ROUTES`, keys `security.*`), while the
 * executable truth lives here. `BackendTransport`'s main face is untouched.
 */

import { ApiError } from './errors';
import {
  apiErrorFromWireResponse,
  createHttpWire,
  jsonBodyOfWireResponse,
  type HttpWireOptions,
} from './types/httpWire';

/** `security.resolveSecret` 的请求体（冻结契约逐字：`{ domain, key }`）。 */
export interface SecurityResolveSecretRequest {
  readonly domain: string;
  readonly key: string;
}

/** `security.resolveSecret` 的响应体（冻结契约逐字：`{ value }`，无回显面）。 */
export interface SecurityResolveSecretResponse {
  readonly value: string;
}

/** `security.putSecret` 的请求（路径两段 + 体 `{ value }`）。 */
export interface SecurityPutSecretRequest {
  readonly domain: string;
  readonly key: string;
  readonly value: string;
}

/** The security domain client surface (no list face — the red line above). */
export interface SecurityClient {
  resolveSecret(request: SecurityResolveSecretRequest): Promise<SecurityResolveSecretResponse>;
  putSecret(request: SecurityPutSecretRequest): Promise<void>;
}

function requireNonEmpty(value: string, field: string, methodLabel: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new ApiError(
      'InvalidArgument',
      `${methodLabel} requires a non-empty ${field} string; references are the only secret coordinate the frontend holds.`,
    );
  }
  return value;
}

/**
 * Build the security domain client over the injected wire options
 * (base defaults to the same-origin `/api/v1`).
 */
export function createSecurityClient(wireOptions: HttpWireOptions = {}): SecurityClient {
  const wire = createHttpWire(wireOptions);

  return {
    async resolveSecret(request: SecurityResolveSecretRequest): Promise<SecurityResolveSecretResponse> {
      const methodLabel = 'security.resolveSecret';
      const domain = requireNonEmpty(request.domain, 'domain', methodLabel);
      const key = requireNonEmpty(request.key, 'key', methodLabel);

      const response = await wire({
        method: 'POST',
        path: '/secrets/resolve',
        body: { domain, key },
      });
      if (!response.ok) {
        throw await apiErrorFromWireResponse(response, methodLabel);
      }
      const parsed = (await jsonBodyOfWireResponse(response, methodLabel)) as {
        value?: unknown;
      } | undefined;
      if (parsed === undefined || typeof parsed.value !== 'string') {
        throw new ApiError(
          'ServiceUnavailable',
          `${methodLabel} returned an unreadable response (expected { value: string }).`,
        );
      }
      return { value: parsed.value };
    },

    async putSecret(request: SecurityPutSecretRequest): Promise<void> {
      const methodLabel = 'security.putSecret';
      const domain = requireNonEmpty(request.domain, 'domain', methodLabel);
      const key = requireNonEmpty(request.key, 'key', methodLabel);
      if (typeof request.value !== 'string') {
        throw new ApiError(
          'InvalidArgument',
          `${methodLabel} requires a string value (the plaintext travels only in the authenticated principal's own request body).`,
        );
      }

      // 路径两段逐段编码；RouteSpec 的单 {id} 插值表达不了两段，
      // WR_F1_HTTP_ROUTES 的登记行用 {id} = 编码后的 domain + '/' + key 对。
      const path =
        `/secrets/${encodeURIComponent(domain)}/${encodeURIComponent(key)}`;
      const response = await wire({ method: 'PUT', path, body: { value: request.value } });
      if (!response.ok) {
        throw await apiErrorFromWireResponse(response, methodLabel);
      }
      // 204 语义：成功即无体无值。
    },
  };
}

/** Compile-time: the client serves exactly its two contract methods. */
type Assert<T extends true> = T;
export type SecurityClientCoversContract = Assert<
  'resolveSecret' | 'putSecret' extends keyof SecurityClient ? true : false
>;
