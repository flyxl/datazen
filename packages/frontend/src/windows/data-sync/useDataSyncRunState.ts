import { useState, type Dispatch, type SetStateAction } from 'react';
import type { DataSyncExecutionResult } from '../../commands/sync';

/**
 * Outcome state of the current Data Sync run.
 *
 * `writeOutcomeUncertain` is the safety fence: once a write may have landed
 * without a confirmed outcome it stays set until a comparison proves the
 * database is consistent again. `scopeReconfirmationRequired` blocks the
 * wizard after an unknown history run could not restore its endpoint scope.
 */
export interface DataSyncRunState {
  writeOutcomeUncertain: boolean;
  setWriteOutcomeUncertain: Dispatch<SetStateAction<boolean>>;
  scopeReconfirmationRequired: boolean;
  setScopeReconfirmationRequired: Dispatch<SetStateAction<boolean>>;
  lastExecutionResult: DataSyncExecutionResult | null;
  setLastExecutionResult: Dispatch<SetStateAction<DataSyncExecutionResult | null>>;
}

export function useDataSyncRunState(): DataSyncRunState {
  const [writeOutcomeUncertain, setWriteOutcomeUncertain] = useState(false);
  const [scopeReconfirmationRequired, setScopeReconfirmationRequired] = useState(false);
  const [lastExecutionResult, setLastExecutionResult] = useState<DataSyncExecutionResult | null>(
    null,
  );

  return {
    writeOutcomeUncertain,
    setWriteOutcomeUncertain,
    scopeReconfirmationRequired,
    setScopeReconfirmationRequired,
    lastExecutionResult,
    setLastExecutionResult,
  };
}
