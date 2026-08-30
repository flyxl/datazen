import type { DriverCapabilities } from '../types';
import type {
  QueryCancelState,
  QueryExecState,
  QueryTerminalState,
} from '../stores/queryExecActions';

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

/**
 * State transitions are deliberately separate from the cancel IPC result.
 * A successful cancel request only produces `cancel_requested`; the query
 * promise/stream must dispatch the terminal transition later.
 */
export type QueryExecutionTransition =
  | { type: 'start' }
  | { type: 'cancel_requested' }
  | { type: 'cancel_failed'; error: string }
  | { type: 'succeeded' }
  | { type: 'failed'; error: string }
  | { type: 'cancelled' }
  | { type: 'outcome_unknown'; error?: string };

/** Pure reducer shared by streaming and non-streaming execution paths. */
export function reduceQueryExecutionState(
  exec: QueryExecState,
  transition: QueryExecutionTransition,
): QueryExecState {
  switch (transition.type) {
    case 'start':
      return {
        ...exec,
        running: true,
        error: null,
        cancelState: 'idle',
        cancelError: null,
        terminalState: null,
      };
    case 'cancel_requested':
      return exec.running
        ? { ...exec, cancelState: 'requested', cancelError: null }
        : exec;
    case 'cancel_failed':
      return exec.running
        ? { ...exec, cancelState: 'failed', cancelError: transition.error }
        : exec;
    case 'succeeded':
      return {
        ...exec,
        running: false,
        error: null,
        cancelState: 'idle',
        cancelError: null,
        terminalState: 'succeeded',
      };
    case 'failed':
      return {
        ...exec,
        running: false,
        error: transition.error,
        cancelState: 'idle',
        cancelError: null,
        terminalState: 'failed',
      };
    case 'cancelled':
      return {
        ...exec,
        running: false,
        error: null,
        cancelState: 'idle',
        cancelError: null,
        terminalState: 'cancelled',
      };
    case 'outcome_unknown':
      return {
        ...exec,
        running: false,
        error: transition.error ?? exec.error,
        cancelState: 'idle',
        terminalState: 'unknown',
      };
  }
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
  if (error) return 'failed';
  return null;
}

function cancelActionStateFor(
  phase: QueryPhase,
  cancelCapability: CancelCapability,
  cancelState: QueryCancelState,
): CancelActionState {
  if (cancelCapability !== 'supported' || !['running', 'cancel_requested'].includes(phase)) {
    return 'unavailable';
  }
  if (phase === 'cancel_requested' || cancelState === 'requested') return 'requested';
  if (cancelState === 'failed') return 'failed';
  return 'available';
}

/** Return the user-action state for the Cancel control without side effects. */
export function getCancelActionState(viewModel: QueryExecutionViewModel): CancelActionState {
  if (
    viewModel.cancelCapability !== 'supported' ||
    !['running', 'cancel_requested'].includes(viewModel.phase)
  ) {
    return 'unavailable';
  }
  if (viewModel.phase === 'cancel_requested' || viewModel.cancelState === 'requested') {
    return 'requested';
  }
  if (viewModel.cancelState === 'failed') return 'failed';
  return 'available';
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

  const cancelState = cancelActionStateFor(phase, cancelCapability, exec.cancelState);

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
