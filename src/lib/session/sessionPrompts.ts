import { ApiError } from '@datazen/backend-client';
import type { DatabaseSwitchResult } from './QueryPanelSession';

/** Deterministic mapping from session lifecycle outcomes to user-facing copy keys. */
export function switchResultPromptKey(result: DatabaseSwitchResult): string | null {
  switch (result.status) {
    case 'switched': return null;
    case 'cancelled': return null;
    case 'conflict': return 'query.session.revisionConflict';
    case 'failed': {
      if (result.error instanceof ApiError && result.error.code === 'SessionLost') {
        return 'query.session.disconnected';
      }
      return 'query.session.switchFailed';
    }
  }
}

export function sessionErrorPromptKey(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.code === 'SessionLost') return 'query.session.disconnected';
    if (error.code === 'ContextConflict') return 'query.session.revisionConflict';
    if (error.code === 'ResourceBusy') return 'query.session.cancelledRetry';
  }
  return 'query.session.executionFailed';
}

export function projectionErrorPromptKey(error: unknown): string {
  if (error instanceof ApiError && error.code === 'SessionLost') return 'query.session.disconnected';
  if (error instanceof Error && /disconnect|lost|network/i.test(error.message)) return 'query.session.disconnected';
  return 'query.session.executionFailed';
}
