/**
 * Onboarding wizard state machine.
 *
 * Steps: S0 (welcome / entry choice) → S1 (step 1 of 2) → S2 (step 2 of 2) → S3 (done).
 *
 * Step 1 is *entry specific* — it renders the inline import form for the import
 * entry, the connection form for the manual entry and the sample dataset panel
 * for the sample entry. Step 2 is *always* the AI provider step, whichever
 * entry the user picked in S0.
 *
 * Q2: Skip is visible at every step except S3.
 * Q5: onboarding state is versioned: `{ completed: boolean; version: number }`.
 */

export type Step = 's0' | 's1' | 's2' | 's3';
export type EntryType = 'manual' | 'import' | 'sample';

/** Which screen step 1 renders for the chosen entry. */
export type StepOneVariant = EntryType;

export interface ImportSummary {
  imported: number;
  /** Existing connections replaced by the import (same id). */
  overwritten: number;
  source: string;
}

export interface WizardState {
  step: Step;
  entry: EntryType;
  /** Result of the inline import (import entry only). */
  importResult?: ImportSummary;
  /** Seeded sample database path (sample entry only). */
  samplePath?: string;
  /** Name of the connection created in step 1 (manual / sample entries). */
  connectionName?: string;
  /** Provider label configured in step 2, `null` when AI was skipped. */
  aiLabel: string | null;
  /** Step 2 happened (used by the done summary). */
  aiConfigured: boolean;
}

export type WizardAction =
  | { type: 'ENTER_MANUAL' }
  | { type: 'ENTER_IMPORT' }
  | { type: 'ENTER_SAMPLE' }
  | { type: 'IMPORT_SUCCESS'; result: ImportSummary }
  | { type: 'SAMPLE_READY'; path: string; connectionName: string }
  | { type: 'SAMPLE_FAILED' }
  | { type: 'SAVE_SUCCESS'; connectionName: string }
  | { type: 'CONTINUE' }
  | { type: 'FINISH'; aiLabel: string | null }
  | { type: 'SKIP' }
  | { type: 'BACK' };

export const INITIAL_STATE: WizardState = {
  step: 's0',
  entry: 'manual',
  aiLabel: null,
  aiConfigured: false,
};

/**
 * Pure reducer for wizard transitions.
 *
 * Transition map:
 *   S0 + ENTER_MANUAL → S1 (connection form)
 *   S0 + ENTER_IMPORT → S1 (inline import form)
 *   S0 + ENTER_SAMPLE → S1 (sample dataset panel)
 *   S1 + SAVE_SUCCESS (manual) → S2
 *   S1 + CONTINUE (import / sample, only when ready) → S2
 *   S2 + FINISH → S3
 *   * + SKIP → S3 (nothing configured)
 *   S1 + BACK → S0
 *   S2 + BACK → S1
 */
export function wizardReducer(state: WizardState, action: WizardAction): WizardState {
  switch (action.type) {
    case 'ENTER_MANUAL':
      return { ...state, step: 's1', entry: 'manual' };
    case 'ENTER_IMPORT':
      return { ...state, step: 's1', entry: 'import' };
    case 'ENTER_SAMPLE':
      return { ...state, step: 's1', entry: 'sample' };
    case 'IMPORT_SUCCESS':
      return { ...state, entry: 'import', importResult: action.result };
    case 'SAMPLE_READY':
      // Idempotent: the sample step may report the same result more than once
      // (StrictMode remount). Returning a new object every time would re-render
      // the shell in a loop.
      if (
        state.entry === 'sample' &&
        state.samplePath === action.path &&
        state.connectionName === action.connectionName
      ) {
        return state;
      }
      return {
        ...state,
        entry: 'sample',
        samplePath: action.path,
        connectionName: action.connectionName,
      };
    case 'SAMPLE_FAILED':
      return { ...state, entry: 'sample', samplePath: undefined };
    case 'SAVE_SUCCESS':
      // The connection form's Save button is the manual entry's way forward.
      return { ...state, connectionName: action.connectionName, step: 's2' };
    case 'CONTINUE': {
      if (state.step !== 's1' || !canContinueFromStepOne(state)) return state;
      return { ...state, step: 's2' };
    }
    case 'FINISH':
      return {
        ...state,
        step: 's3',
        aiLabel: action.aiLabel,
        aiConfigured: action.aiLabel !== null,
      };
    case 'SKIP':
      return { ...state, step: 's3', aiLabel: null, aiConfigured: false };
    case 'BACK': {
      if (state.step === 's2') return { ...state, step: 's1' };
      if (state.step === 's1') return { ...state, step: 's0' };
      return state;
    }
    default:
      return state;
  }
}

/** Step 1 is complete: the manual form saved, the import succeeded or the sample is seeded. */
export function canLeaveStepOne(state: WizardState): boolean {
  switch (state.entry) {
    case 'import':
      return importedCount(state) > 0;
    case 'sample':
      return Boolean(state.samplePath);
    case 'manual':
      return true;
  }
}

/**
 * How many connections the import actually wrote: brand-new ones plus existing
 * ones it replaced. Re-importing an export of the same machine only overwrites,
 * and that still counts as a completed step 1.
 */
export function importedCount(state: WizardState): number {
  const result = state.importResult;
  if (!result) return 0;
  return result.imported + (result.overwritten ?? 0);
}

/**
 * Whether the footer's Continue button advances out of step 1.
 *
 * The manual entry has no Continue: its own Save button is the single way
 * forward (the connection must be persisted before the AI step).
 */
export function canContinueFromStepOne(state: WizardState): boolean {
  return state.entry !== 'manual' && canLeaveStepOne(state);
}

/**
 * Step dots: only step 1 and step 2 exist, and the indicator is hidden on the
 * entry screen (S0) and the done screen (S3).
 */
export function stepIndexFor(step: Step): 0 | 1 | 2 {
  if (step === 's1') return 1;
  if (step === 's2') return 2;
  return 0;
}

/** i18n key of the step indicator label. */
export function stepLabelKey(step: Step): 'onboarding.s1.stepLabel' | 'onboarding.s2.stepLabel' {
  return step === 's2' ? 'onboarding.s2.stepLabel' : 'onboarding.s1.stepLabel';
}

/** How the done screen's connection row is filled in. */
export type ConnectionSummary =
  | { kind: 'imported'; imported: number; overwritten: number; source: string }
  | { kind: 'connection'; name: string }
  | { kind: 'none' };

export function connectionSummary(state: WizardState): ConnectionSummary {
  if (state.entry === 'import' && importedCount(state) > 0) {
    return {
      kind: 'imported',
      imported: state.importResult?.imported ?? 0,
      overwritten: state.importResult?.overwritten ?? 0,
      source: state.importResult?.source ?? '',
    };
  }
  if (state.connectionName) return { kind: 'connection', name: state.connectionName };
  return { kind: 'none' };
}
