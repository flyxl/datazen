/**
 * Execution state management for statement frame and gutter extensions.
 *
 * Provides a StateField that tracks the current execution status, the
 * target range, and the document version. Consumers dispatch effects
 * to start/cancel/finish execution; the field exposes a stable snapshot.
 *
 * §Track S4-A step 6: running spinner matched by documentVersion + targetRange.
 */
import { StateField, StateEffect } from '@codemirror/state';
import type { EditorExecutionState } from './types';

// ── Effects ──────────────────────────────────────────────────────────────

/** Start execution — transitions from idle to running. */
export const StartExecutionEffect = StateEffect.define<{
  targetRange: { from: number; to: number } | null;
  documentVersion: number;
}>();

/** Cancel execution — transitions from running to cancelling. */
export const CancelExecutionEffect = StateEffect.define<void>();

/** Finish execution — transitions from running/cancelling to idle. */
export const FinishExecutionEffect = StateEffect.define<void>();

// ── State ────────────────────────────────────────────────────────────────

/** Initial state. */
export const INITIAL_EXECUTION_STATE: EditorExecutionState = {
  status: 'idle',
  targetRange: null,
  documentVersion: null,
};

/**
 * CodeMirror StateField holding the current execution state.
 *
 * Read with `state.field(executionStateField)`.
 */
export const executionStateField = StateField.define<EditorExecutionState>({
  create() {
    return { ...INITIAL_EXECUTION_STATE };
  },

  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(StartExecutionEffect)) {
        return {
          status: 'running',
          targetRange: effect.value.targetRange,
          documentVersion: effect.value.documentVersion,
        };
      }
      if (effect.is(CancelExecutionEffect)) {
        return {
          ...value,
          status: 'cancelling',
        };
      }
      if (effect.is(FinishExecutionEffect)) {
        return { ...INITIAL_EXECUTION_STATE };
      }
    }
    return value;
  },
});

// ── Pure helpers ─────────────────────────────────────────────────────────

/**
 * Pure: check if the given range matches the current running target.
 *
 * §Track S4-A step 6: spinner only shown when both documentVersion
 * and targetRange match exactly.
 */
export function matchesRunningTarget(
  state: EditorExecutionState,
  documentVersion: number,
  range: { from: number; to: number },
): boolean {
  if (state.status !== 'running') return false;
  if (state.documentVersion !== documentVersion) return false;
  if (!state.targetRange) return false;
  return state.targetRange.from === range.from && state.targetRange.to === range.to;
}

/**
 * Pure: check if the execution has been cancelled or finished (i.e. no longer running).
 */
export function isIdleOrCancelled(state: EditorExecutionState): boolean {
  return state.status === 'idle' || state.status === 'cancelling';
}
