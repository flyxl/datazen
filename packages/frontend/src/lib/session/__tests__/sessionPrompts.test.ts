import { describe, expect, it } from 'vitest';
import { ApiError, type SessionView } from '@datazen/backend-client';
import {
  needsTransactionSwitchConfirm,
  projectionErrorPromptKey,
  sessionErrorPromptKey,
  switchResultPromptKey,
} from '../sessionPrompts';

function view(state: SessionView['state']): SessionView {
  return { state } as SessionView;
}

describe('[tester] session prompt mapping', () => {
  it('stays silent for a completed or user-cancelled switch', () => {
    expect(switchResultPromptKey({ status: 'switched', session: view('ready') })).toBeNull();
    expect(switchResultPromptKey({ status: 'cancelled' })).toBeNull();
  });

  it('maps a revision conflict and a lost session to their own copy', () => {
    expect(switchResultPromptKey({ status: 'conflict', session: null })).toBe(
      'query.session.revisionConflict',
    );
    expect(
      switchResultPromptKey({
        status: 'failed',
        error: new ApiError('SessionLost', 'gone'),
      }),
    ).toBe('query.session.disconnected');
    expect(
      switchResultPromptKey({
        status: 'failed',
        error: new ApiError('ResourceBusy', 'busy'),
      }),
    ).toBe('query.session.switchFailed');
  });

  it('maps execution and projection failures to the right copy', () => {
    expect(sessionErrorPromptKey(new ApiError('ResourceBusy', 'busy'))).toBe(
      'query.session.cancelledRetry',
    );
    expect(sessionErrorPromptKey(new ApiError('ContextConflict', 'stale'))).toBe(
      'query.session.revisionConflict',
    );
    expect(sessionErrorPromptKey(new Error('boom'))).toBe('query.session.executionFailed');
    expect(projectionErrorPromptKey(new Error('socket disconnected'))).toBe(
      'query.session.disconnected',
    );
    expect(projectionErrorPromptKey(new Error('syntax error'))).toBe(
      'query.session.executionFailed',
    );
  });

  it('confirms a transaction switch only while the session is still alive', () => {
    expect(needsTransactionSwitchConfirm(view('ready'))).toBe(true);
    expect(needsTransactionSwitchConfirm(view('executing'))).toBe(true);
    expect(needsTransactionSwitchConfirm(view('reconfiguring'))).toBe(true);
    expect(needsTransactionSwitchConfirm(view('closed'))).toBe(false);
    expect(needsTransactionSwitchConfirm(view('lost'))).toBe(false);
  });
});