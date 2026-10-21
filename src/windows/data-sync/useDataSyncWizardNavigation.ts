import { useCallback, useMemo } from 'react';
import type { DataSyncComparison } from './useDataSyncComparison';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncRunState } from './useDataSyncRunState';
import type { DataSyncWizardState } from './dataSyncTypes';
import { tablesForCompare } from './mappingView';
import { WIZARD_STEPS } from './useDataSyncWizardState';

export interface UseDataSyncWizardNavigationOptions {
  wizard: DataSyncWizardState;
  endpoints: DataSyncEndpoints;
  runState: DataSyncRunState;
  comparison: DataSyncComparison;
}

/**
 * Navigation of the Data Sync wizard.
 *
 * Moving forward from the setup step inspects the endpoints, moving forward
 * from the objects step compares them; both fall back to the previous step
 * when the phase fails.
 */
export function useDataSyncWizardNavigation({
  wizard,
  endpoints,
  runState,
  comparison,
}: UseDataSyncWizardNavigationOptions) {
  const {
    step,
    setStep,
    syncState,
    inspectionComplete,
    mappingResults,
  } = wizard;
  const { sourceId, targetId, sourceDatabase, targetDatabase, activePairing, sourceSessionError, targetSessionError } =
    endpoints;
  const { scopeReconfirmationRequired } = runState;
  const { handleInspect, handleCompare, compared } = comparison;

  const busy = syncState === 'inspecting' || syncState === 'comparing' || syncState === 'executing';
  const compareDisabled = Boolean(sourceSessionError || targetSessionError);
  const stepIndex = WIZARD_STEPS.indexOf(step);

  const canNext = useMemo(() => {
    switch (step) {
      case 'endpoints':
        return Boolean(
          sourceId &&
            targetId &&
            sourceDatabase &&
            targetDatabase &&
            activePairing?.supported &&
            !scopeReconfirmationRequired &&
            !compareDisabled,
        );
      case 'setup':
        return !busy;
      case 'objects':
        return inspectionComplete && tablesForCompare(mappingResults).length > 0 && !busy;
      case 'compare':
        return compared && !busy;
      default:
        return false;
    }
  }, [
    step,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    activePairing,
    scopeReconfirmationRequired,
    compareDisabled,
    busy,
    inspectionComplete,
    mappingResults,
    compared,
  ]);

  const goNext = useCallback(async () => {
    const next = WIZARD_STEPS[stepIndex + 1];
    if (!next) return;

    if (step === 'setup' && next === 'objects') {
      setStep(next);
      const ok = await handleInspect();
      if (!ok) setStep(step);
      return;
    }
    if (step === 'objects' && next === 'compare') {
      setStep(next);
      const ok = await handleCompare();
      if (!ok) setStep(step);
      return;
    }
    setStep(next);
  }, [step, stepIndex, handleInspect, handleCompare]);

  const goBack = useCallback(() => {
    if (busy) return;
    const previous = WIZARD_STEPS[stepIndex - 1];
    if (previous) setStep(previous);
  }, [busy, stepIndex]);

  return { busy, compareDisabled, stepIndex, canNext, goNext, goBack };
}

export type DataSyncWizardNavigation = ReturnType<typeof useDataSyncWizardNavigation>;
