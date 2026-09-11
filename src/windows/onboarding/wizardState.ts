/**
 * Onboarding wizard state machine.
 *
 * Steps: S0 (welcome) → S1 (connect) → S2 (AI) → S3 (done).
 * Entry types: manual, import, sample — determine which steps are shown.
 *
 * Q2: Skip is visible at every step except S3.
 * Q5: onboarding state is versioned: `{ completed: boolean; version: number }`.
 */

export type Step = 's0' | 's1' | 's2' | 's3';
export type EntryType = 'manual' | 'import' | 'sample';

export interface WizardState {
  step: Step;
  entry: EntryType;
  importResult?: { imported: number; source: string };
  tested: boolean;
  validated: boolean | null;
  selectedDriverId: string;
  selectedProviderId: string;
}

export type WizardAction =
  | { type: 'ENTER_MANUAL' }
  | { type: 'IMPORT_SUCCESS'; result: { imported: number; source: string } }
  | { type: 'SAMPLE' }
  | { type: 'SAVE_SUCCESS' }
  | { type: 'FINISH' }
  | { type: 'SKIP' }
  | { type: 'BACK' };

export const INITIAL_STATE: WizardState = {
  step: 's0',
  entry: 'manual',
  tested: false,
  validated: null,
  selectedDriverId: 'postgres',
  selectedProviderId: 'open_ai',
};

/**
 * Pure reducer for wizard transitions.
 *
 * Transition map:
 *   S0 + ENTER_MANUAL → S1
 *   S0 + IMPORT_SUCCESS → S2
 *   S0 + SAMPLE → S3
 *   S1 + SAVE_SUCCESS → S2
 *   S2 + FINISH → S3
 *   * + SKIP → S3 (validated=null)
 *   S1 + BACK → S0
 *   S2 + BACK (import) → S0
 *   S2 + BACK (manual) → S1
 */
export function wizardReducer(
  state: WizardState,
  action: WizardAction,
): WizardState {
  switch (action.type) {
    case 'ENTER_MANUAL':
      return { ...state, step: 's1', entry: 'manual' };
    case 'IMPORT_SUCCESS':
      return {
        ...state,
        step: 's2',
        entry: 'import',
        importResult: action.result,
      };
    case 'SAMPLE':
      return { ...state, step: 's3', entry: 'sample' };
    case 'SAVE_SUCCESS':
      return { ...state, step: 's2', tested: true };
    case 'FINISH':
      return { ...state, step: 's3' };
    case 'SKIP':
      return { ...state, step: 's3', validated: null };
    case 'BACK': {
      if (state.step === 's2' && state.entry === 'import')
        return { ...state, step: 's0' };
      if (state.step === 's2') return { ...state, step: 's1' };
      if (state.step === 's1') return { ...state, step: 's0' };
      return state;
    }
    default:
      return state;
  }
}
