import type { DriverCapabilities } from '../types';
import type { QueryExecState, QueryTerminalState } from '../stores/queryExecActions';

export type QueryPhase =
  | 'idle'
  | 'running'
  | 'cancel_requested'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'outcome_unknown';

export type CancelCapability = 'supported' | 'unsupported' | 'unknown';
export type CancelActionState = 'available' | 'requested' | 'unavailable' | 'failed';

export interface QueryExecutionViewModel {
  phase: QueryPhase;
  cancelCapability: CancelCapability;
  cancelState: CancelActionState;
  elapsedMs: number | null;
  rowCount: number | null;
  affectedRows: number | null;
  error: string | null;
}

export function getCancelCapability(
  capabilities: DriverCapabilities | null | undefined,
): CancelCapability {
  if (!capabilities) return 'unknown';
  return capabilities.supportsCancelQuery ? 'supported' : 'unsupported';
}

export function isCancellationError(message: string | null | undefined): boolean {
  if (!message) return false;
  return /cancel(?:led|ed)?|user.?request|abort(?:ed)?|interrupt(?:ed)?/i.test(message);
}

function terminalPhase(
  terminalState: QueryTerminalState | null | undefined,
  error: string | null,
): QueryPhase | null {
  if (terminalState === 'succeeded') return 'succeeded';
  if (terminalState === 'cancelled') return 'cancelled';
  if (terminalState === 'failed') return 'failed';
  if (terminalState === 'unknown') return 'outcome_unknown';
  if (isCancellationError(error)) return 'cancelled';
  if (error) return 'failed';
  return null;
}

export function toQueryExecutionViewModel(
  exec: QueryExecState,
  capabilities: DriverCapabilities | null | undefined,
): QueryExecutionViewModel {
  const cancelCapability = getCancelCapability(capabilities);
  const error = exec.cancelError ?? exec.error;

  let phase = terminalPhase(exec.terminalState, exec.error);
  if (exec.running) {
    phase = exec.cancelState === 'requested' ? 'cancel_requested' : 'running';
  } else if (!phase) {
    phase = 'idle';
  }

  let cancelState: CancelActionState = 'unavailable';
  if (cancelCapability === 'supported' && exec.running) {
    if (exec.cancelState === 'requested') cancelState = 'requested';
    else if (exec.cancelState === 'failed') cancelState = 'failed';
    else cancelState = 'available';
  }

  const activeResult = exec.results[exec.activeResultIdx];
  const rowCount = activeResult?.rows.length ?? null;
  const affectedRows = activeResult?.rowsAffected ?? null;

  return {
    phase,
    cancelCapability,
    cancelState,
    elapsedMs: exec.executionTimeMs,
    rowCount,
    affectedRows,
    error,
  };
}
