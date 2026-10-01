/**
 * 错误面测试：`ApiError` 反序列化的**严格性**，以及重试分类。
 *
 * 关键不变量：**未知 code 不得被静默折叠成「合法错误」**。传输层返回了一个新版本的
 * code 时，`parseApiError` 必须返回 `null`，让它落到 `BackendError`，原始载荷留在
 * `cause` 里。否则前端会对一个它不认识的状态做出「已知」的重试判断。
 */

import { describe, expect, it } from 'vitest';

import {
  API_ERROR_CODES,
  UNBOUND_MESSAGE,
  apiErrorCode,
  apiErrorRequestId,
  backendError,
  isApiErrorCode,
  isRetryDisposition,
  mayRetryWithoutCheckingExecution,
  parseApiError,
  requiresOutcomeVerification,
} from '@datazen/backend-client';

function apiErrorPayload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    code: 'sessionNotFound',
    message: 'session is no longer available',
    requestId: 'req-1',
    retryDisposition: 'never',
    ...overrides,
  };
}

describe('ApiError 解析', () => {
  it('接受完整载荷并保持 requestId', () => {
    const parsed = parseApiError(apiErrorPayload());
    expect(parsed).not.toBeNull();
    expect(parsed?.code).toBe('sessionNotFound');
    expect(parsed?.requestId).toBe('req-1');
    expect(parsed?.retryDisposition).toBe('never');
  });

  it('缺 requestId 视为 null，而不是解析失败', () => {
    const parsed = parseApiError(apiErrorPayload({ requestId: undefined }));
    expect(parsed?.requestId).toBeNull();
  });

  it('未知 code 返回 null（不得伪造合法 ApiError）', () => {
    expect(parseApiError(apiErrorPayload({ code: 'someFutureCode' }))).toBeNull();
    expect(parseApiError(apiErrorPayload({ code: '' }))).toBeNull();
    expect(parseApiError(apiErrorPayload({ code: 42 }))).toBeNull();
  });

  it('非法 retryDisposition / 缺失 message 一律拒绝', () => {
    expect(parseApiError(apiErrorPayload({ retryDisposition: 'retryForever' }))).toBeNull();
    expect(parseApiError(apiErrorPayload({ message: 7 }))).toBeNull();
    expect(parseApiError(apiErrorPayload({ requestId: 7 }))).toBeNull();
  });

  it('非对象载荷返回 null', () => {
    for (const value of [null, undefined, 1, 'oops', true]) {
      expect(parseApiError(value)).toBeNull();
    }
  });

  it('枚举守卫接受 31 个 code，拒绝其余字符串', () => {
    expect(API_ERROR_CODES.every((code) => isApiErrorCode(code))).toBe(true);
    expect(isApiErrorCode('nope')).toBe(false);
    expect(isApiErrorCode(undefined)).toBe(false);
    expect(isRetryDisposition('safeRead')).toBe(true);
    expect(isRetryDisposition('sometimes')).toBe(false);
  });
});

describe('重试分类', () => {
  it('只有 safeRead 才允许直接重发同一幂等键', () => {
    expect(
      mayRetryWithoutCheckingExecution(
        backendError(
          'transport',
          'x',
          parseApiError(apiErrorPayload({ retryDisposition: 'safeRead' })),
        ),
      ),
    ).toBe(true);
    expect(
      mayRetryWithoutCheckingExecution(
        backendError(
          'transport',
          'x',
          parseApiError(apiErrorPayload({ retryDisposition: 'checkExecution' })),
        ),
      ),
    ).toBe(false);
    expect(
      mayRetryWithoutCheckingExecution(
        backendError(
          'transport',
          'x',
          parseApiError(apiErrorPayload({ retryDisposition: 'never' })),
        ),
      ),
    ).toBe(false);
    expect(mayRetryWithoutCheckingExecution(new Error('plain'))).toBe(false);
    expect(mayRetryWithoutCheckingExecution(null)).toBe(false);
  });

  it('outcomeUnknown 与 idempotencyConflict 必须先核验执行结果', () => {
    for (const code of ['outcomeUnknown', 'idempotencyConflict'] as const) {
      const error = backendError('transport', 'x', parseApiError(apiErrorPayload({ code })));
      expect(requiresOutcomeVerification(error)).toBe(true);
    }
    expect(
      requiresOutcomeVerification(backendError('transport', 'x', parseApiError(apiErrorPayload()))),
    ).toBe(false);
  });

  it('无 apiError 的 BackendError 不报 code，不报 requestId', () => {
    const error = backendError('transport', 'socket closed');
    expect(error.kind).toBe('transport');
    expect(error.apiError).toBeNull();
    expect(apiErrorCode(error)).toBeNull();
    expect(apiErrorRequestId(error)).toBeNull();
  });

  it('未绑定文案是 §7.4 的固定串，可被日志直接搜索', () => {
    expect(UNBOUND_MESSAGE).toBe(
      'BackendClient has not been bound; check that the platform adapter ran.',
    );
  });

  it('资源类 code 仍映射为 never：退避重发必须由调用方按 code 自己决定', () => {
    // 概要 §6.3 的三个取值表达不了「退避后重发」，Rust 侧保守映射为 never。
    for (const code of [
      'resourceBusy',
      'queueFull',
      'rateLimited',
      'serviceUnavailable',
    ] as const) {
      expect(parseApiError(apiErrorPayload({ code }))?.retryDisposition).toBe('never');
    }
  });
});
