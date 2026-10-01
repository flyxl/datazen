import { describe, expect, it } from 'vitest';

import { ApiError, defaultRetryDisposition, deserializeApiError } from '../src/index';

describe('deserializeApiError', () => {
  it('reads a well-formed §6.3 payload verbatim', () => {
    const error = deserializeApiError({
      code: 'SessionQuotaExceeded',
      message: 'too many open sessions',
      requestId: 'req-77',
      retryDisposition: 'never',
    });

    expect(error).toBeInstanceOf(ApiError);
    expect(error.toPayload()).toEqual({
      code: 'SessionQuotaExceeded',
      message: 'too many open sessions',
      requestId: 'req-77',
      retryDisposition: 'never',
    });
  });

  it('derives retryDisposition when the backend omitted it', () => {
    const error = deserializeApiError({ code: 'ResourceBusy', message: 'busy', requestId: 'r' });

    expect(error.retryDisposition).toBe(defaultRetryDisposition('ResourceBusy'));
    expect(error.retryDisposition).toBe('safeRead');
  });

  it('rejects a retryDisposition that is not one of the three', () => {
    const error = deserializeApiError({
      code: 'RateLimited',
      message: 'slow down',
      requestId: 'r',
      retryDisposition: 'retryForever',
    });

    expect(error.retryDisposition).toBe('safeRead');
  });

  it('reports an unknown code as ServiceUnavailable instead of passing it through', () => {
    const error = deserializeApiError({ code: 'SomethingNobodyDefined', message: 'boom' });

    // A frontend `switch` cannot handle an unregistered code, so it is classified
    // rather than forwarded.
    expect(error.code).toBe('ServiceUnavailable');
  });

  it('keeps the original payload as the cause', () => {
    const raw = { code: 'NotFound', message: 'missing', requestId: 'r-9' };
    const error = deserializeApiError(raw);

    expect(error.cause).toBe(raw);
  });

  it('wraps a plain Error as ServiceUnavailable and preserves the message', () => {
    const raw = new Error('socket closed');
    const error = deserializeApiError(raw);

    expect(error.code).toBe('ServiceUnavailable');
    expect(error.message).toBe('socket closed');
    expect(error.cause).toBe(raw);
  });

  it('survives a non-object payload without throwing', () => {
    for (const raw of [null, undefined, 'nope', 42, []]) {
      const error = deserializeApiError(raw);
      expect(error).toBeInstanceOf(ApiError);
      expect(error.code).toBe('ServiceUnavailable');
    }
  });

  it('returns an existing ApiError unchanged, so requestId survives a re-wrap', () => {
    const original = new ApiError('PermissionDenied', 'no', { requestId: 'r-3' });
    expect(deserializeApiError(original)).toBe(original);
  });
});

describe('ApiError retry semantics', () => {
  it('treats a rejected request as transient only when resending is safe', () => {
    expect(new ApiError('ServiceUnavailable', 'down').isTransient()).toBe(true);
    expect(new ApiError('PermissionDenied', 'nope').isTransient()).toBe(false);
  });

  it('never marks an accepted request as safe to resend', () => {
    // The request was accepted; its outcome must be read from getExecution.
    expect(new ApiError('OutcomeUnknown', 'unknown').retryDisposition).toBe('checkExecution');
    expect(new ApiError('OutcomeUnknown', 'unknown').isTransient()).toBe(false);
  });

  it('never resends a request whose result is already known-good', () => {
    expect(new ApiError('IdempotencyConflict', 'reused key').retryDisposition).toBe('never');
    expect(new ApiError('ConfigRevisionMismatch', 'stale').retryDisposition).toBe('never');
  });
});